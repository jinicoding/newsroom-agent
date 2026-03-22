//! Article writing & editing command handlers (기사작성·편집 도메인)
//! Commands: /anonymize, /archive, /article, /checklist, /draft, /export, /headline, /improve, /legal, /multiformat, /proofread, /publish, /quote, /readability, /rewrite, /stats, /summary, /translate

use crate::commands::auto_compact_if_needed;
use crate::commands_project::*;
use crate::commands_research::{ensure_sources_dir_at, load_sources};
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

// ── /article ────────────────────────────────────────────────────────────

pub async fn handle_article(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let raw_args = input
        .strip_prefix("/article")
        .unwrap_or("")
        .trim();

    let (article_type, topic) = parse_article_args(raw_args);
    let topic = topic.as_str();

    // Search for related research files
    let research = find_related_research(topic);
    if !research.is_empty() {
        println!(
            "{GREEN}  📎 관련 리서치 {}건 발견{RESET}",
            research.len()
        );
        for (filename, _) in &research {
            println!("     - {filename}");
        }
        println!();
    }

    let (prompt, _) = build_article_prompt(topic, &research, article_type.as_deref());

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save draft to file if a topic was provided and we got a response
    if !topic.is_empty() && !response.trim().is_empty() {
        let path = draft_file_path(topic);
        match save_article_draft(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 초안 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  초안 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /checklist ───────────────────────────────────────────────────────────

const CHECKLIST_DIR: &str = ".journalist/checklist";

/// Parse `/checklist` input to extract `--file <path>` and inline text.
/// Returns `(Option<file_path>, remaining_text)`.
pub fn parse_checklist_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    if let Some(rest) = args.strip_prefix("--file") {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return (None, String::new());
        }
        let mut path_end = rest.len();
        for (i, ch) in rest.char_indices() {
            if ch.is_whitespace() {
                path_end = i;
                break;
            }
        }
        let file_path = rest[..path_end].to_string();
        let remaining = rest[path_end..].trim().to_string();
        (Some(file_path), remaining)
    } else {
        (None, args.to_string())
    }
}

/// Build the prompt for the `/checklist` command (pre-publication article checklist).
pub fn build_checklist_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 기사 초안에 대해 출고 전 체크리스트를 점검해주세요.\n\n\
         다음 6개 항목을 각각 검토하고, 항목별로 ✅ (통과) 또는 ❌ (미흡) 판정을 내려주세요:\n\n\
         ## 점검 항목\n\n\
         ### 1. 육하원칙 (5W1H) 충족 여부\n\
         - 누가(Who), 무엇을(What), 언제(When), 어디서(Where), 왜(Why), 어떻게(How)가 모두 포함되어 있는지 확인\n\
         - 누락된 요소가 있으면 구체적으로 지적\n\n\
         ### 2. 출처 명시 확인\n\
         - 모든 주요 사실에 출처가 명시되어 있는지 확인\n\
         - 출처 없는 주장이나 수치가 있으면 지적\n\n\
         ### 3. 중립성/균형 보도 여부\n\
         - 한쪽 시각에 치우치지 않았는지 확인\n\
         - 반대 의견이나 다른 시각이 필요한 부분 지적\n\n\
         ### 4. [확인 필요] 태그 잔존 확인\n\
         - 기사 내 [확인 필요], [TODO], [TBD] 등 미완성 태그가 남아있는지 확인\n\
         - 발견 시 해당 위치와 내용을 명시\n\n\
         ### 5. 법적 리스크 (명예훼손, 초상권 등)\n\
         - 명예훼손 소지가 있는 표현 확인\n\
         - 초상권, 개인정보 노출 우려 확인\n\
         - 저작권 침해 소지 확인\n\n\
         ### 6. 숫자/날짜 일관성\n\
         - 기사 내 숫자, 날짜, 통계가 서로 모순되지 않는지 확인\n\
         - 단위 표기가 일관적인지 확인\n\n\
         ## 결과 형식\n\n\
         각 항목별로 판정(✅/❌)과 상세 설명을 제시하고,\n\
         마지막에 **종합 판정**과 **출고 전 수정 권고사항**을 정리해주세요.\n\n\
         ## 기사 초안\n\n\
         {article}"
    ))
}

/// Build the checklist file path: `.journalist/checklist/YYYY-MM-DD_<slug>.md`
pub fn checklist_file_path(source: &str) -> std::path::PathBuf {
    checklist_file_path_with_date(source, &today_str())
}

/// Build the checklist file path with an explicit date string (for testing).
pub fn checklist_file_path_with_date(source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_checklist.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(CHECKLIST_DIR).join(filename)
}

/// Save checklist result to file. Creates the checklist directory if needed.
fn save_checklist(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/checklist` command: pre-publication article validation.
pub async fn handle_checklist(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/checklist").unwrap_or("").trim();
    let (file_path, inline_text) = parse_checklist_args(args);

    // Read article content from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_checklist_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /checklist <기사 초안 텍스트>{RESET}");
            println!("{DIM}  또는:   /checklist --file <경로>{RESET}");
            println!("{DIM}  예시:   /checklist --file draft.md{RESET}");
            println!(
                "{DIM}  기사 초안을 출고 전 6개 항목(육하원칙, 출처, 중립성, 태그, 법적 리스크, 숫자/날짜)으로 점검합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save checklist result to .journalist/checklist/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "checklist".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "checklist".to_string()
            } else {
                preview
            }
        };
        let path = checklist_file_path(&slug_source);
        match save_checklist(&path, &response) {
            Ok(_) => {
                println!("{GREEN}  ✓ 체크리스트 저장: {}{RESET}\n", path.display());
            }
            Err(e) => {
                eprintln!("{RED}  체크리스트 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /translate ───────────────────────────────────────────────────────────

const TRANSLATE_DIR: &str = ".journalist/translate";

/// Parse `/translate` arguments: extract `--file <path>` and inline text.
/// Returns `(Option<file_path>, remaining_text)`.
pub fn parse_translate_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    if let Some(rest) = args.strip_prefix("--file") {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return (None, String::new());
        }
        let mut path_end = rest.len();
        for (i, ch) in rest.char_indices() {
            if ch.is_whitespace() {
                path_end = i;
                break;
            }
        }
        let file_path = rest[..path_end].to_string();
        let remaining = rest[path_end..].trim().to_string();
        (Some(file_path), remaining)
    } else {
        (None, args.to_string())
    }
}

/// Build the prompt for `/translate`: localize foreign news for Korean readers.
pub fn build_translate_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 외신 기사를 **한국 독자**를 위해 번역·현지화해주세요.\n\n\
         ## 번역 지침\n\n\
         1. **단순 직역이 아닌 현지화 번역**: 한국 독자가 맥락을 이해할 수 있도록 배경 설명을 추가하세요.\n\
         2. **고유명사 현지화**: 인물명은 한글 표기(원어 병기), 기관명은 통용 한글명 사용.\n\
         3. **단위 변환**: 달러→원화 환산(괄호 병기), 마일→킬로미터, 화씨→섭씨 등.\n\
         4. **한국 관련성 부각**: 한국 경제·사회에 미치는 영향이 있다면 별도 문단으로 추가.\n\
         5. **문체**: 한국 신문 기사체(경어체, 역피라미드 구조) 사용.\n\
         6. **출처 표기**: 원문 매체명과 기자명을 기사 끝에 명시.\n\n\
         ## 출력 형식\n\n\
         ```\n\
         # [번역 제목]\n\n\
         [번역된 기사 본문]\n\n\
         ## 한국 독자 참고사항\n\
         (한국과의 관련성, 추가 맥락 설명)\n\n\
         ## 주요 용어\n\
         | 원문 | 번역 | 설명 |\n\
         |------|------|------|\n\n\
         ---\n\
         원문: [매체명], [기자명]\n\
         ```\n\n\
         ---\n\n\
         ## 원문 기사\n\n\
         {article}"
    ))
}

/// Build the translate file path using today's date.
pub fn translate_file_path(slug_source: &str) -> std::path::PathBuf {
    translate_file_path_with_date(slug_source, &today_str())
}

/// Build the translate file path with an explicit date string (for testing).
pub fn translate_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_translate.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(TRANSLATE_DIR).join(filename)
}

/// Save translate result to file. Creates the translate directory if needed.
fn save_translate(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/translate` command: translate and localize foreign articles for Korean readers.
pub async fn handle_translate(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/translate").unwrap_or("").trim();
    let (file_path, inline_text) = parse_translate_args(args);

    // Read article from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_translate_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /translate <외신 기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /translate --file <경로>{RESET}");
            println!("{DIM}  예시:   /translate --file reuters_article.txt{RESET}");
            println!(
                "{DIM}  외신 기사를 한국 독자용으로 번역·현지화합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save translation to .journalist/translate/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "translate".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "translate".to_string()
            } else {
                preview
            }
        };
        let path = translate_file_path(&slug_source);
        match save_translate(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 번역 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  번역 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /headline ────────────────────────────────────────────────────────────

const HEADLINE_DIR: &str = ".journalist/headline";

/// Parse `/headline` arguments: supports `--file <path>` and inline text.
/// Returns (Option<file_path>, inline_text).
pub fn parse_headline_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    if let Some(rest) = args.strip_prefix("--file") {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return (None, String::new());
        }
        let mut path_end = rest.len();
        for (i, ch) in rest.char_indices() {
            if ch.is_whitespace() {
                path_end = i;
                break;
            }
        }
        let file_path = rest[..path_end].to_string();
        let remaining = rest[path_end..].trim().to_string();
        (Some(file_path), remaining)
    } else {
        (None, args.to_string())
    }
}

/// Build the prompt for `/headline`: generate 5–7 headline candidates in various styles.
pub fn build_headline_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 기사 초안(또는 주제)을 읽고, **한국 신문 스타일의 헤드라인 후보 5~7개**를 생성해주세요.\n\n\
         ## 헤드라인 스타일 (각 스타일별 최소 1개)\n\n\
         1. **스트레이트**: 핵심 사실을 간결하게 전달. 주어+동사 구조.\n\
         2. **분석**: 맥락·의미를 담은 헤드라인. '~의 의미', '~이 뜻하는 것' 등.\n\
         3. **피처**: 독자의 호기심을 자극하는 내러티브형. 인물·장면 중심.\n\
         4. **클릭유도**: 숫자·질문·강한 표현으로 클릭을 유도. 단, 낚시성 지양.\n\n\
         ## 한국 신문 헤드라인 관습\n\n\
         - **간결함**: 15~25자 내외 (공백 포함)\n\
         - **핵심 동사**: 능동형 동사로 끝맺음 ('~했다', '~한다', '~나서' 등)\n\
         - **숫자 활용**: 구체적 수치가 있으면 적극 활용\n\
         - **따옴표**: 인용이나 강조 시 홑따옴표('') 사용\n\
         - **말줄임표**: 긴장감이나 반전에 '…' 활용 가능\n\
         - **주어 생략**: 문맥상 명확하면 주어 생략 가능\n\n\
         ## 출력 형식\n\n\
         각 헤드라인에 스타일 태그를 붙여주세요:\n\
         ```\n\
         [스트레이트] 헤드라인 텍스트\n\
         [분석] 헤드라인 텍스트\n\
         [피처] 헤드라인 텍스트\n\
         [클릭유도] 헤드라인 텍스트\n\
         ```\n\n\
         각 헤드라인 아래에 한 줄로 **선택 이유**를 간단히 설명해주세요.\n\n\
         ---\n\n\
         기사 초안/주제:\n\n{article}"
    ))
}

/// Build headline file path with today's date.
pub fn headline_file_path(slug_source: &str) -> std::path::PathBuf {
    headline_file_path_with_date(slug_source, &today_str())
}

/// Build headline file path with an explicit date string (for testing).
pub fn headline_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_headline.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(HEADLINE_DIR).join(filename)
}

/// Save headline result to file. Creates the headline directory if needed.
fn save_headline(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/headline` command: generate headline candidates for an article draft or topic.
pub async fn handle_headline(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/headline").unwrap_or("").trim();
    let (file_path, inline_text) = parse_headline_args(args);

    // Read article from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_headline_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /headline <기사 초안 또는 주제>{RESET}");
            println!("{DIM}  또는:   /headline --file <경로>{RESET}");
            println!("{DIM}  예시:   /headline 삼성전자 1분기 영업이익 전년 대비 30% 증가{RESET}");
            println!(
                "{DIM}  기사 초안이나 주제에 맞는 헤드라인 후보 5~7개를 생성합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save headline to .journalist/headline/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "headline".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "headline".to_string()
            } else {
                preview
            }
        };
        let path = headline_file_path(&slug_source);
        match save_headline(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 헤드라인 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  헤드라인 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /rewrite ─────────────────────────────────────────────────────────────

const REWRITE_DIR: &str = ".journalist/drafts";

/// Parse `/rewrite` arguments: supports `--style`, `--length`, `--file`, and inline text.
/// Returns (Option<style>, Option<length>, Option<file_path>, inline_text).
pub fn parse_rewrite_args(args: &str) -> (Option<String>, Option<String>, Option<String>, String) {
    let args = args.trim();
    let mut style: Option<String> = None;
    let mut length: Option<String> = None;
    let mut file_path: Option<String> = None;
    let mut remaining_parts: Vec<String> = Vec::new();

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "--style" => {
                if i + 1 < tokens.len() {
                    style = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--length" => {
                if i + 1 < tokens.len() {
                    length = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--file" => {
                if i + 1 < tokens.len() {
                    file_path = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            other => {
                remaining_parts.push(other.to_string());
                i += 1;
            }
        }
    }

    (style, length, file_path, remaining_parts.join(" "))
}

/// Build the prompt for `/rewrite`: rewrite an article in a different style/tone.
pub fn build_rewrite_prompt(
    article: &str,
    style: Option<&str>,
    length: Option<&str>,
) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    let style_name = style.unwrap_or("스트레이트");
    let style_desc = match style_name {
        "스트레이트" | "straight" => {
            "**스트레이트**: 역피라미드 구조. 핵심 사실을 첫 문단에 배치. 객관적이고 간결한 문체."
        }
        "피처" | "feature" => {
            "**피처**: 내러티브형 구조. 인물·장면 묘사로 시작. 독자의 감정에 호소하는 문체."
        }
        "칼럼" | "column" | "opinion" => {
            "**칼럼/오피니언**: 필자의 시각과 분석이 담긴 논평형. 주장-근거-결론 구조."
        }
        "요약" | "summary" => {
            "**요약**: 핵심 사실만 간추린 브리핑형. 불릿포인트 활용 가능. 최대한 압축."
        }
        "sns" | "SNS" | "소셜" => {
            "**SNS**: 소셜미디어에 적합한 짧고 임팩트 있는 문체. 이모지 활용 가능. 핵심만 전달."
        }
        other => {
            // Allow custom style descriptions
            return Some(format!(
                "아래 기사를 **{other}** 스타일로 재작성해주세요.\n\n\
                 {length_instruction}\n\n\
                 ## 재작성 규칙\n\n\
                 - 원문의 핵심 사실과 정보를 정확히 유지\n\
                 - 인용문은 그대로 보존\n\
                 - 숫자·고유명사의 정확성 유지\n\
                 - 원문에 없는 사실을 추가하지 않음\n\n\
                 ## 원문\n\n{article}",
                length_instruction = length_instruction(length),
            ));
        }
    };

    Some(format!(
        "아래 기사를 다음 스타일로 재작성해주세요.\n\n\
         ## 목표 스타일\n\n\
         {style_desc}\n\n\
         {length_instruction}\n\n\
         ## 재작성 규칙\n\n\
         - 원문의 핵심 사실과 정보를 정확히 유지\n\
         - 인용문은 그대로 보존\n\
         - 숫자·고유명사의 정확성 유지\n\
         - 원문에 없는 사실을 추가하지 않음\n\
         - 문단 구조와 흐름을 목표 스타일에 맞게 재구성\n\n\
         ## 원문\n\n{article}",
        length_instruction = length_instruction(length),
    ))
}

/// Build length instruction string for the rewrite prompt.
fn length_instruction(length: Option<&str>) -> String {
    match length {
        Some(len) => format!("## 글자 수 제한\n\n공백 포함 **{len}자** 이내로 작성해주세요."),
        None => String::new(),
    }
}

/// Build rewrite output file path using today's date.
pub fn rewrite_file_path(slug_source: &str) -> std::path::PathBuf {
    rewrite_file_path_with_date(slug_source, &today_str())
}

/// Build rewrite file path with an explicit date string (for testing).
pub fn rewrite_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_rewrite.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(REWRITE_DIR).join(filename)
}

/// Save rewrite result to file. Creates the drafts directory if needed.
fn save_rewrite(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/rewrite` command: rewrite an article in a different style/tone.
pub async fn handle_rewrite(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/rewrite").unwrap_or("").trim();
    let (style, length, file_path, inline_text) = parse_rewrite_args(args);

    // Read article from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_rewrite_prompt(&article, style.as_deref(), length.as_deref()) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /rewrite <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /rewrite --file <경로>{RESET}");
            println!(
                "{DIM}  옵션:   --style <스트레이트|피처|칼럼|요약|SNS>{RESET}"
            );
            println!("{DIM}  옵션:   --length <글자수>{RESET}");
            println!(
                "{DIM}  예시:   /rewrite --style 요약 --file draft.txt{RESET}"
            );
            println!(
                "{DIM}  기존 기사를 다른 포맷·톤으로 재작성합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save rewrite to .journalist/drafts/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "rewrite".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "rewrite".to_string()
            } else {
                preview
            }
        };
        let path = rewrite_file_path(&slug_source);
        match save_rewrite(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 재작성 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  재작성 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /summary ─────────────────────────────────────────────────────────────

/// Parse `/summary` arguments: if the argument is an existing file path, read it;
/// otherwise treat it as inline text.
pub fn resolve_summary_input(args: &str) -> Option<String> {
    let args = args.trim();
    if args.is_empty() {
        return None;
    }

    // Check if the first token is an existing file
    let first_token = args.split_whitespace().next().unwrap_or("");
    if std::path::Path::new(first_token).is_file() {
        match std::fs::read_to_string(first_token) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {first_token} ({} bytes){RESET}",
                    content.len()
                );
                Some(content)
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {first_token} — {e}{RESET}\n");
                None
            }
        }
    } else {
        Some(args.to_string())
    }
}

/// Build the prompt for `/summary`: generate a concise 3–5 line summary.
pub fn build_summary_prompt(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 문서를 읽고 **3~5줄로 핵심 요약**을 작성해주세요.\n\n\
         ## 요약 규칙\n\n\
         1. **첫 줄**: 가장 중요한 사실/결론을 한 문장으로.\n\
         2. **나머지**: 핵심 근거, 배경, 수치를 간결하게.\n\
         3. 전문 용어가 있으면 괄호 안에 간단히 풀어주세요.\n\
         4. 출처나 날짜 등 메타정보가 있으면 포함하세요.\n\
         5. 한국어로 작성하세요.\n\n\
         ---\n\n\
         {text}"
    ))
}

/// Handle `/summary <filepath or text>`.
pub async fn handle_summary(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/summary").unwrap_or("").trim();

    let text = match resolve_summary_input(args) {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            println!("{DIM}  사용법: /summary <파일경로 또는 텍스트>{RESET}");
            println!("{DIM}  예시:   /summary press_release.txt{RESET}");
            println!("{DIM}  예시:   /summary 정부가 오늘 새로운 부동산 정책을 발표했다...{RESET}");
            println!("{DIM}  보도자료, 판결문, 정책문서 등을 3~5줄로 빠르게 요약합니다.{RESET}\n");
            return;
        }
    };

    let prompt = build_summary_prompt(&text).unwrap();
    run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);
}

// ── /stats ──────────────────────────────────────────────────────────────

/// Text statistics computed locally (no AI).
#[derive(Debug, PartialEq)]
pub struct TextStats {
    pub chars_with_spaces: usize,
    pub chars_without_spaces: usize,
    pub words: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    /// Estimated reading time in seconds (based on ~500 chars/min for Korean).
    pub reading_time_secs: u64,
}

/// Compute text statistics from a string.
pub fn compute_text_stats(text: &str) -> TextStats {
    let chars_with_spaces = text.chars().count();
    let chars_without_spaces = text.chars().filter(|c| !c.is_whitespace()).count();

    // Word count: split on whitespace, count non-empty tokens
    let words = text.split_whitespace().count();

    // Sentence count: split on sentence-ending punctuation (. ! ? 。)
    let sentences = text
        .chars()
        .filter(|&c| c == '.' || c == '!' || c == '?' || c == '。')
        .count()
        .max(if chars_without_spaces > 0 { 1 } else { 0 });

    // Paragraph count: sequences of non-empty lines separated by blank lines
    let paragraphs = text
        .split('\n')
        .fold((0usize, false), |(count, in_para), line| {
            let non_empty = !line.trim().is_empty();
            if non_empty && !in_para {
                (count + 1, true)
            } else if !non_empty {
                (count, false)
            } else {
                (count, in_para)
            }
        })
        .0;

    // Korean reading speed ~500 chars/min (excluding spaces)
    let reading_time_secs = if chars_without_spaces > 0 {
        (chars_without_spaces as u64 * 60) / 500
    } else {
        0
    };

    TextStats {
        chars_with_spaces,
        chars_without_spaces,
        words,
        sentences,
        paragraphs,
        reading_time_secs,
    }
}

/// Find the most recently modified file in `.journalist/drafts/`.
fn find_latest_draft() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(DRAFTS_DIR);
    if !dir.exists() {
        return None;
    }
    let mut best: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "md") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if best.as_ref().map_or(true, |(_, t)| modified > *t) {
                        best = Some((path, modified));
                    }
                }
            }
        }
    }
    best.map(|(p, _)| p)
}

/// Format reading time as human-readable string.
fn format_reading_time(secs: u64) -> String {
    if secs < 60 {
        format!("{}초", secs)
    } else {
        let min = secs / 60;
        let sec = secs % 60;
        if sec == 0 {
            format!("{}분", min)
        } else {
            format!("{}분 {}초", min, sec)
        }
    }
}

/// Handle `/stats [파일경로]` — show text statistics for a file.
pub fn handle_stats(input: &str) {
    let arg = input.strip_prefix("/stats").unwrap_or("").trim();

    let (path, content) = if arg.is_empty() {
        // No argument: find latest draft
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => (p.to_string_lossy().to_string(), c),
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                eprintln!("{DIM}  분석할 파일이 없습니다. 경로를 지정하거나 /article로 초안을 먼저 작성하세요.{RESET}\n");
                return;
            }
        }
    } else {
        match std::fs::read_to_string(arg) {
            Ok(c) => (arg.to_string(), c),
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패 ({arg}): {e}{RESET}\n");
                return;
            }
        }
    };

    let stats = compute_text_stats(&content);

    println!("{BOLD}  📊 기사 통계: {path}{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");
    println!(
        "  글자 수 (공백 포함)  {}",
        stats.chars_with_spaces
    );
    println!(
        "  글자 수 (공백 제외)  {}",
        stats.chars_without_spaces
    );
    println!("  단어 수             {}", stats.words);
    println!("  문장 수             {}", stats.sentences);
    println!("  문단 수             {}", stats.paragraphs);
    println!(
        "  예상 읽기 시간       {}",
        format_reading_time(stats.reading_time_secs)
    );
    println!();
}

// ── /draft ──────────────────────────────────────────────────────────────

/// Base directory for versioned drafts: `.journalist/drafts/<slug>/v1.md, v2.md, ...`
const DRAFT_VERSIONS_BASE: &str = ".journalist/drafts";

/// Format a UNIX timestamp as "YYYY-MM-DD HH:MM" (UTC).
pub fn format_unix_timestamp(secs: u64) -> String {
    let s = secs as i64;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Convert days since epoch to y/m/d (civil calendar)
    // Algorithm from Howard Hinnant's chrono-compatible date library
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}")
}

/// Return the directory path for a given draft title.
fn draft_versions_dir(title: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(title, 50);
    std::path::PathBuf::from(DRAFT_VERSIONS_BASE).join(slug)
}

/// Find the next version number for a draft title.
fn next_version_number(dir: &std::path::Path) -> u32 {
    if !dir.exists() {
        return 1;
    }
    let mut max = 0u32;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix('v') {
                if let Some(num_str) = rest.strip_suffix(".md") {
                    if let Ok(n) = num_str.parse::<u32>() {
                        if n > max {
                            max = n;
                        }
                    }
                }
            }
        }
    }
    max + 1
}

/// List all version files in a draft directory, sorted by version number.
fn list_versions(dir: &std::path::Path) -> Vec<(u32, std::path::PathBuf)> {
    let mut versions = Vec::new();
    if !dir.exists() {
        return versions;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix('v') {
                if let Some(num_str) = rest.strip_suffix(".md") {
                    if let Ok(n) = num_str.parse::<u32>() {
                        versions.push((n, entry.path()));
                    }
                }
            }
        }
    }
    versions.sort_by_key(|(n, _)| *n);
    versions
}

/// Handle `/draft` command with subcommands: save, list, load, diff.
pub fn handle_draft(input: &str) {
    let args = input.strip_prefix("/draft").unwrap_or("").trim();

    if args.is_empty() {
        print_draft_usage();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "save" => handle_draft_save(rest),
        "list" => handle_draft_list(rest),
        "load" => handle_draft_load(rest),
        "diff" => handle_draft_diff(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_draft_usage();
        }
    }
}

fn print_draft_usage() {
    println!("{DIM}  사용법:");
    println!("    /draft save <제목> [파일]   기사를 버전별로 저장 (파일 미지정 시 최신 초안)");
    println!("    /draft list [제목]          저장된 초안 목록");
    println!("    /draft load <제목> [버전]   특정 버전 불러오기 (미지정 시 최신)");
    println!("    /draft diff <제목> [v1] [v2] 두 버전 간 차이 비교{RESET}\n");
}

/// `/draft save <title> [file]`
fn handle_draft_save(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /draft save <제목> [파일]{RESET}\n");
        return;
    }

    let (title, file_arg) = match args.split_once(char::is_whitespace) {
        Some((t, f)) => (t.trim(), f.trim()),
        None => (args, ""),
    };

    // Read content: from file argument, or find latest draft
    let content = if !file_arg.is_empty() {
        match std::fs::read_to_string(file_arg) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패 ({file_arg}): {e}{RESET}\n");
                return;
            }
        }
    } else {
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                eprintln!("{RED}  저장할 파일이 없습니다. 파일 경로를 지정하거나 /article로 초안을 먼저 작성하세요.{RESET}\n");
                return;
            }
        }
    };

    let dir = draft_versions_dir(title);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("{RED}  디렉토리 생성 실패: {e}{RESET}\n");
        return;
    }

    let ver = next_version_number(&dir);
    let path = dir.join(format!("v{ver}.md"));
    if let Err(e) = std::fs::write(&path, &content) {
        eprintln!("{RED}  저장 실패: {e}{RESET}\n");
        return;
    }

    let char_count = content.chars().count();
    println!(
        "{GREEN}  ✅ 저장: {title} v{ver} ({char_count}자) → {}{RESET}\n",
        path.display()
    );
}

/// `/draft list [title]`
fn handle_draft_list(title: &str) {
    if title.is_empty() {
        // List all draft titles
        let base = std::path::Path::new(DRAFT_VERSIONS_BASE);
        if !base.exists() {
            println!("{DIM}  저장된 초안이 없습니다.{RESET}\n");
            return;
        }
        let mut entries: Vec<(String, usize, String, usize)> = Vec::new();
        if let Ok(dirs) = std::fs::read_dir(base) {
            for entry in dirs.flatten() {
                if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let versions = list_versions(&entry.path());
                if versions.is_empty() {
                    continue;
                }
                let ver_count = versions.len();
                // Last modified time of the latest version
                let last_path = &versions.last().unwrap().1;
                let modified = std::fs::metadata(last_path)
                    .and_then(|m| m.modified())
                    .ok();
                let date_str = modified
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| format_unix_timestamp(d.as_secs()))
                    .unwrap_or_else(|| "-".to_string());
                // Char count of latest version
                let char_count = std::fs::read_to_string(last_path)
                    .map(|c| c.chars().count())
                    .unwrap_or(0);
                entries.push((name, ver_count, date_str, char_count));
            }
        }

        if entries.is_empty() {
            println!("{DIM}  저장된 초안이 없습니다.{RESET}\n");
            return;
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));
        println!("{BOLD}  📝 초안 목록{RESET}");
        println!("{DIM}  ──────────────────────────────{RESET}");
        for (name, ver_count, date, chars) in &entries {
            println!("  {name}  (v{ver_count}, {date}, {chars}자)");
        }
        println!();
    } else {
        // List versions for a specific title
        let dir = draft_versions_dir(title);
        let versions = list_versions(&dir);
        if versions.is_empty() {
            eprintln!("{DIM}  '{title}'에 저장된 버전이 없습니다.{RESET}\n");
            return;
        }

        println!("{BOLD}  📝 {title} 버전 목록{RESET}");
        println!("{DIM}  ──────────────────────────────{RESET}");
        for (ver, path) in &versions {
            let modified = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok();
            let date_str = modified
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_unix_timestamp(d.as_secs()))
                .unwrap_or_else(|| "-".to_string());
            let char_count = std::fs::read_to_string(path)
                .map(|c| c.chars().count())
                .unwrap_or(0);
            println!("  v{ver}  ({date_str}, {char_count}자)");
        }
        println!();
    }
}

/// `/draft load <title> [version]`
fn handle_draft_load(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /draft load <제목> [버전]{RESET}\n");
        return;
    }

    let (title, ver_arg) = match args.split_once(char::is_whitespace) {
        Some((t, v)) => (t.trim(), v.trim()),
        None => (args, ""),
    };

    let dir = draft_versions_dir(title);
    let versions = list_versions(&dir);
    if versions.is_empty() {
        eprintln!("{DIM}  '{title}'에 저장된 버전이 없습니다.{RESET}\n");
        return;
    }

    let target_ver = if ver_arg.is_empty() {
        // Load latest
        versions.last().unwrap().0
    } else {
        // Parse version: accept "v3" or "3"
        let num_str = ver_arg.strip_prefix('v').unwrap_or(ver_arg);
        match num_str.parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("{RED}  버전 번호가 올바르지 않습니다: {ver_arg}{RESET}\n");
                return;
            }
        }
    };

    let path = dir.join(format!("v{target_ver}.md"));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let char_count = content.chars().count();
            println!(
                "{BOLD}  📄 {title} v{target_ver} ({char_count}자){RESET}"
            );
            println!("{DIM}  ──────────────────────────────{RESET}");
            println!("{content}");
        }
        Err(_) => {
            let available: Vec<String> = versions.iter().map(|(v, _)| format!("v{v}")).collect();
            eprintln!(
                "{RED}  v{target_ver} 버전이 존재하지 않습니다. 사용 가능: {}{RESET}\n",
                available.join(", ")
            );
        }
    }
}

/// `/draft diff <title> [v1] [v2]`
fn handle_draft_diff(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /draft diff <제목> [v1] [v2]{RESET}\n");
        return;
    }

    let parts: Vec<&str> = args.split_whitespace().collect();
    let title = parts[0];

    let dir = draft_versions_dir(title);
    let versions = list_versions(&dir);
    if versions.len() < 2 {
        eprintln!("{DIM}  비교하려면 최소 2개 버전이 필요합니다.{RESET}\n");
        return;
    }

    // Determine which two versions to compare
    let (v1, v2) = if parts.len() >= 3 {
        let parse_ver = |s: &str| -> Option<u32> {
            let num_str = s.strip_prefix('v').unwrap_or(s);
            num_str.parse().ok()
        };
        match (parse_ver(parts[1]), parse_ver(parts[2])) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                eprintln!("{RED}  버전 번호가 올바르지 않습니다.{RESET}\n");
                return;
            }
        }
    } else if parts.len() == 2 {
        // One version specified: compare it with the latest
        let parse_ver = |s: &str| -> Option<u32> {
            let num_str = s.strip_prefix('v').unwrap_or(s);
            num_str.parse().ok()
        };
        match parse_ver(parts[1]) {
            Some(a) => {
                let latest = versions.last().unwrap().0;
                if a == latest {
                    // Compare with second-to-last
                    let prev = versions[versions.len() - 2].0;
                    (prev, a)
                } else {
                    (a, latest)
                }
            }
            None => {
                eprintln!("{RED}  버전 번호가 올바르지 않습니다.{RESET}\n");
                return;
            }
        }
    } else {
        // No versions specified: compare last two
        let len = versions.len();
        (versions[len - 2].0, versions[len - 1].0)
    };

    let path1 = dir.join(format!("v{v1}.md"));
    let path2 = dir.join(format!("v{v2}.md"));

    let content1 = match std::fs::read_to_string(&path1) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("{RED}  v{v1} 버전이 존재하지 않습니다.{RESET}\n");
            return;
        }
    };
    let content2 = match std::fs::read_to_string(&path2) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("{RED}  v{v2} 버전이 존재하지 않습니다.{RESET}\n");
            return;
        }
    };

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();

    println!(
        "{BOLD}  📊 {title}: v{v1} → v{v2} 비교{RESET}"
    );
    println!("{DIM}  ──────────────────────────────{RESET}");

    // Simple line-by-line diff
    let max_lines = lines1.len().max(lines2.len());
    let mut adds = 0usize;
    let mut removes = 0usize;
    let mut changes = Vec::new();

    for i in 0..max_lines {
        let l1 = lines1.get(i).copied();
        let l2 = lines2.get(i).copied();
        match (l1, l2) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                changes.push(format!("{RED}  - [{ln}] {a}{RESET}", ln = i + 1));
                changes.push(format!("{GREEN}  + [{ln}] {b}{RESET}", ln = i + 1));
                removes += 1;
                adds += 1;
            }
            (Some(a), None) => {
                changes.push(format!("{RED}  - [{ln}] {a}{RESET}", ln = i + 1));
                removes += 1;
            }
            (None, Some(b)) => {
                changes.push(format!("{GREEN}  + [{ln}] {b}{RESET}", ln = i + 1));
                adds += 1;
            }
            (None, None) => {}
        }
    }

    if changes.is_empty() {
        println!("{DIM}  두 버전이 동일합니다.{RESET}\n");
    } else {
        let c1_chars = content1.chars().count();
        let c2_chars = content2.chars().count();
        println!(
            "{DIM}  v{v1}: {c1_chars}자 → v{v2}: {c2_chars}자 (차이: {adds} 추가, {removes} 삭제){RESET}"
        );
        for line in &changes {
            println!("{line}");
        }
        println!();
    }
}

// ── /export ─────────────────────────────────────────────────────────────

/// Base directory for exported articles.
const EXPORTS_DIR: &str = ".journalist/exports";

/// Strip markdown markup to produce clean plain text.
pub fn markdown_to_plain_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());

    for line in md.lines() {
        let trimmed = line.trim();

        // Skip horizontal rules
        if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
            && trimmed.len() >= 3
            && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3
        {
            out.push('\n');
            continue;
        }

        // Strip heading markers
        let line = if trimmed.starts_with('#') {
            let content = trimmed.trim_start_matches('#').trim();
            content
        } else {
            trimmed
        };

        // Strip bold/italic markers
        let line = line.replace("**", "").replace("__", "");
        let line = line.replace('*', "").replace('_', " ");

        // Strip inline code backticks
        let line = line.replace('`', "");

        // Strip link syntax [text](url) → text
        let line = strip_md_links(&line);

        // Strip image syntax ![alt](url) → alt
        let line = strip_md_images(&line);

        // Strip list markers
        let line = strip_list_marker(&line);

        out.push_str(&line);
        out.push('\n');
    }

    // Collapse triple+ newlines into double
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }

    out.trim().to_string()
}

/// Strip markdown link syntax: [text](url) → text
fn strip_md_links(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            // Look for ](
            if let Some(close_bracket) = chars[i + 1..].iter().position(|&c| c == ']') {
                let close_idx = i + 1 + close_bracket;
                if close_idx + 1 < chars.len() && chars[close_idx + 1] == '(' {
                    if let Some(close_paren) =
                        chars[close_idx + 2..].iter().position(|&c| c == ')')
                    {
                        // Extract link text
                        let text: String = chars[i + 1..close_idx].iter().collect();
                        result.push_str(&text);
                        i = close_idx + 2 + close_paren + 1;
                        continue;
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Strip markdown image syntax: ![alt](url) → alt
fn strip_md_images(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if let Some(close_bracket) = chars[i + 2..].iter().position(|&c| c == ']') {
                let close_idx = i + 2 + close_bracket;
                if close_idx + 1 < chars.len() && chars[close_idx + 1] == '(' {
                    if let Some(close_paren) =
                        chars[close_idx + 2..].iter().position(|&c| c == ')')
                    {
                        let alt: String = chars[i + 2..close_idx].iter().collect();
                        result.push_str(&alt);
                        i = close_idx + 2 + close_paren + 1;
                        continue;
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Strip list markers (-, *, numbered) from line start.
fn strip_list_marker(s: &str) -> String {
    let trimmed = s.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return trimmed[2..].to_string();
    }
    // Numbered list: "1. ", "2. ", etc.
    if let Some(dot_pos) = trimmed.find(". ") {
        if dot_pos <= 3 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return trimmed[dot_pos + 2..].to_string();
        }
    }
    s.to_string()
}

/// Convert markdown to simple HTML.
pub fn markdown_to_html(md: &str) -> String {
    let mut out = String::with_capacity(md.len() * 2);
    out.push_str("<!DOCTYPE html>\n<html lang=\"ko\">\n<head>\n");
    out.push_str("<meta charset=\"UTF-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    out.push_str("<style>\n");
    out.push_str("body { font-family: 'Noto Sans KR', sans-serif; max-width: 720px; margin: 2em auto; padding: 0 1em; line-height: 1.8; color: #333; }\n");
    out.push_str("h1 { font-size: 1.6em; border-bottom: 2px solid #333; padding-bottom: 0.3em; }\n");
    out.push_str("h2 { font-size: 1.3em; margin-top: 1.5em; }\n");
    out.push_str("h3 { font-size: 1.1em; }\n");
    out.push_str("blockquote { border-left: 3px solid #ccc; padding-left: 1em; color: #666; margin: 1em 0; }\n");
    out.push_str(".meta { color: #888; font-size: 0.9em; margin-bottom: 2em; }\n");
    out.push_str("</style>\n</head>\n<body>\n");

    let mut in_paragraph = false;
    let mut in_list = false;
    let mut in_blockquote = false;

    for line in md.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            if in_blockquote {
                out.push_str("</blockquote>\n");
                in_blockquote = false;
            }
            continue;
        }

        // Headings
        if trimmed.starts_with("### ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            let content = html_escape(&trimmed[4..]);
            out.push_str(&format!("<h3>{content}</h3>\n"));
            continue;
        }
        if trimmed.starts_with("## ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            let content = html_escape(&trimmed[3..]);
            out.push_str(&format!("<h2>{content}</h2>\n"));
            continue;
        }
        if trimmed.starts_with("# ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            let content = html_escape(&trimmed[2..]);
            out.push_str(&format!("<h1>{content}</h1>\n"));
            continue;
        }

        // Blockquote
        if trimmed.starts_with("> ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            if !in_blockquote {
                out.push_str("<blockquote>\n");
                in_blockquote = true;
            }
            let content = inline_md_to_html(&trimmed[2..]);
            out.push_str(&format!("<p>{content}</p>\n"));
            continue;
        }

        // List items
        if (trimmed.starts_with("- ") || trimmed.starts_with("* "))
            || (trimmed.len() > 2
                && trimmed.find(". ").map_or(false, |p| {
                    p <= 3 && trimmed[..p].chars().all(|c| c.is_ascii_digit())
                }))
        {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            let text = strip_list_marker(trimmed);
            let content = inline_md_to_html(&text);
            out.push_str(&format!("<li>{content}</li>\n"));
            continue;
        }

        // Regular paragraph
        if !in_paragraph {
            out.push_str("<p>");
            in_paragraph = true;
        } else {
            out.push_str("<br>\n");
        }
        let content = inline_md_to_html(trimmed);
        out.push_str(&content);
    }

    if in_paragraph {
        out.push_str("</p>\n");
    }
    if in_list {
        out.push_str("</ul>\n");
    }
    if in_blockquote {
        out.push_str("</blockquote>\n");
    }

    out.push_str("</body>\n</html>\n");
    out
}

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convert inline markdown (bold, italic, code, links) to HTML.
fn inline_md_to_html(s: &str) -> String {
    let s = html_escape(s);
    // Bold: **text** or __text__
    let s = regex_replace_pairs(&s, "**", "<strong>", "</strong>");
    let s = regex_replace_pairs(&s, "__", "<strong>", "</strong>");
    // Italic: *text* or _text_ (simplified)
    let s = regex_replace_pairs(&s, "*", "<em>", "</em>");
    // Inline code: `code`
    let s = regex_replace_pairs(&s, "`", "<code>", "</code>");
    s
}

/// Simple paired-delimiter replacement.
fn regex_replace_pairs(s: &str, delim: &str, open: &str, close: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    let mut is_open = false;

    while let Some(pos) = rest.find(delim) {
        result.push_str(&rest[..pos]);
        if is_open {
            result.push_str(close);
        } else {
            result.push_str(open);
        }
        is_open = !is_open;
        rest = &rest[pos + delim.len()..];
    }
    result.push_str(rest);
    // If we opened but never closed, treat the tag as literal
    if is_open {
        // Re-do without replacement — just return original
        return s.to_string();
    }
    result
}

/// Build the metadata header for an exported article.
fn build_export_meta(source_path: &str, char_count: usize) -> String {
    let today = today_str();
    let filename = std::path::Path::new(source_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    format!(
        "제목: {filename}\n날짜: {today}\n글자수: {char_count}자\n"
    )
}

/// Handle `/export <파일> [--html]` — export article to plain text or HTML.
pub fn handle_export(input: &str) {
    let args = input.strip_prefix("/export").unwrap_or("").trim();

    if args.is_empty() {
        // Try latest draft
        match find_latest_draft() {
            Some(p) => export_file(&p.to_string_lossy(), false),
            None => {
                eprintln!("{DIM}  사용법: /export <파일> [--html]{RESET}");
                eprintln!("{DIM}  마크다운 기사를 텍스트 또는 HTML로 내보냅니다.{RESET}\n");
            }
        }
        return;
    }

    let html_mode = args.contains("--html");
    let file_arg = args.replace("--html", "").trim().to_string();

    if file_arg.is_empty() {
        match find_latest_draft() {
            Some(p) => export_file(&p.to_string_lossy(), html_mode),
            None => {
                eprintln!("{RED}  내보낼 파일을 지정하세요.{RESET}\n");
            }
        }
    } else {
        export_file(&file_arg, html_mode);
    }
}

/// Core export logic: read file, convert, save, print info.
fn export_file(source_path: &str, html_mode: bool) {
    let content = match std::fs::read_to_string(source_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  파일 읽기 실패 ({source_path}): {e}{RESET}\n");
            return;
        }
    };

    // Build output
    let (output, ext) = if html_mode {
        (markdown_to_html(&content), "html")
    } else {
        let plain = markdown_to_plain_text(&content);
        let meta = build_export_meta(source_path, plain.chars().filter(|c| !c.is_whitespace()).count());
        (format!("{meta}\n---\n\n{plain}"), "txt")
    };

    // Ensure exports directory
    let exports = std::path::Path::new(EXPORTS_DIR);
    if let Err(e) = std::fs::create_dir_all(exports) {
        eprintln!("{RED}  디렉토리 생성 실패: {e}{RESET}\n");
        return;
    }

    // Build output filename
    let stem = std::path::Path::new(source_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "export".to_string());
    let out_name = format!("{stem}.{ext}");
    let out_path = exports.join(&out_name);

    if let Err(e) = std::fs::write(&out_path, &output) {
        eprintln!("{RED}  저장 실패: {e}{RESET}\n");
        return;
    }

    let char_count = if html_mode {
        markdown_to_plain_text(&content)
            .chars()
            .filter(|c| !c.is_whitespace())
            .count()
    } else {
        output
            .chars()
            .filter(|c| !c.is_whitespace())
            .count()
    };

    let format_label = if html_mode { "HTML" } else { "텍스트" };
    println!(
        "{GREEN}  ✅ {format_label} 내보내기 완료: {}{RESET}",
        out_path.display()
    );
    println!("{DIM}  글자수: {char_count}자 (공백 제외){RESET}");
    println!(
        "{DIM}  💡 클립보드 복사: cat {} | xclip -selection clipboard{RESET}\n",
        out_path.display()
    );
}

// ── /proofread ─────────────────────────────────────────────────────────

const PROOFREAD_DIR: &str = ".journalist/proofread";

/// Parse `/proofread` arguments: `--file <path>` and remaining inline text.
pub fn parse_proofread_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    let mut file_path: Option<String> = None;
    let mut remaining_parts: Vec<String> = Vec::new();

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "--file" => {
                if i + 1 < tokens.len() {
                    file_path = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            other => {
                remaining_parts.push(other.to_string());
                i += 1;
            }
        }
    }

    (file_path, remaining_parts.join(" "))
}

/// Build the proofread prompt with Korean news style rules embedded.
pub fn build_proofread_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    Some(format!(
        r#"당신은 한국 신문사의 교열 전문가입니다. 아래 기사를 교열하세요.

## 교열 규칙
1. **맞춤법·띄어쓰기**: 한글 맞춤법 통일안 및 표준어 규정 준수
2. **경어체 통일**: 뉴스 기사는 '~했다', '~이다' 등 해요체가 아닌 하십시오체/해라체(보도문체) 통일
3. **숫자 표기**: 만 단위 이상은 한글 병기 (예: 1조2000억원), 날짜는 'O일' (예: 15일)
4. **외래어 표기법**: 국립국어원 외래어 표기법 준수 (예: 컴퓨터, 인터넷)
5. **중복 표현 제거**: '약 ~정도', '먼저 ~에 앞서' 등 불필요한 중복 삭제
6. **인용문 형식**: 직접 인용은 큰따옴표(" "), 간접 인용은 따옴표 없이 '~(이)라고 말했다'
7. **주어-술어 호응**: 문장 내 주어와 술어의 호응 확인
8. **문장 길이**: 한 문장이 80자를 초과하면 분리 권장
9. **비문·어색한 표현**: 자연스러운 한국어로 교정
10. **뉴스 용어**: 약어 첫 등장 시 풀어쓰기 (예: GDP(국내총생산))

## 출력 형식
아래 형식으로 교정 결과를 출력하세요:

### 교열 결과

| # | 위치 | 원문 | 교정 | 근거 |
|---|------|------|------|------|
| 1 | 1문단 | ... | ... | ... |

### 교정된 전문
(교정이 반영된 전체 기사)

### 총평
(전반적인 문체·구조 평가, 1~2문장)

## 원문
{article}"#
    ))
}

/// Build proofread result file path with an explicit date string.
pub fn proofread_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_proofread.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(PROOFREAD_DIR).join(filename)
}

/// Build proofread result file path with today's date.
pub fn proofread_file_path(slug_source: &str) -> std::path::PathBuf {
    proofread_file_path_with_date(slug_source, &today_str())
}

/// Save proofread result to file. Creates the directory if needed.
fn save_proofread(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/proofread` command: proofread a Korean article for grammar, spelling, and news style.
pub async fn handle_proofread(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/proofread").unwrap_or("").trim();
    let (file_path, inline_text) = parse_proofread_args(args);

    // Read article from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_proofread_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /proofread <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /proofread --file <경로>{RESET}");
            println!(
                "{DIM}  한국어 기사의 맞춤법, 문법, 뉴스 문체를 교정합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save proofread result to .journalist/proofread/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "proofread".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "proofread".to_string()
            } else {
                preview
            }
        };
        let path = proofread_file_path(&slug_source);
        match save_proofread(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 교열 결과 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  교열 결과 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /quote ──────────────────────────────────────────────────────────────

/// Quotes database path.
const QUOTES_FILE: &str = ".journalist/quotes.json";

/// Handle the /quote command: manage interview quotes.
pub fn handle_quote(input: &str) {
    let args = input.strip_prefix("/quote").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /quote add <취재원> <발언>{RESET}");
                println!("{DIM}  예시: /quote add 홍길동 \"반도체 수출이 3개월 연속 증가했습니다\"{RESET}\n");
            } else {
                quote_add(rest);
            }
        }
        "list" => {
            let rest = args.strip_prefix("list").unwrap_or("").trim();
            quote_list(rest);
        }
        "search" => {
            let rest = args.strip_prefix("search").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /quote search <키워드>{RESET}\n");
            } else {
                quote_search(rest);
            }
        }
        "remove" => {
            let rest = args.strip_prefix("remove").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /quote remove <번호>{RESET}");
                println!("{DIM}  예시: /quote remove 2{RESET}\n");
            } else {
                quote_remove(rest);
            }
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /quote [add|list|search|remove]{RESET}\n");
        }
    }
}

fn load_quotes() -> Vec<serde_json::Value> {
    load_quotes_from(std::path::Path::new(QUOTES_FILE))
}

fn load_quotes_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_quotes(quotes: &[serde_json::Value]) {
    save_quotes_to(quotes, std::path::Path::new(QUOTES_FILE));
}

fn save_quotes_to(quotes: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(quotes) {
        let _ = std::fs::write(path, json);
    }
}

/// Look up source org from sources.json by name.
fn source_org_for(name: &str) -> Option<String> {
    let sources = load_sources();
    for s in &sources {
        if s["name"].as_str() == Some(name) {
            if let Some(org) = s["org"].as_str() {
                if !org.is_empty() {
                    return Some(org.to_string());
                }
            }
        }
    }
    None
}

fn quote_add(args: &str) {
    // Parse: <취재원> <발언> — the first token is the source name, rest is the quote
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 || parts[1].is_empty() {
        println!("{DIM}  취재원 이름과 발언 내용이 모두 필요합니다.{RESET}\n");
        return;
    }
    let source_name = parts[0];
    let text = parts[1].trim_matches('"');
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);

    let entry = serde_json::json!({
        "source": source_name,
        "text": text,
        "timestamp": timestamp,
    });
    let mut quotes = load_quotes();
    quotes.push(entry);
    save_quotes(&quotes);

    let org_info = source_org_for(source_name)
        .map(|o| format!(" ({})", o))
        .unwrap_or_default();
    println!(
        "{DIM}  인용문 추가됨: {source_name}{org_info} — \"{text}\" [{timestamp}]{RESET}\n"
    );
}

fn quote_list(filter_source: &str) {
    let quotes = load_quotes();
    if quotes.is_empty() {
        println!("{DIM}  인용문 DB가 비어 있습니다.");
        println!("  /quote add <취재원> <발언> 으로 추가하세요.{RESET}\n");
        return;
    }

    let filtered: Vec<(usize, &serde_json::Value)> = if filter_source.is_empty() {
        quotes.iter().enumerate().collect()
    } else {
        quotes
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                q["source"]
                    .as_str()
                    .map(|s| s == filter_source)
                    .unwrap_or(false)
            })
            .collect()
    };

    if filtered.is_empty() {
        println!("{DIM}  '{filter_source}' 취재원의 인용문이 없습니다.{RESET}\n");
        return;
    }

    let title = if filter_source.is_empty() {
        format!("인용문 목록 ({} 건)", filtered.len())
    } else {
        let org_info = source_org_for(filter_source)
            .map(|o| format!(" ({})", o))
            .unwrap_or_default();
        format!(
            "{filter_source}{org_info} 인용문 ({} 건)",
            filtered.len()
        )
    };
    println!("{DIM}  ── {title} ──");
    for (i, q) in &filtered {
        let source = q["source"].as_str().unwrap_or("?");
        let text = q["text"].as_str().unwrap_or("");
        let ts = q["timestamp"].as_str().unwrap_or("");
        println!("  {}. [{ts}] {source}: \"{text}\"", i + 1);
    }
    println!("{RESET}");
}

fn quote_search(keyword: &str) {
    let quotes = load_quotes();
    let keyword_lower = keyword.to_lowercase();
    let matches: Vec<(usize, &serde_json::Value)> = quotes
        .iter()
        .enumerate()
        .filter(|(_, q)| {
            let text = q["text"].as_str().unwrap_or("").to_lowercase();
            let source = q["source"].as_str().unwrap_or("").to_lowercase();
            text.contains(&keyword_lower) || source.contains(&keyword_lower)
        })
        .collect();

    if matches.is_empty() {
        println!("{DIM}  '{keyword}' 검색 결과가 없습니다.{RESET}\n");
        return;
    }

    println!("{DIM}  ── 인용문 검색: '{keyword}' ({} 건) ──", matches.len());
    for (i, q) in &matches {
        let source = q["source"].as_str().unwrap_or("?");
        let text = q["text"].as_str().unwrap_or("");
        let ts = q["timestamp"].as_str().unwrap_or("");
        println!("  {}. [{ts}] {source}: \"{text}\"", i + 1);
    }
    println!("{RESET}");
}

fn quote_remove(args: &str) {
    let idx: usize = match args.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  올바른 번호를 입력하세요.{RESET}\n");
            return;
        }
    };
    let mut quotes = load_quotes();
    if idx > quotes.len() {
        eprintln!("{RED}  번호 {idx}번은 범위를 벗어났습니다 (총 {} 건).{RESET}\n", quotes.len());
        return;
    }
    let removed = quotes.remove(idx - 1);
    save_quotes(&quotes);
    let source = removed["source"].as_str().unwrap_or("?");
    let text = removed["text"].as_str().unwrap_or("");
    let preview = if text.len() > 30 {
        format!("{}…", &text[..text.char_indices().take(30).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(30)])
    } else {
        text.to_string()
    };
    println!("{DIM}  인용문 삭제됨: {source} — \"{preview}\"{RESET}\n");
}

// ── /legal ───────────────────────────────────────────────────────────────

const LEGAL_DIR: &str = ".journalist/legal";

/// Parse `/legal` input to extract `--file <path>` and inline text.
/// Returns `(Option<file_path>, remaining_text)`.
pub fn parse_legal_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    if let Some(rest) = args.strip_prefix("--file") {
        let rest = rest.trim_start();
        if rest.is_empty() {
            return (None, String::new());
        }
        let mut path_end = rest.len();
        for (i, ch) in rest.char_indices() {
            if ch.is_whitespace() {
                path_end = i;
                break;
            }
        }
        let file_path = rest[..path_end].to_string();
        let remaining = rest[path_end..].trim().to_string();
        (Some(file_path), remaining)
    } else {
        (None, args.to_string())
    }
}

/// Build the prompt for the `/legal` command (pre-publication legal risk check).
pub fn build_legal_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 기사 텍스트에 대해 출고 전 법적 리스크를 점검해주세요.\n\n\
         기사 텍스트:\n\"\"\"\n{article}\n\"\"\"\n\n\
         다음 항목을 순서대로 점검하고, 각 항목마다 리스크 등급을 표시하세요:\n\n\
         ## 1. 명예훼손 위험 요소\n\
         - 미확인 사실을 단정적으로 주장하고 있는지\n\
         - 출처 없이 특정인/단체를 비난하고 있는지\n\
         - 사생활 침해 요소가 있는지 (주거지, 가족관계, 건강정보 등)\n\
         - **형사상 명예훼손**: 사실 적시라도 공익 목적 없이 명예를 훼손하면 처벌 대상\n\n\
         ## 2. 초상권·프라이버시 침해\n\
         - 본인 동의 없는 사진/영상 사용 여부\n\
         - 사적 공간에서의 촬영물 포함 여부\n\
         - 개인정보(전화번호, 주소, 주민번호 등) 노출 여부\n\n\
         ## 3. 일방적 보도 여부 (반론권)\n\
         - 비판 대상의 반론/해명이 포함되어 있는지\n\
         - 반론 요청 시도 여부가 기재되어 있는지\n\
         - 언론중재법상 반론보도청구권 리스크\n\n\
         ## 4. 공인/사인 구분 기준 적용\n\
         - 기사 대상이 공인인지 사인인지 판단\n\
         - 공인: 공적 활동에 대한 비판은 허용 범위가 넓음\n\
         - 사인: 보도 기준이 엄격, 공익성 입증 필요\n\
         - 적용된 기준이 적절한지 판단\n\n\
         ## 5. 기타 법적 리스크\n\
         - 저작권 침해 (타 매체 기사/사진 무단 인용)\n\
         - 재판 계류 중 사건의 무죄추정 원칙 준수 여부\n\
         - 소년법/성폭력처벌법 등 보도 제한 규정 위반 여부\n\n\
         ## 종합 판정\n\
         각 항목별로 다음 등급을 부여하세요:\n\
         - ✅ 안전: 법적 리스크 없음\n\
         - ⚠️ 주의: 수정을 권고하는 부분 있음\n\
         - 🚨 위험: 반드시 수정 또는 삭제 필요\n\n\
         **종합 리스크 등급**과 함께, ⚠️ 이상 항목에 대해 **구체적인 수정 제안**을 제시하세요.\n\
         법적 근거(조항)를 가능한 한 명시하세요."
    ))
}

/// Build the legal check file path with an explicit date string (for testing).
pub fn legal_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_legal.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(LEGAL_DIR).join(filename)
}

/// Build legal check file path with today's date.
pub fn legal_file_path(slug_source: &str) -> std::path::PathBuf {
    legal_file_path_with_date(slug_source, &today_str())
}

/// Save legal check result to file. Creates the legal directory if needed.
fn save_legal(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// List existing legal check files in the legal directory.
fn legal_list() {
    let dir = std::path::Path::new(LEGAL_DIR);
    if !dir.exists() {
        println!("{DIM}  저장된 법적 점검 기록이 없습니다.{RESET}\n");
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect(),
        Err(_) => {
            println!("{DIM}  법적 점검 디렉토리를 읽을 수 없습니다.{RESET}\n");
            return;
        }
    };
    if entries.is_empty() {
        println!("{DIM}  저장된 법적 점검 기록이 없습니다.{RESET}\n");
        return;
    }
    entries.sort_by_key(|e| e.file_name());
    println!("{DIM}  저장된 법적 점검 목록:{RESET}");
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        println!(
            "{DIM}  {idx}. {name}{RESET}",
            idx = i + 1,
            name = name.to_string_lossy()
        );
    }
    println!();
}

/// Handle the `/legal` command: pre-publication legal risk check.
pub async fn handle_legal(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/legal").unwrap_or("").trim();

    if args == "list" {
        legal_list();
        return;
    }

    let (file_path, inline_text) = parse_legal_args(args);

    // Read article content from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_legal_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /legal <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /legal --file <경로>{RESET}");
            println!("{DIM}  또는:   /legal list — 저장된 법적 점검 목록{RESET}");
            println!("{DIM}  예시:   /legal --file draft.md{RESET}");
            println!(
                "{DIM}  기사의 명예훼손, 초상권, 반론권, 공인/사인 구분 등 법적 리스크를 점검합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save legal check result to .journalist/legal/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "legal".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "legal".to_string()
            } else {
                preview
            }
        };
        let path = legal_file_path(&slug_source);
        match save_legal(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 법적 점검 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  법적 점검 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /correction — 정정보도 관리 ────────────────────────────────────────

const CORRECTIONS_DIR: &str = ".journalist/corrections";
const CORRECTIONS_FILE: &str = ".journalist/corrections/corrections.jsonl";

/// A single correction record stored as JSONL.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CorrectionRecord {
    pub date: String,
    pub article: String,
    pub error: String,
    pub fix: String,
    pub status: String,
}

/// Handle the `/correction` command: manage correction reports (정정보도).
pub async fn handle_correction(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/correction").unwrap_or("").trim();

    if args.is_empty() || args == "help" {
        correction_usage();
        return;
    }

    if args == "list" {
        correction_list();
        return;
    }

    if args.starts_with("add") {
        let add_args = args.strip_prefix("add").unwrap_or("").trim();
        correction_add(add_args);
        return;
    }

    if args.starts_with("report") {
        let report_args = args.strip_prefix("report").unwrap_or("").trim();
        correction_report(agent, report_args, session_total, model).await;
        return;
    }

    println!("{DIM}  알 수 없는 하위 명령: {args}{RESET}");
    correction_usage();
}

fn correction_usage() {
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}  /correction add --article <제목> --error <오류 내용> --fix <정정 내용>{RESET}");
    println!("{DIM}  /correction list                — 정정 이력 조회{RESET}");
    println!("{DIM}  /correction report [--article <제목>] — AI 기반 정정보도문 생성{RESET}");
    println!(
        "{DIM}  정정보도 기록을 관리합니다. 한국 언론중재법에 따른 정정보도문을 생성합니다.{RESET}\n"
    );
}

/// Parse `--key value` pairs from add args.
pub fn parse_correction_add_args(args: &str) -> (String, String, String) {
    let mut article = String::new();
    let mut error = String::new();
    let mut fix = String::new();

    let parts: Vec<&str> = args.splitn(7, ' ').collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--article" => {
                if i + 1 < parts.len() {
                    article = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--error" => {
                if i + 1 < parts.len() {
                    error = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--fix" => {
                if i + 1 < parts.len() {
                    fix = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    (article, error, fix)
}

fn correction_add(args: &str) {
    let (article, error, fix) = parse_correction_add_args(args);

    if article.is_empty() || error.is_empty() || fix.is_empty() {
        println!(
            "{RED}  --article, --error, --fix 모두 필수입니다.{RESET}"
        );
        println!("{DIM}  예: /correction add --article \"제목\" --error \"오류 내용\" --fix \"정정 내용\"{RESET}\n");
        return;
    }

    let record = CorrectionRecord {
        date: today_str(),
        article,
        error,
        fix,
        status: "pending".to_string(),
    };

    if let Err(e) = append_correction(&record) {
        eprintln!("{RED}  정정 기록 저장 실패: {e}{RESET}\n");
        return;
    }

    println!(
        "{GREEN}  ✓ 정정 기록 추가: {} — {}{RESET}\n",
        record.article, record.error
    );
}

/// Append a correction record to the JSONL file.
pub fn append_correction(record: &CorrectionRecord) -> Result<(), std::io::Error> {
    let dir = std::path::Path::new(CORRECTIONS_DIR);
    std::fs::create_dir_all(dir)?;

    let json = serde_json::to_string(record).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(CORRECTIONS_FILE)?;
    writeln!(file, "{json}")?;
    Ok(())
}

/// Load all correction records from the JSONL file.
pub fn load_corrections() -> Vec<CorrectionRecord> {
    let path = std::path::Path::new(CORRECTIONS_FILE);
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn correction_list() {
    let records = load_corrections();
    if records.is_empty() {
        println!("{DIM}  저장된 정정 기록이 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  정정 기록 ({} 건):{RESET}", records.len());
    for (i, r) in records.iter().enumerate() {
        println!(
            "  {}) [{}] {} — 오류: {} → 정정: {} ({})",
            i + 1,
            r.date,
            r.article,
            r.error,
            r.fix,
            r.status
        );
    }
    println!();
}

/// Build the prompt for generating a correction report (정정보도문).
pub fn build_correction_report_prompt(article_title: &str, records: &[CorrectionRecord]) -> String {
    let records_text = records
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "{}. 기사: {} | 날짜: {} | 오류: {} | 정정: {}",
                i + 1,
                r.article,
                r.date,
                r.error,
                r.fix
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "아래 정정 기록을 바탕으로 정정보도문을 작성해주세요.\n\n\
         ## 정정 기록\n{records_text}\n\n\
         ## 작성 기준 (한국 언론중재법)\n\
         정정보도는 다음 규정을 반드시 준수해야 합니다:\n\n\
         1. **게재 위치·크기**: 원보도와 같은 크기, 같은 위치에 게재 (언론중재법 제15조)\n\
         2. **게재 시한**: 청구를 받은 날로부터 3일 이내(일간) 또는 다음 발행일(주간 이상)에 게재\n\
         3. **정정보도문 형식**:\n\
            - 제목: \"[정정보도] 〈원보도 제목〉 관련\"\n\
            - 원보도 일자, 매체, 제목 명시\n\
            - 오류 내용과 정정 내용을 명확히 구분하여 기술\n\
            - 사과 또는 유감 표명 포함\n\
         4. **반론보도와 구분**: 정정보도는 사실의 오류를 바로잡는 것, \
            반론보도는 피해자의 입장을 전달하는 것\n\
         5. **재발 방지**: 해당 오류의 원인과 재발 방지 대책 언급\n\n\
         {}\
         정정보도문을 완성된 형태로 작성해주세요. \
         위 법적 요건을 모두 반영하고, 언론사 실무에서 바로 사용할 수 있는 수준으로 작성하세요.",
        if !article_title.is_empty() {
            format!("대상 기사 제목: \"{article_title}\"\n\n")
        } else {
            String::new()
        }
    )
}

async fn correction_report(
    agent: &mut Agent,
    args: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let records = load_corrections();
    if records.is_empty() {
        println!("{DIM}  정정 기록이 없습니다. 먼저 /correction add로 기록을 추가하세요.{RESET}\n");
        return;
    }

    // Parse optional --article filter
    let article_filter = if let Some(rest) = args.strip_prefix("--article") {
        rest.trim().to_string()
    } else {
        args.to_string()
    };

    let filtered: Vec<CorrectionRecord> = if article_filter.is_empty() {
        records
    } else {
        records
            .into_iter()
            .filter(|r| r.article.contains(&article_filter))
            .collect()
    };

    if filtered.is_empty() {
        println!(
            "{DIM}  \"{article_filter}\"에 해당하는 정정 기록이 없습니다.{RESET}\n"
        );
        return;
    }

    let prompt = build_correction_report_prompt(&article_filter, &filtered);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save the correction report
    if !response.trim().is_empty() {
        let slug = topic_to_slug(
            if article_filter.is_empty() {
                "correction"
            } else {
                &article_filter
            },
            50,
        );
        let path = correction_report_path_with_date(&slug, &today_str());
        match save_correction_report(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 정정보도문 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  정정보도문 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

/// Build correction report file path with explicit date (for testing).
pub fn correction_report_path_with_date(slug: &str, date: &str) -> std::path::PathBuf {
    let filename = if slug.is_empty() {
        format!("{date}_correction.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(CORRECTIONS_DIR).join(filename)
}

fn save_correction_report(
    path: &std::path::Path,
    content: &str,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

// ── /archive — 출고 기사 아카이브 시스템 ────────────────────────────────

const ARCHIVE_DIR: &str = ".journalist/archive";
const ARCHIVE_INDEX: &str = ".journalist/archive/index.json";

pub fn handle_archive(input: &str) {
    let args = input.strip_prefix("/archive").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "save" => {
            let rest = args.strip_prefix("save").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /archive save <제목> [--section 경제] [--type 스트레이트] [--tags 반도체,삼성]{RESET}");
                println!("{DIM}  본문: 표준 입력(파이프) 또는 --file <경로> 로 지정{RESET}\n");
            } else {
                archive_save(rest);
            }
        }
        "list" => {
            let rest = args.strip_prefix("list").unwrap_or("").trim();
            archive_list(rest);
        }
        "search" => {
            let rest = args.strip_prefix("search").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /archive search <키워드>{RESET}");
                println!("{DIM}  예시: /archive search 반도체{RESET}\n");
            } else {
                archive_search(rest);
            }
        }
        "view" => {
            let rest = args.strip_prefix("view").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /archive view <번호>{RESET}");
                println!("{DIM}  예시: /archive view 3{RESET}\n");
            } else {
                archive_view(rest);
            }
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /archive [save|list|search|view]{RESET}\n");
        }
    }
}

/// Parse archive save arguments: extract title, --section, --type, --tags, --file.
fn parse_archive_save_args(args: &str) -> (String, String, String, Vec<String>, Option<String>) {
    let mut title = String::new();
    let mut section = String::new();
    let mut article_type = String::new();
    let mut tags: Vec<String> = Vec::new();
    let mut file_path: Option<String> = None;

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--section" => {
                if i + 1 < parts.len() {
                    section = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--type" => {
                if i + 1 < parts.len() {
                    article_type = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--tags" => {
                if i + 1 < parts.len() {
                    tags = parts[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--file" => {
                if i + 1 < parts.len() {
                    file_path = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                if title.is_empty() {
                    title = parts[i].to_string();
                } else {
                    // Accumulate multi-word title until we hit a flag
                    title.push(' ');
                    title.push_str(parts[i]);
                }
                i += 1;
            }
        }
    }

    (title, section, article_type, tags, file_path)
}

fn archive_save(args: &str) {
    let (title, section, article_type, tags, file_path) = parse_archive_save_args(args);

    if title.is_empty() {
        println!("{DIM}  제목을 입력하세요.{RESET}\n");
        return;
    }

    // Read body from file if --file provided
    let body = if let Some(ref fp) = file_path {
        match std::fs::read_to_string(fp) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                return;
            }
        }
    } else {
        String::new()
    };

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);
    let date = &timestamp[..10]; // "YYYY-MM-DD"

    let mut index = load_archive_index_from(std::path::Path::new(ARCHIVE_INDEX));
    let id = index.len() + 1;

    // Save body text file
    let text_filename = format!("{id:04}.txt");
    let text_path = std::path::Path::new(ARCHIVE_DIR).join(&text_filename);
    ensure_sources_dir_at(&text_path);
    let _ = std::fs::write(&text_path, &body);

    // Build metadata entry
    let entry = serde_json::json!({
        "id": id,
        "title": title,
        "date": date,
        "section": section,
        "type": article_type,
        "tags": tags,
        "file": text_filename,
    });

    index.push(entry);
    save_archive_index_to(&index, std::path::Path::new(ARCHIVE_INDEX));

    println!("{DIM}  기사 아카이브 저장됨: #{id} \"{title}\" [{date}]{RESET}");
    if !section.is_empty() {
        println!("{DIM}    섹션: {section}{RESET}");
    }
    if !article_type.is_empty() {
        println!("{DIM}    유형: {article_type}{RESET}");
    }
    if !tags.is_empty() {
        println!("{DIM}    태그: {}{RESET}", tags.join(", "));
    }
    println!();
}

fn archive_list(args: &str) {
    archive_list_from(std::path::Path::new(ARCHIVE_INDEX), args);
}

fn archive_list_from(index_path: &std::path::Path, args: &str) {
    let index = load_archive_index_from(index_path);
    if index.is_empty() {
        println!("{DIM}  아카이브가 비어있습니다.");
        println!("  /archive save <제목> 으로 기사를 저장하세요.{RESET}\n");
        return;
    }

    // Parse --section and --recent flags
    let mut section_filter: Option<String> = None;
    let mut recent_limit: Option<usize> = None;
    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--section" => {
                if i + 1 < parts.len() {
                    section_filter = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--recent" => {
                if i + 1 < parts.len() {
                    recent_limit = parts[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    // Filter by section
    let filtered: Vec<&serde_json::Value> = index
        .iter()
        .filter(|e| {
            if let Some(ref sec) = section_filter {
                e["section"].as_str().unwrap_or("") == sec
            } else {
                true
            }
        })
        .collect();

    // Apply recent limit (from the end)
    let display: Vec<&&serde_json::Value> = if let Some(n) = recent_limit {
        filtered.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
    } else {
        filtered.iter().collect()
    };

    if display.is_empty() {
        println!("{DIM}  조건에 맞는 기사가 없습니다.{RESET}\n");
        return;
    }

    println!("{BOLD}  기사 아카이브 ({} 건){RESET}", display.len());
    println!("{DIM}  ─────────────────────────────────────{RESET}");
    for entry in &display {
        let id = entry["id"].as_u64().unwrap_or(0);
        let date = entry["date"].as_str().unwrap_or("?");
        let title = entry["title"].as_str().unwrap_or("?");
        let section = entry["section"].as_str().unwrap_or("");
        let sec_display = if section.is_empty() {
            String::new()
        } else {
            format!(" [{section}]")
        };
        println!("{DIM}  {id:>4}. {date}  {title}{sec_display}{RESET}");
    }
    println!();
}

fn archive_search(keyword: &str) {
    archive_search_in(
        std::path::Path::new(ARCHIVE_INDEX),
        std::path::Path::new(ARCHIVE_DIR),
        keyword,
    );
}

fn archive_search_in(index_path: &std::path::Path, archive_dir: &std::path::Path, keyword: &str) {
    let index = load_archive_index_from(index_path);
    if index.is_empty() {
        println!("{DIM}  아카이브가 비어있습니다.{RESET}\n");
        return;
    }

    let keyword_lower = keyword.to_lowercase();
    let mut results: Vec<&serde_json::Value> = Vec::new();

    for entry in &index {
        // Search in title
        let title = entry["title"].as_str().unwrap_or("");
        if title.to_lowercase().contains(&keyword_lower) {
            results.push(entry);
            continue;
        }

        // Search in tags
        if let Some(tags) = entry["tags"].as_array() {
            if tags.iter().any(|t| {
                t.as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&keyword_lower)
            }) {
                results.push(entry);
                continue;
            }
        }

        // Search in body text
        let filename = entry["file"].as_str().unwrap_or("");
        if !filename.is_empty() {
            let text_path = archive_dir.join(filename);
            if let Ok(body) = std::fs::read_to_string(&text_path) {
                if body.to_lowercase().contains(&keyword_lower) {
                    results.push(entry);
                }
            }
        }
    }

    if results.is_empty() {
        println!("{DIM}  \"{keyword}\" 검색 결과 없음.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  \"{keyword}\" 검색 결과 ({} 건){RESET}",
        results.len()
    );
    println!("{DIM}  ─────────────────────────────────────{RESET}");
    for entry in &results {
        let id = entry["id"].as_u64().unwrap_or(0);
        let date = entry["date"].as_str().unwrap_or("?");
        let title = entry["title"].as_str().unwrap_or("?");
        let section = entry["section"].as_str().unwrap_or("");
        let sec_display = if section.is_empty() {
            String::new()
        } else {
            format!(" [{section}]")
        };
        println!("{DIM}  {id:>4}. {date}  {title}{sec_display}{RESET}");
    }
    println!();
}

fn archive_view(id_str: &str) {
    archive_view_in(
        std::path::Path::new(ARCHIVE_INDEX),
        std::path::Path::new(ARCHIVE_DIR),
        id_str,
    );
}

fn archive_view_in(
    index_path: &std::path::Path,
    archive_dir: &std::path::Path,
    id_str: &str,
) {
    let id: usize = match id_str.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  유효한 번호를 입력하세요.{RESET}\n");
            return;
        }
    };

    let index = load_archive_index_from(index_path);
    let entry = index.iter().find(|e| e["id"].as_u64() == Some(id as u64));
    let entry = match entry {
        Some(e) => e,
        None => {
            eprintln!("{RED}  #{id} 기사를 찾을 수 없습니다.{RESET}\n");
            return;
        }
    };

    let title = entry["title"].as_str().unwrap_or("?");
    let date = entry["date"].as_str().unwrap_or("?");
    let section = entry["section"].as_str().unwrap_or("");
    let article_type = entry["type"].as_str().unwrap_or("");
    let tags: Vec<&str> = entry["tags"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    println!("{BOLD}  #{id} {title}{RESET}");
    println!("{DIM}  날짜: {date}{RESET}");
    if !section.is_empty() {
        println!("{DIM}  섹션: {section}{RESET}");
    }
    if !article_type.is_empty() {
        println!("{DIM}  유형: {article_type}{RESET}");
    }
    if !tags.is_empty() {
        println!("{DIM}  태그: {}{RESET}", tags.join(", "));
    }
    println!("{DIM}  ─────────────────────────────────────{RESET}");

    let filename = entry["file"].as_str().unwrap_or("");
    if !filename.is_empty() {
        let text_path = archive_dir.join(filename);
        match std::fs::read_to_string(&text_path) {
            Ok(body) => {
                if body.is_empty() {
                    println!("{DIM}  (본문 없음){RESET}");
                } else {
                    println!("{DIM}{body}{RESET}");
                }
            }
            Err(_) => {
                println!("{DIM}  (본문 파일을 읽을 수 없습니다){RESET}");
            }
        }
    }
    println!();
}

fn load_archive_index_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_archive_index_to(index: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(index) {
        let _ = std::fs::write(path, json);
    }
}

// ── /publish ────────────────────────────────────────────────────────────

/// Possible outcomes for each pipeline step.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishStepResult {
    /// Step completed successfully.
    Pass(String),
    /// Step failed (e.g. file not found, empty article).
    Fail(String),
    /// Legal step found 위험 — pipeline must halt.
    Blocked(String),
}

/// Run the publish pipeline: checklist → proofread → legal → export.
/// Returns a vec of (step_name, result) pairs.
/// Stops early if legal returns 위험.
pub async fn handle_publish(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/publish").unwrap_or("").trim();

    // Determine the target file (--file flag or latest draft)
    let file_path = if args.contains("--file") {
        let parts: Vec<&str> = args.splitn(3, "--file").collect();
        let after = parts.get(1).unwrap_or(&"").trim();
        let path = after.split_whitespace().next().unwrap_or("").to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    } else if !args.is_empty() {
        Some(args.to_string())
    } else {
        find_latest_draft().map(|p| p.to_string_lossy().to_string())
    };

    let file_path = match file_path {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /publish <파일> 또는 /publish --file <경로>{RESET}");
            println!(
                "{DIM}  출고 파이프라인: checklist → proofread → legal → export 를 순차 실행합니다.{RESET}"
            );
            println!("{DIM}  legal 단계에서 🚨 위험 판정 시 파이프라인을 중단합니다.{RESET}\n");
            return;
        }
    };

    // Verify file exists
    if !std::path::Path::new(&file_path).exists() {
        eprintln!("{RED}  파일을 찾을 수 없습니다: {file_path}{RESET}\n");
        return;
    }

    println!("\n{BOLD}  ══════════════════════════════════════{RESET}");
    println!("{BOLD}   🚀 출고 파이프라인 시작{RESET}");
    println!("{BOLD}   대상: {file_path}{RESET}");
    println!("{BOLD}  ══════════════════════════════════════{RESET}\n");

    let results =
        run_publish_pipeline(agent, session_total, model, &file_path).await;

    print_publish_report(&results);
}

/// Core pipeline logic, separated for testability of the report printer.
async fn run_publish_pipeline(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
    file_path: &str,
) -> Vec<(&'static str, PublishStepResult)> {
    let mut results: Vec<(&'static str, PublishStepResult)> = Vec::new();
    let steps: &[&str] = &["checklist", "proofread", "legal", "export"];

    for &step in steps {
        println!(
            "{CYAN}  ▶ [{step}] 단계 실행 중...{RESET}"
        );

        let result = match step {
            "checklist" => {
                let cmd = format!("/checklist --file {file_path}");
                handle_checklist(agent, &cmd, session_total, model).await;
                PublishStepResult::Pass("체크리스트 완료".to_string())
            }
            "proofread" => {
                let cmd = format!("/proofread --file {file_path}");
                handle_proofread(agent, &cmd, session_total, model).await;
                PublishStepResult::Pass("교열 완료".to_string())
            }
            "legal" => {
                // We need to capture the legal response to check for 위험
                let article = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        let msg = format!("파일 읽기 실패: {e}");
                        results.push((step, PublishStepResult::Fail(msg)));
                        break;
                    }
                };
                let prompt = match build_legal_prompt(&article) {
                    Some(p) => p,
                    None => {
                        results.push((
                            step,
                            PublishStepResult::Fail("빈 기사 — 법적 점검 불가".to_string()),
                        ));
                        break;
                    }
                };
                let response = run_prompt(agent, &prompt, session_total, model).await;
                auto_compact_if_needed(agent);

                // Save legal result (same as handle_legal)
                if !response.trim().is_empty() {
                    let slug = std::path::Path::new(file_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "legal".to_string());
                    let path = legal_file_path(&slug);
                    if let Ok(()) = save_legal(&path, &response) {
                        println!("{GREEN}  ✓ 법적 점검 저장: {}{RESET}", path.display());
                    }
                }

                // Check for 🚨 위험 — halt if found
                if response.contains("위험") {
                    PublishStepResult::Blocked(
                        "🚨 법적 리스크 '위험' 판정 — 파이프라인 중단".to_string(),
                    )
                } else {
                    PublishStepResult::Pass("법적 점검 통과".to_string())
                }
            }
            "export" => {
                let cmd = format!("/export {file_path}");
                handle_export(&cmd);
                PublishStepResult::Pass("내보내기 완료".to_string())
            }
            _ => unreachable!(),
        };

        let blocked = matches!(&result, PublishStepResult::Blocked(_));
        results.push((step, result));

        if blocked {
            // Mark remaining steps as skipped
            let done = results.len();
            for &remaining in &steps[done..] {
                results.push((
                    remaining,
                    PublishStepResult::Fail("이전 단계 중단으로 건너뜀".to_string()),
                ));
            }
            break;
        }
    }

    results
}

/// Print the publish pipeline summary report.
pub fn print_publish_report(results: &[(&str, PublishStepResult)]) {
    println!("\n{BOLD}  ──────────────────────────────────────{RESET}");
    println!("{BOLD}   📊 출고 파이프라인 결과{RESET}");
    println!("{BOLD}  ──────────────────────────────────────{RESET}\n");

    let mut pass_count = 0u32;
    let mut fail_count = 0u32;
    let mut blocked = false;

    for (step, result) in results {
        match result {
            PublishStepResult::Pass(msg) => {
                println!("   ✅ {step}: {msg}");
                pass_count += 1;
            }
            PublishStepResult::Fail(msg) => {
                println!("   ❌ {step}: {msg}");
                fail_count += 1;
            }
            PublishStepResult::Blocked(msg) => {
                println!("   🚨 {step}: {msg}");
                fail_count += 1;
                blocked = true;
            }
        }
    }

    println!();
    if blocked {
        println!(
            "{RED}  ⛔ 출고 중단 — 법적 리스크를 먼저 해결하세요 (통과: {pass_count}, 실패/중단: {fail_count}){RESET}\n"
        );
    } else if fail_count > 0 {
        println!(
            "{YELLOW}  ⚠ 일부 단계 실패 (통과: {pass_count}, 실패: {fail_count}){RESET}\n"
        );
    } else {
        println!(
            "{GREEN}  ✅ 출고 준비 완료! 모든 단계 통과 ({pass_count}/{pass_count}){RESET}\n"
        );
    }
}

// ── /anonymize ───────────────────────────────────────────────────────────

const ANONYMIZE_DIR: &str = ".journalist/anonymize";

/// Build anonymize result file path with an explicit date string.
pub fn anonymize_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_anonymize.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(ANONYMIZE_DIR).join(filename)
}

/// Build anonymize result file path with today's date.
pub fn anonymize_file_path(slug_source: &str) -> std::path::PathBuf {
    anonymize_file_path_with_date(slug_source, &today_str())
}

/// Save anonymized result to file. Creates the directory if needed.
fn save_anonymize(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Build the anonymize prompt for PII de-identification in Korean news articles.
pub fn build_anonymize_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    Some(format!(
        r#"당신은 한국 언론의 취재원 보호 전문가입니다. 아래 기사 텍스트에서 개인식별정보(PII)를 감지하고 익명화 처리하세요.

## 익명화 규칙
1. **실명** → A씨, B씨, C씨 등 (등장 순서대로 알파벳 부여, 동일 인물은 같은 알파벳 유지)
2. **소속·기관명** → A기관, B기관, C기관 등 (공공기관·정부부처·국제기구는 유지 가능)
3. **직함·직위** → 구체적 직함 제거, '관계자', '임원', '직원' 등으로 대체
4. **전화번호** → [전화번호 삭제]
5. **이메일** → [이메일 삭제]
6. **주소** → [주소 삭제] (시·도 단위는 유지 가능)
7. **주민등록번호·계좌번호 등** → [개인정보 삭제]
8. **나이·성별**: 기사 맥락에 필수적이면 유지, 아니면 삭제
9. **공인(대통령, 장관, 국회의원 등 공적 인물)**: 실명 유지 가능
10. **기업명**: 상장사·대기업은 유지, 중소기업·스타트업은 익명화 고려

## 출력 형식

### 감지된 개인식별정보

| # | 유형 | 원문 | 처리 | 비고 |
|---|------|------|------|------|
| 1 | 실명 | 홍길동 | A씨 | 취재원 |

### 익명화된 전문
(모든 PII가 처리된 전체 기사)

### 익명화 매핑표
(원문 ↔ 익명화 대응표, 내부 참조용)

| 익명 | 원문 | 유형 |
|------|------|------|
| A씨 | 홍길동 | 실명 |

### 주의사항
(익명화 과정에서 판단이 필요했던 사항, 공인 여부 판단 근거 등)

## 원문
{article}"#
    ))
}

/// Handle the `/anonymize` command: detect and anonymize PII in article text.
pub async fn handle_anonymize(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/anonymize").unwrap_or("").trim();
    let (file_path, inline_text) = parse_proofread_args(args);

    // Read article from file or inline
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_anonymize_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /anonymize <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /anonymize --file <경로>{RESET}");
            println!(
                "{DIM}  기사에서 실명·전화번호·이메일 등 개인식별정보를 감지하고 익명화합니다.{RESET}"
            );
            println!("{DIM}  탐사보도 초안 공유, 법률 검토 시 취재원 보호에 활용하세요.{RESET}\n");
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save anonymized result to .journalist/anonymize/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "anonymize".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "anonymize".to_string()
            } else {
                preview
            }
        };
        let path = anonymize_file_path(&slug_source);
        match save_anonymize(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 익명화 결과 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  익명화 결과 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /readability ────────────────────────────────────────────────────────

/// Readability metrics for Korean article text.
#[derive(Debug, PartialEq)]
pub struct ReadabilityMetrics {
    /// Average sentence length in characters.
    pub avg_sentence_len: f64,
    /// Ratio of long sentences (over 80 chars), 0.0–1.0.
    pub long_sentence_ratio: f64,
    /// Average number of sentences per paragraph.
    pub avg_paragraph_len: f64,
    /// Estimated passive voice ratio, 0.0–1.0.
    pub passive_ratio: f64,
    /// Jargon density (ratio of jargon-like words), 0.0–1.0.
    pub jargon_density: f64,
    /// Overall grade: A (best) through F (worst).
    pub grade: char,
    /// Total number of sentences detected.
    pub sentence_count: usize,
    /// Total number of paragraphs detected.
    pub paragraph_count: usize,
}

/// Split Korean text into sentences using sentence-ending markers.
///
/// Korean sentences typically end with 다, 요, 죠, 음, 임 followed by period/question/exclamation,
/// or just period/question/exclamation on their own.
fn split_korean_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if ch == '.' || ch == '!' || ch == '?' || ch == '。' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }
    }
    // Remaining text without terminal punctuation counts as a sentence if non-empty
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }
    sentences
}

/// Split text into paragraphs (groups of non-empty lines separated by blank lines).
fn split_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                paragraphs.push(trimmed);
            }
            current.clear();
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        paragraphs.push(trimmed);
    }
    paragraphs
}

/// Korean passive voice suffixes.
const PASSIVE_SUFFIXES: &[&str] = &[
    "되었다", "됐다", "되었습니다", "됐습니다", "되었으며", "되었고",
    "된다", "됩니다", "되고", "되며", "되어", "돼",
    "받았다", "받았습니다", "받게", "받는다",
    "당했다", "당했습니다", "당하고",
];

/// Common jargon / technical terms frequently found in Korean news articles.
const JARGON_TERMS: &[&str] = &[
    "거버넌스", "컨센서스", "패러다임", "이니셔티브", "로드맵",
    "리스크", "레버리지", "포트폴리오", "밸류에이션", "모멘텀",
    "인프라", "플랫폼", "컴플라이언스", "가이드라인", "프레임워크",
    "시너지", "이해관계자", "스테이크홀더", "디폴트", "모라토리엄",
    "유동성", "변동성", "펀더멘털", "스프레드", "디커플링",
];

/// Compute readability metrics for Korean article text.
pub fn compute_readability(text: &str) -> ReadabilityMetrics {
    let sentences = split_korean_sentences(text);
    let paragraphs = split_paragraphs(text);

    let sentence_count = sentences.len();
    let paragraph_count = paragraphs.len();

    // Average sentence length (character count excluding spaces)
    let avg_sentence_len = if sentence_count > 0 {
        let total_chars: usize = sentences
            .iter()
            .map(|s| s.chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        total_chars as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Long sentence ratio (over 80 chars excluding spaces)
    let long_sentence_ratio = if sentence_count > 0 {
        let long_count = sentences
            .iter()
            .filter(|s| s.chars().filter(|c| !c.is_whitespace()).count() > 80)
            .count();
        long_count as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Average paragraph length in sentences
    let avg_paragraph_len = if paragraph_count > 0 {
        let para_sentence_counts: Vec<usize> = paragraphs
            .iter()
            .map(|p| split_korean_sentences(p).len())
            .collect();
        let total: usize = para_sentence_counts.iter().sum();
        total as f64 / paragraph_count as f64
    } else {
        0.0
    };

    // Passive voice ratio
    let passive_ratio = if sentence_count > 0 {
        let passive_count = sentences
            .iter()
            .filter(|s| PASSIVE_SUFFIXES.iter().any(|suf| s.contains(suf)))
            .count();
        passive_count as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Jargon density: count jargon occurrences per word
    let words: Vec<&str> = text.split_whitespace().collect();
    let word_count = words.len();
    let jargon_density = if word_count > 0 {
        let jargon_count = words
            .iter()
            .filter(|w| JARGON_TERMS.iter().any(|j| w.contains(j)))
            .count();
        jargon_count as f64 / word_count as f64
    } else {
        0.0
    };

    // Grade: score 0–100, then map to A–F
    // Lower avg sentence len is better, lower long_sentence_ratio is better, etc.
    let mut score = 100.0_f64;

    // Penalty for long average sentence (ideal: ~30 chars for Korean)
    if avg_sentence_len > 30.0 {
        score -= (avg_sentence_len - 30.0) * 0.5;
    }

    // Penalty for long sentences
    score -= long_sentence_ratio * 30.0;

    // Penalty for long paragraphs (ideal: 2–4 sentences)
    if avg_paragraph_len > 4.0 {
        score -= (avg_paragraph_len - 4.0) * 5.0;
    }

    // Penalty for passive voice
    score -= passive_ratio * 20.0;

    // Penalty for jargon
    score -= jargon_density * 30.0;

    let score = score.max(0.0).min(100.0);
    let grade = if score >= 90.0 {
        'A'
    } else if score >= 80.0 {
        'B'
    } else if score >= 70.0 {
        'C'
    } else if score >= 60.0 {
        'D'
    } else if score >= 50.0 {
        'E'
    } else {
        'F'
    };

    ReadabilityMetrics {
        avg_sentence_len,
        long_sentence_ratio,
        avg_paragraph_len,
        passive_ratio,
        jargon_density,
        grade,
        sentence_count,
        paragraph_count,
    }
}

/// Handle `/readability [파일경로]` — show readability analysis for article text.
pub fn handle_readability(input: &str) {
    let arg = input.strip_prefix("/readability").unwrap_or("").trim();

    let (path, content) = if arg.is_empty() {
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => (p.to_string_lossy().to_string(), c),
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                eprintln!("{DIM}  분석할 파일이 없습니다. 경로를 지정하거나 /article로 초안을 먼저 작성하세요.{RESET}\n");
                return;
            }
        }
    } else {
        match std::fs::read_to_string(arg) {
            Ok(c) => (arg.to_string(), c),
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패 ({arg}): {e}{RESET}\n");
                return;
            }
        }
    };

    let m = compute_readability(&content);

    println!("{BOLD}  📖 가독성 분석: {path}{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");
    println!("  종합 등급             {}", grade_colored(m.grade));
    println!("  문장 수               {}", m.sentence_count);
    println!("  문단 수               {}", m.paragraph_count);
    println!(
        "  평균 문장 길이         {:.1}자",
        m.avg_sentence_len
    );
    println!(
        "  긴 문장 비율 (>80자)  {:.1}%",
        m.long_sentence_ratio * 100.0
    );
    println!(
        "  평균 문단 길이         {:.1}문장",
        m.avg_paragraph_len
    );
    println!(
        "  수동태 추정 비율       {:.1}%",
        m.passive_ratio * 100.0
    );
    println!(
        "  전문 용어 밀도         {:.1}%",
        m.jargon_density * 100.0
    );
    println!();

    // Tips
    if m.long_sentence_ratio > 0.3 {
        println!("  {YELLOW}💡 긴 문장이 많습니다. 80자 이하로 줄이면 가독성이 높아집니다.{RESET}");
    }
    if m.passive_ratio > 0.3 {
        println!(
            "  {YELLOW}💡 수동태 표현이 많습니다. 능동태로 바꾸면 더 명확해집니다.{RESET}"
        );
    }
    if m.jargon_density > 0.05 {
        println!("  {YELLOW}💡 전문 용어가 많습니다. 독자 눈높이에 맞게 풀어쓰는 것을 권장합니다.{RESET}");
    }
    if m.avg_paragraph_len > 5.0 {
        println!("  {YELLOW}💡 문단이 깁니다. 3~4문장 단위로 끊으면 읽기 편합니다.{RESET}");
    }
    println!();
}

/// Return a grade string with color.
fn grade_colored(grade: char) -> String {
    match grade {
        'A' => format!("{GREEN}{BOLD}{grade}{RESET}"),
        'B' => format!("{GREEN}{grade}{RESET}"),
        'C' => format!("{YELLOW}{grade}{RESET}"),
        'D' => format!("{YELLOW}{grade}{RESET}"),
        _ => format!("{RED}{grade}{RESET}"),
    }
}

// ── /improve ────────────────────────────────────────────────────────────

/// Directory for improve feedback results.
const IMPROVE_DIR: &str = ".journalist/improve";

/// Parse /improve arguments. Supports `--file <path>` and inline text.
/// Returns (file_path, remaining_text).
pub fn parse_improve_args(args: &str) -> (Option<String>, String) {
    // Reuse same parsing logic as proofread
    parse_proofread_args(args)
}

/// Build the improve prompt with editorial feedback criteria.
pub fn build_improve_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    Some(format!(
        r#"당신은 한국 종합일간지의 베테랑 편집데스크입니다. 아래 기사를 읽고, 이 기사를 더 좋은 기사로 만들기 위한 구체적 개선안을 제시하세요.

## 분석 항목

다음 5개 항목 각각에 대해 **(1) 현재 상태 평가** (10점 만점)와 **(2) 구체적 수정 제안**을 제시하세요.

### 1. 리드문 흡인력
- 첫 문단이 독자의 관심을 끌어당기는가?
- 핵심 뉴스 가치가 첫 2~3문장에 드러나는가?
- 더 강력한 리드를 쓸 수 있다면 대안을 제시하세요.

### 2. 문단 전환 자연스러움
- 문단과 문단 사이의 논리적 흐름이 자연스러운가?
- 비약이나 끊김이 있는 곳을 지적하고, 연결 방안을 제안하세요.

### 3. 인용문 활용도
- 인용문이 기사의 신뢰도와 생동감을 높이고 있는가?
- 인용문이 부족하거나 과다한 곳, 더 효과적인 배치 방법을 제안하세요.
- 인용문이 없다면, 어떤 취재원의 발언이 필요한지 제안하세요.

### 4. 구체성·수치 뒷받침
- 주장이나 서술에 구체적 수치·데이터·사례가 뒷받침되는가?
- 추상적 서술을 구체화할 수 있는 포인트를 지적하세요.
- 추가로 필요한 데이터나 팩트를 제안하세요.

### 5. 결론/마무리 완성도
- 기사의 마무리가 독자에게 여운이나 시사점을 남기는가?
- 미래 전망, 의미 부여, 독자 행동 유도 등 더 나은 마무리 방안을 제안하세요.

## 출력 형식

### 📊 종합 평가
| 항목 | 점수 | 한줄 평가 |
|------|------|-----------|
| 리드문 흡인력 | ?/10 | ... |
| 문단 전환 | ?/10 | ... |
| 인용문 활용 | ?/10 | ... |
| 구체성·수치 | ?/10 | ... |
| 결론 완성도 | ?/10 | ... |
| **종합** | **?/50** | ... |

### 📝 항목별 상세 피드백
(각 항목에 대해: 현재 상태 → 문제점 → 구체적 수정 제안)

### ✍️ 핵심 개선 Top 3
(가장 효과가 클 3가지 수정 사항을 우선순위로)

## 원문
{article}"#
    ))
}

/// Build improve result file path with an explicit date string.
pub fn improve_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_improve.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(IMPROVE_DIR).join(filename)
}

/// Build improve result file path with today's date.
pub fn improve_file_path(slug_source: &str) -> std::path::PathBuf {
    improve_file_path_with_date(slug_source, &today_str())
}

/// Save improve result to file. Creates the directory if needed.
fn save_improve(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the /improve command: AI-based article improvement suggestions.
pub async fn handle_improve(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/improve").unwrap_or("").trim();
    let (file_path, inline_text) = parse_improve_args(args);

    // Read article from file, latest draft, or inline text
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else if inline_text.is_empty() {
        // Try latest draft
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => {
                    println!(
                        "{DIM}  최근 초안 사용: {} ({} bytes){RESET}",
                        p.display(),
                        c.len()
                    );
                    c
                }
                Err(e) => {
                    eprintln!("{RED}  초안 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                println!("{DIM}  사용법: /improve <기사 텍스트>{RESET}");
                println!("{DIM}  또는:   /improve --file <경로>{RESET}");
                println!("{DIM}  또는:   /article로 초안을 먼저 작성하세요.{RESET}");
                println!(
                    "{DIM}  편집 데스크 수준의 기사 개선 제안을 AI가 제시합니다.{RESET}\n"
                );
                return;
            }
        }
    } else {
        inline_text
    };

    let prompt = match build_improve_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  기사 내용이 비어 있습니다.{RESET}\n");
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save improve result to .journalist/improve/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "improve".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "improve".to_string()
            } else {
                preview
            }
        };
        let path = improve_file_path(&slug_source);
        match save_improve(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 개선 제안 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  개선 제안 저장 실패: {e}{RESET}\n");
            }
        }
    }
}
// ── /multiformat ────────────────────────────────────────────────────────

const MULTIFORMAT_DIR: &str = ".journalist/multiformat";

/// Known multiformat output formats.
pub const MULTIFORMAT_FORMATS: &[&str] = &["broadcast", "online", "card", "brief"];

/// Parse `/multiformat` arguments: extract `--format <type>` and the article file path.
/// Returns `(Option<format>, Option<file_path>, remaining_text)`.
pub fn parse_multiformat_args(args: &str) -> (Option<String>, Option<String>, String) {
    let args = args.trim();
    if args.is_empty() {
        return (None, None, String::new());
    }

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut format: Option<String> = None;
    let mut file_path: Option<String> = None;
    let mut remaining = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        if tokens[i] == "--format" && i + 1 < tokens.len() {
            format = Some(tokens[i + 1].to_string());
            i += 2;
        } else if file_path.is_none() && std::path::Path::new(tokens[i]).extension().is_some() {
            file_path = Some(tokens[i].to_string());
            i += 1;
        } else {
            remaining.push(tokens[i]);
            i += 1;
        }
    }

    (format, file_path, remaining.join(" "))
}

/// Build the prompt for `/multiformat` based on the target format.
pub fn build_multiformat_prompt(article: &str, format: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    let format_instruction = match format {
        "broadcast" => {
            "방송 원고 형식으로 변환해주세요.\n\n\
             ## 형식 요구사항\n\n\
             ### 앵커 멘트 (Anchor Lead)\n\
             - 시청자의 주의를 끄는 1~2문장의 도입부\n\
             - 구어체, 명확한 발음 고려\n\n\
             ### 리포트 본문\n\
             - 구어체 문장 (읽었을 때 자연스러운 호흡 단위)\n\
             - 현장음(NAT), 인터뷰(BITE) 삽입 위치 표시\n\
             - 시간 표기: 한글 (예: 오늘 오후, 지난달)\n\
             - 영상 지시 (CG, 자료화면 등) 괄호로 표시\n\n\
             ### 앵커 마무리 (Anchor Out)\n\
             - 1문장 요약 또는 향후 전망"
        }
        "online" => {
            "온라인 기사 형식으로 변환해주세요.\n\n\
             ## 형식 요구사항\n\n\
             - 짧은 문단 (2~3문장)\n\
             - 소제목(##)으로 섹션 구분\n\
             - 핵심 수치·팩트는 **볼드** 처리\n\
             - 관련 키워드에 [하이퍼링크 구조] 표시\n\
             - 첫 문단에 핵심 내용 요약 (역피라미드)\n\
             - 모바일 가독성 고려: 한 문단 80자 이내 권장\n\
             - 마지막에 '관련 기사' 섹션 제안"
        }
        "card" => {
            "SNS 카드뉴스 형식으로 변환해주세요.\n\n\
             ## 형식 요구사항\n\n\
             - 총 5장 이내의 카드로 구성\n\
             - 각 카드는 `[카드 N]` 헤더로 구분\n\
             - 카드 1: 제목 + 핵심 메시지 (한 줄)\n\
             - 카드 2~4: 핵심 팩트, 수치, 인용 (각 카드 3줄 이내)\n\
             - 마지막 카드: 결론 또는 Call-to-Action\n\
             - 이모지 활용 가능\n\
             - 각 카드에 이미지 제안 간단히 괄호로 표기"
        }
        "brief" => {
            "뉴스 브리프(3줄 요약) 형식으로 변환해주세요.\n\n\
             ## 형식 요구사항\n\n\
             - 정확히 3줄로 요약\n\
             - 1줄: 무엇이 일어났는가 (What)\n\
             - 2줄: 왜 중요한가 (Why it matters)\n\
             - 3줄: 앞으로 어떻게 되는가 (What's next)\n\
             - 각 줄은 한 문장, 50자 이내 권장\n\
             - 부호 없이 간결한 문체"
        }
        _ => return None,
    };

    Some(format!(
        "아래 기사를 {format_instruction}\n\n\
         ---\n\n\
         {article}"
    ))
}

/// Build multiformat output file path using today's date.
pub fn multiformat_file_path(slug: &str, format: &str) -> std::path::PathBuf {
    multiformat_file_path_with_date(slug, format, &today_str())
}

/// Build multiformat output file path with an explicit date (for testing).
pub fn multiformat_file_path_with_date(
    slug: &str,
    format: &str,
    date: &str,
) -> std::path::PathBuf {
    let slug = topic_to_slug(slug, 50);
    let filename = if slug.is_empty() {
        format!("{date}_{format}.md")
    } else {
        format!("{date}_{slug}_{format}.md")
    };
    std::path::PathBuf::from(MULTIFORMAT_DIR).join(filename)
}

/// Save multiformat result to file.
fn save_multiformat(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/multiformat` command: convert an article to a different media format.
pub async fn handle_multiformat(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/multiformat").unwrap_or("").trim();
    let (format, file_path, inline_text) = parse_multiformat_args(args);

    // Read article from file or inline text
    let article = if let Some(ref path) = file_path {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {path} ({} bytes){RESET}",
                    content.len()
                );
                if inline_text.is_empty() {
                    content
                } else {
                    format!("{content}\n\n{inline_text}")
                }
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {path} — {e}{RESET}\n");
                return;
            }
        }
    } else {
        inline_text
    };

    let format_str = format.as_deref().unwrap_or("");

    // Validate format
    if !MULTIFORMAT_FORMATS.contains(&format_str) || article.trim().is_empty() {
        println!("{DIM}  사용법: /multiformat <기사파일> --format <포맷>{RESET}");
        println!("{DIM}  포맷:   broadcast (방송원고) | online (온라인기사) | card (카드뉴스) | brief (3줄요약){RESET}");
        println!("{DIM}  예시:   /multiformat article.md --format broadcast{RESET}");
        println!("{DIM}  예시:   /multiformat article.md --format card{RESET}");
        println!(
            "{DIM}  기사를 다매체 포맷으로 변환합니다.{RESET}\n"
        );
        return;
    }

    let prompt = match build_multiformat_prompt(&article, format_str) {
        Some(p) => p,
        None => return,
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save result to .journalist/multiformat/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "multiformat".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "multiformat".to_string()
            } else {
                preview
            }
        };
        let path = multiformat_file_path(&slug_source, format_str);
        match save_multiformat(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ {format_str} 포맷 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  저장 실패: {e}{RESET}\n");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::commands_project::*;
    use crate::commands_research::*;
    use crate::commands_writing::*;
    use crate::commands_workflow::*;

    fn temp_archive_paths() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let archive_dir = dir.path().join("archive");
        std::fs::create_dir_all(&archive_dir).unwrap();
        let index_path = archive_dir.join("index.json");
        (dir, index_path, archive_dir)
    }


    #[test]
    fn checklist_prompt_empty_returns_none() {
        assert!(build_checklist_prompt("").is_none());
        assert!(build_checklist_prompt("   ").is_none());
    }

    #[test]
    fn checklist_prompt_contains_all_categories() {
        let prompt = build_checklist_prompt("테스트 기사 초안").unwrap();
        assert!(prompt.contains("육하원칙"));
        assert!(prompt.contains("출처 명시"));
        assert!(prompt.contains("중립성"));
        assert!(prompt.contains("[확인 필요]"));
        assert!(prompt.contains("법적 리스크"));
        assert!(prompt.contains("숫자/날짜"));
        assert!(prompt.contains("테스트 기사 초안"));
    }

    #[test]
    fn checklist_file_path_with_source() {
        let path = checklist_file_path_with_date("반도체 수출 기사", "2026-03-18");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/checklist/2026-03-18_반도체-수출-기사.md"
        );
    }

    #[test]
    fn checklist_file_path_empty_slug() {
        let path = checklist_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/checklist/2026-03-18_checklist.md"
        );
    }

    #[test]
    fn save_checklist_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sub").join("checklist.md");
        save_checklist(&path, "체크리스트 결과").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "체크리스트 결과");
    }

    #[test]
    fn parse_checklist_args_inline() {
        let (file, text) = parse_checklist_args("기사 초안 텍스트");
        assert!(file.is_none());
        assert_eq!(text, "기사 초안 텍스트");
    }

    #[test]
    fn parse_checklist_args_file_flag() {
        let (file, text) = parse_checklist_args("--file draft.md");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_checklist_args_file_with_extra() {
        let (file, text) = parse_checklist_args("--file draft.md 추가 메모");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert_eq!(text, "추가 메모");
    }

    #[test]
    fn checklist_file_read_integration() {
        let dir = tempfile::TempDir::new().unwrap();
        let article_file = dir.path().join("article.md");
        std::fs::write(&article_file, "기사 초안 내용입니다").unwrap();
        let content = std::fs::read_to_string(&article_file).unwrap();
        assert_eq!(content, "기사 초안 내용입니다");
        let prompt = build_checklist_prompt(&content);
        assert!(prompt.is_some());
        assert!(prompt.unwrap().contains("기사 초안 내용입니다"));
    }

    #[test]
    fn parse_translate_args_inline_text() {
        let (file, text) = parse_translate_args("The Federal Reserve raised rates.");
        assert!(file.is_none());
        assert_eq!(text, "The Federal Reserve raised rates.");
    }

    #[test]
    fn parse_translate_args_file_flag() {
        let (file, text) = parse_translate_args("--file article.txt");
        assert_eq!(file.as_deref(), Some("article.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_translate_args_file_with_extra_text() {
        let (file, text) = parse_translate_args("--file article.txt additional context");
        assert_eq!(file.as_deref(), Some("article.txt"));
        assert_eq!(text, "additional context");
    }

    #[test]
    fn parse_translate_args_empty() {
        let (file, text) = parse_translate_args("");
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_translate_prompt_basic() {
        let prompt = build_translate_prompt("The Fed raised rates by 25bp.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("The Fed raised rates by 25bp."));
        assert!(p.contains("한국 독자"));
        assert!(p.contains("현지화"));
    }

    #[test]
    fn build_translate_prompt_empty_returns_none() {
        assert!(build_translate_prompt("").is_none());
        assert!(build_translate_prompt("   ").is_none());
    }

    #[test]
    fn translate_file_path_with_topic() {
        let path = translate_file_path_with_date("Fed rate hike", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/translate/2026-03-18_fed-rate-hike.md")
        );
    }

    #[test]
    fn translate_file_path_empty_topic() {
        let path = translate_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/translate/2026-03-18_translate.md")
        );
    }

    #[test]
    fn save_translate_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("translate").join("test.md");
        let result = save_translate(&path, "# 번역 결과\n\n내용");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("번역 결과"));
    }

    #[test]
    fn parse_headline_args_inline_text() {
        let (file, text) = parse_headline_args("삼성전자 1분기 실적 발표");
        assert!(file.is_none());
        assert_eq!(text, "삼성전자 1분기 실적 발표");
    }

    #[test]
    fn parse_headline_args_file_flag() {
        let (file, text) = parse_headline_args("--file draft.txt");
        assert_eq!(file.as_deref(), Some("draft.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_headline_args_file_with_extra_text() {
        let (file, text) = parse_headline_args("--file draft.txt 추가 맥락");
        assert_eq!(file.as_deref(), Some("draft.txt"));
        assert_eq!(text, "추가 맥락");
    }

    #[test]
    fn parse_headline_args_empty() {
        let (file, text) = parse_headline_args("");
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_headline_prompt_basic() {
        let prompt = build_headline_prompt("삼성전자가 1분기 영업이익 15조원을 기록했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("삼성전자가 1분기 영업이익 15조원을 기록했다."));
        assert!(p.contains("헤드라인"));
        assert!(p.contains("스트레이트"));
        assert!(p.contains("분석"));
        assert!(p.contains("피처"));
        assert!(p.contains("클릭유도"));
    }

    #[test]
    fn build_headline_prompt_empty_returns_none() {
        assert!(build_headline_prompt("").is_none());
        assert!(build_headline_prompt("   ").is_none());
    }

    #[test]
    fn headline_file_path_with_topic() {
        let path = headline_file_path_with_date("삼성전자 실적", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/headline/2026-03-18_삼성전자-실적.md")
        );
    }

    #[test]
    fn headline_file_path_empty_slug() {
        let path = headline_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/headline/2026-03-18_headline.md")
        );
    }

    #[test]
    fn save_headline_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("headline").join("test.md");
        let result = save_headline(&path, "# 헤드라인 후보\n\n[스트레이트] 테스트");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("헤드라인 후보"));
    }

    #[test]
    fn parse_rewrite_args_inline_text() {
        let (style, length, file, text) = parse_rewrite_args("삼성전자 기사 본문");
        assert!(style.is_none());
        assert!(length.is_none());
        assert!(file.is_none());
        assert_eq!(text, "삼성전자 기사 본문");
    }

    #[test]
    fn parse_rewrite_args_with_style() {
        let (style, length, file, text) = parse_rewrite_args("--style 요약 기사 본문");
        assert_eq!(style.as_deref(), Some("요약"));
        assert!(length.is_none());
        assert!(file.is_none());
        assert_eq!(text, "기사 본문");
    }

    #[test]
    fn parse_rewrite_args_with_all_options() {
        let (style, length, file, text) =
            parse_rewrite_args("--style 피처 --length 500 --file draft.txt 추가 맥락");
        assert_eq!(style.as_deref(), Some("피처"));
        assert_eq!(length.as_deref(), Some("500"));
        assert_eq!(file.as_deref(), Some("draft.txt"));
        assert_eq!(text, "추가 맥락");
    }

    #[test]
    fn parse_rewrite_args_file_only() {
        let (style, length, file, text) = parse_rewrite_args("--file article.txt");
        assert!(style.is_none());
        assert!(length.is_none());
        assert_eq!(file.as_deref(), Some("article.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_rewrite_args_empty() {
        let (style, length, file, text) = parse_rewrite_args("");
        assert!(style.is_none());
        assert!(length.is_none());
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_rewrite_prompt_basic() {
        let prompt = build_rewrite_prompt("삼성전자가 1분기 실적을 발표했다.", None, None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("삼성전자가 1분기 실적을 발표했다."));
        assert!(p.contains("스트레이트"));
        assert!(p.contains("재작성"));
    }

    #[test]
    fn build_rewrite_prompt_with_style() {
        let prompt =
            build_rewrite_prompt("기사 본문입니다.", Some("피처"), None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("피처"));
        assert!(p.contains("내러티브"));
    }

    #[test]
    fn build_rewrite_prompt_with_length() {
        let prompt =
            build_rewrite_prompt("기사 본문입니다.", Some("요약"), Some("300"));
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("요약"));
        assert!(p.contains("300자"));
    }

    #[test]
    fn build_rewrite_prompt_sns_style() {
        let prompt = build_rewrite_prompt("기사 본문.", Some("sns"), None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("SNS"));
    }

    #[test]
    fn build_rewrite_prompt_custom_style() {
        let prompt = build_rewrite_prompt("기사 본문.", Some("뉴스레터"), None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("뉴스레터"));
    }

    #[test]
    fn build_rewrite_prompt_empty_returns_none() {
        assert!(build_rewrite_prompt("", None, None).is_none());
        assert!(build_rewrite_prompt("   ", None, None).is_none());
    }

    #[test]
    fn rewrite_file_path_with_topic() {
        let path = rewrite_file_path_with_date("삼성전자 실적", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/drafts/2026-03-18_삼성전자-실적.md")
        );
    }

    #[test]
    fn rewrite_file_path_empty_slug() {
        let path = rewrite_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/drafts/2026-03-18_rewrite.md")
        );
    }

    #[test]
    fn save_rewrite_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("drafts").join("test.md");
        let result = save_rewrite(&path, "# 재작성\n\n재작성된 기사 본문");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("재작성"));
    }

    #[test]
    fn build_summary_prompt_basic() {
        let prompt = build_summary_prompt("정부가 오늘 새로운 부동산 정책을 발표했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("정부가 오늘 새로운 부동산 정책을 발표했다."));
        assert!(p.contains("3~5줄"));
        assert!(p.contains("핵심 요약"));
    }

    #[test]
    fn build_summary_prompt_empty_returns_none() {
        assert!(build_summary_prompt("").is_none());
        assert!(build_summary_prompt("   ").is_none());
    }

    #[test]
    fn resolve_summary_input_inline_text() {
        let result = resolve_summary_input("정부가 부동산 정책을 발표했다");
        assert_eq!(result, Some("정부가 부동산 정책을 발표했다".to_string()));
    }

    #[test]
    fn resolve_summary_input_empty() {
        assert!(resolve_summary_input("").is_none());
        assert!(resolve_summary_input("   ").is_none());
    }

    #[test]
    fn resolve_summary_input_reads_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test_doc.txt");
        std::fs::write(&file_path, "보도자료 내용입니다.").unwrap();
        let result = resolve_summary_input(file_path.to_str().unwrap());
        assert_eq!(result, Some("보도자료 내용입니다.".to_string()));
    }

    #[test]
    fn resolve_summary_input_nonexistent_file_treated_as_text() {
        let result = resolve_summary_input("no_such_file_xyz.txt");
        // Non-existent file path is treated as inline text
        assert_eq!(result, Some("no_such_file_xyz.txt".to_string()));
    }

    #[test]
    fn stats_empty_text() {
        let stats = compute_text_stats("");
        assert_eq!(stats.chars_with_spaces, 0);
        assert_eq!(stats.chars_without_spaces, 0);
        assert_eq!(stats.words, 0);
        assert_eq!(stats.sentences, 0);
        assert_eq!(stats.paragraphs, 0);
        assert_eq!(stats.reading_time_secs, 0);
    }

    #[test]
    fn stats_single_sentence() {
        let text = "오늘 서울 날씨는 맑음.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.chars_with_spaces, 13);
        assert_eq!(stats.chars_without_spaces, 10);
        assert_eq!(stats.words, 4); // "오늘" "서울" "날씨는" "맑음."
        assert_eq!(stats.sentences, 1);
        assert_eq!(stats.paragraphs, 1);
    }

    #[test]
    fn stats_multiple_paragraphs() {
        let text = "첫 번째 문단입니다.\n\n두 번째 문단입니다.\n\n세 번째 문단입니다.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.paragraphs, 3);
        assert_eq!(stats.sentences, 3);
    }

    #[test]
    fn stats_reading_time() {
        // 500 chars (no spaces) → 60 seconds
        let text = "가".repeat(500);
        let stats = compute_text_stats(&text);
        assert_eq!(stats.reading_time_secs, 60);
    }

    #[test]
    fn stats_mixed_punctuation() {
        let text = "정말요? 네! 좋습니다.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.sentences, 3);
    }

    #[test]
    fn format_reading_time_seconds_only() {
        assert_eq!(format_reading_time(30), "30초");
    }

    #[test]
    fn format_reading_time_minutes_only() {
        assert_eq!(format_reading_time(120), "2분");
    }

    #[test]
    fn format_reading_time_mixed() {
        assert_eq!(format_reading_time(90), "1분 30초");
    }

    #[test]
    fn stats_words_english() {
        let text = "Hello world. This is a test.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.words, 6);
        assert_eq!(stats.sentences, 2);
    }

    #[test]
    fn draft_versions_dir_uses_slug() {
        let dir = draft_versions_dir("테스트 기사");
        assert!(dir.to_string_lossy().contains("테스트-기사"));
        assert!(dir.starts_with(DRAFT_VERSIONS_BASE));
    }

    #[test]
    fn draft_next_version_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("draft-test");
        // Dir doesn't exist yet
        assert_eq!(next_version_number(&dir), 1);
        // Create dir, still empty
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(next_version_number(&dir), 1);
    }

    #[test]
    fn draft_next_version_increments() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("draft-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("v1.md"), "first").unwrap();
        assert_eq!(next_version_number(&dir), 2);
        std::fs::write(dir.join("v2.md"), "second").unwrap();
        assert_eq!(next_version_number(&dir), 3);
    }

    #[test]
    fn draft_list_versions_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("draft-test");
        std::fs::create_dir_all(&dir).unwrap();
        // Create out of order
        std::fs::write(dir.join("v3.md"), "third").unwrap();
        std::fs::write(dir.join("v1.md"), "first").unwrap();
        std::fs::write(dir.join("v2.md"), "second").unwrap();
        // Also a non-version file
        std::fs::write(dir.join("notes.txt"), "ignore").unwrap();

        let versions = list_versions(&dir);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].0, 1);
        assert_eq!(versions[1].0, 2);
        assert_eq!(versions[2].0, 3);
    }

    #[test]
    fn draft_list_versions_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("nonexistent");
        let versions = list_versions(&dir);
        assert!(versions.is_empty());
    }

    #[test]
    fn draft_format_unix_timestamp_epoch() {
        // 2024-01-01 00:00 UTC = 1704067200
        let s = format_unix_timestamp(1_704_067_200);
        assert_eq!(s, "2024-01-01 00:00");
    }

    #[test]
    fn draft_format_unix_timestamp_nonzero_time() {
        // 2025-06-15 14:30 UTC = 1750000200
        let s = format_unix_timestamp(1_750_000_200);
        assert!(s.starts_with("2025-"));
        assert!(s.contains(':'));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_headings() {
        let md = "# 제목\n\n본문 내용입니다.\n\n## 소제목\n\n더 많은 내용.";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains('#'));
        assert!(plain.contains("제목"));
        assert!(plain.contains("본문 내용입니다."));
        assert!(plain.contains("소제목"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_bold_italic() {
        let md = "이것은 **굵은** 글씨와 *기울임* 입니다.";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains("**"));
        assert!(!plain.contains('*'));
        assert!(plain.contains("굵은"));
        assert!(plain.contains("기울임"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_links() {
        let md = "자세한 내용은 [여기](https://example.com)를 참고하세요.";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains("https://"));
        assert!(!plain.contains('['));
        assert!(plain.contains("여기"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_images() {
        let md = "이미지: ![대체텍스트](image.png)";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains("image.png"));
        assert!(plain.contains("대체텍스트"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_list_markers() {
        let md = "- 항목1\n- 항목2\n1. 번호항목";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.starts_with("- "));
        assert!(plain.contains("항목1"));
        assert!(plain.contains("번호항목"));
    }

    #[test]
    fn export_markdown_to_html_basic_structure() {
        let md = "# 제목\n\n본문 내용.";
        let html = markdown_to_html(md);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<h1>제목</h1>"));
        assert!(html.contains("<p>본문 내용.</p>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn export_markdown_to_html_blockquote() {
        let md = "> 인용문입니다.";
        let html = markdown_to_html(md);
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("인용문입니다."));
    }

    #[test]
    fn export_markdown_to_html_list() {
        let md = "- 항목1\n- 항목2";
        let html = markdown_to_html(md);
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>항목1</li>"));
        assert!(html.contains("<li>항목2</li>"));
    }

    #[test]
    fn export_html_escapes_special_chars() {
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert('xss')&lt;/script&gt;"
        );
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn export_inline_md_to_html_bold() {
        let result = inline_md_to_html("이것은 **굵은** 텍스트");
        assert!(result.contains("<strong>굵은</strong>"));
    }

    #[test]
    fn export_strip_list_marker_dash() {
        assert_eq!(strip_list_marker("- 항목"), "항목");
        assert_eq!(strip_list_marker("* 항목"), "항목");
        assert_eq!(strip_list_marker("1. 항목"), "항목");
        assert_eq!(strip_list_marker("일반 텍스트"), "일반 텍스트");
    }

    #[test]
    fn export_build_meta_includes_info() {
        let meta = build_export_meta("test-article.md", 500);
        assert!(meta.contains("제목: test-article"));
        assert!(meta.contains("글자수: 500자"));
        assert!(meta.contains("날짜:"));
    }

    #[test]
    fn export_file_creates_text_output() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("article.md");
        std::fs::write(&src, "# 테스트 기사\n\n본문 **내용**입니다.").unwrap();

        // Set working dir context for EXPORTS_DIR
        let exports = tmp.path().join(".journalist").join("exports");
        std::fs::create_dir_all(&exports).unwrap();

        // Directly test the conversion functions
        let content = std::fs::read_to_string(&src).unwrap();
        let plain = markdown_to_plain_text(&content);
        assert!(plain.contains("테스트 기사"));
        assert!(plain.contains("본문"));
        assert!(!plain.contains("**"));
        assert!(!plain.contains('#'));
    }

    #[test]
    fn export_file_creates_html_output() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("article.md");
        std::fs::write(&src, "# 테스트\n\n> 인용\n\n- 목록").unwrap();

        let content = std::fs::read_to_string(&src).unwrap();
        let html = markdown_to_html(&content);
        assert!(html.contains("<h1>테스트</h1>"));
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("<li>목록</li>"));
    }

    #[test]
    fn export_regex_replace_pairs_balanced() {
        let result = regex_replace_pairs("a **b** c", "**", "<strong>", "</strong>");
        assert_eq!(result, "a <strong>b</strong> c");
    }

    #[test]
    fn export_regex_replace_pairs_unbalanced() {
        // Unbalanced delimiters should return original
        let result = regex_replace_pairs("a **b c", "**", "<strong>", "</strong>");
        assert_eq!(result, "a **b c");
    }

    #[test]
    fn parse_proofread_args_inline_text() {
        let (file, text) = parse_proofread_args("삼성전자가 실적을 발표했다");
        assert!(file.is_none());
        assert_eq!(text, "삼성전자가 실적을 발표했다");
    }

    #[test]
    fn parse_proofread_args_with_file() {
        let (file, text) = parse_proofread_args("--file article.txt");
        assert_eq!(file.as_deref(), Some("article.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_proofread_args_file_and_text() {
        let (file, text) = parse_proofread_args("--file draft.md 추가 맥락");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert_eq!(text, "추가 맥락");
    }

    #[test]
    fn parse_proofread_args_empty() {
        let (file, text) = parse_proofread_args("");
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_proofread_prompt_basic() {
        let prompt = build_proofread_prompt("삼성전자가 1분기 실적을 발표했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("삼성전자가 1분기 실적을 발표했다."));
        assert!(p.contains("교열"));
        assert!(p.contains("맞춤법"));
        assert!(p.contains("경어체"));
    }

    #[test]
    fn build_proofread_prompt_empty_returns_none() {
        assert!(build_proofread_prompt("").is_none());
        assert!(build_proofread_prompt("   ").is_none());
    }

    #[test]
    fn proofread_file_path_with_topic() {
        let path = proofread_file_path_with_date("반도체 수출", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/proofread/2026-03-20_반도체-수출.md"
        );
    }

    #[test]
    fn proofread_file_path_empty_slug() {
        let path = proofread_file_path_with_date("", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/proofread/2026-03-20_proofread.md"
        );
    }

    #[test]
    fn save_proofread_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("proofread").join("test.md");
        let result = save_proofread(&path, "# 교열 결과\n\n교정된 기사 본문");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("교열 결과"));
    }

    #[test]
    fn quote_load_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("quotes.json");
        let quotes = load_quotes_from(&path);
        assert!(quotes.is_empty());
    }

    #[test]
    fn quote_save_and_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("quotes.json");
        let quotes = vec![serde_json::json!({
            "source": "홍길동",
            "text": "반도체 수출이 증가했습니다",
            "timestamp": "2026-03-20 09:30",
        })];
        save_quotes_to(&quotes, &path);
        assert!(path.exists());
        let loaded = load_quotes_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["source"], "홍길동");
        assert_eq!(loaded[0]["text"], "반도체 수출이 증가했습니다");
        assert_eq!(loaded[0]["timestamp"], "2026-03-20 09:30");
    }

    #[test]
    fn quote_save_multiple_and_remove() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("quotes.json");
        let mut quotes = vec![
            serde_json::json!({"source": "김기자", "text": "첫 번째 발언", "timestamp": "2026-03-20 10:00"}),
            serde_json::json!({"source": "이기자", "text": "두 번째 발언", "timestamp": "2026-03-20 11:00"}),
            serde_json::json!({"source": "김기자", "text": "세 번째 발언", "timestamp": "2026-03-20 12:00"}),
        ];
        save_quotes_to(&quotes, &path);
        assert_eq!(load_quotes_from(&path).len(), 3);

        // Remove second entry (index 1)
        quotes.remove(1);
        save_quotes_to(&quotes, &path);
        let loaded = load_quotes_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["source"], "김기자");
        assert_eq!(loaded[1]["text"], "세 번째 발언");
    }

    #[test]
    fn quote_source_org_lookup() {
        // source_org_for reads from the global SOURCES_FILE, so when no sources exist
        // it should return None.
        let result = source_org_for("존재하지않는취재원");
        assert!(result.is_none());
    }

    #[test]
    fn quote_save_creates_parent_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sub").join("dir").join("quotes.json");
        let quotes = vec![serde_json::json!({"source": "테스트", "text": "발언", "timestamp": "2026-01-01 00:00"})];
        save_quotes_to(&quotes, &path);
        assert!(path.exists());
    }

    #[test]
    fn legal_prompt_with_text() {
        let prompt = build_legal_prompt("김 의원이 뇌물을 받았다는 소문이 있다.");
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("명예훼손"));
        assert!(prompt.contains("초상권"));
        assert!(prompt.contains("반론권"));
        assert!(prompt.contains("공인/사인"));
        assert!(prompt.contains("김 의원이 뇌물을 받았다는 소문이 있다."));
    }

    #[test]
    fn legal_prompt_empty_returns_none() {
        assert!(build_legal_prompt("").is_none());
        assert!(build_legal_prompt("   ").is_none());
    }

    #[test]
    fn parse_legal_args_inline() {
        let (file, text) = parse_legal_args("기사 텍스트 내용");
        assert!(file.is_none());
        assert_eq!(text, "기사 텍스트 내용");
    }

    #[test]
    fn parse_legal_args_file_flag() {
        let (file, text) = parse_legal_args("--file draft.md");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_legal_args_file_with_extra() {
        let (file, text) = parse_legal_args("--file draft.md 추가 메모");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert_eq!(text, "추가 메모");
    }

    #[test]
    fn legal_file_path_with_slug() {
        let path = legal_file_path_with_date("김 의원 뇌물 의혹", "2026-03-20");
        let path_str = path.to_string_lossy();
        assert!(path_str.starts_with(".journalist/legal/"));
        assert!(path_str.contains("2026-03-20"));
        assert!(path_str.contains("김-의원-뇌물-의혹"));
        assert!(path_str.ends_with(".md"));
    }

    #[test]
    fn legal_file_path_empty_slug() {
        let path = legal_file_path_with_date("", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/legal/2026-03-20_legal.md"
        );
    }

    #[test]
    fn save_legal_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("legal").join("test.md");
        save_legal(&path, "# 법적 점검 결과\n내용").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# 법적 점검 결과\n내용");
    }

    fn add_archive_entry(
        index_path: &Path,
        archive_dir: &Path,
        id: usize,
        title: &str,
        date: &str,
        section: &str,
        article_type: &str,
        tags: Vec<&str>,
        body: &str,
    ) {
        let mut index = load_archive_index_from(index_path);
        let text_filename = format!("{id:04}.txt");
        let text_path = archive_dir.join(&text_filename);
        std::fs::write(&text_path, body).unwrap();
        let entry = serde_json::json!({
            "id": id,
            "title": title,
            "date": date,
            "section": section,
            "type": article_type,
            "tags": tags,
            "file": text_filename,
        });
        index.push(entry);
        save_archive_index_to(&index, index_path);
    }

    #[test]
    fn archive_save_and_load_index() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(
            &index_path,
            &archive_dir,
            1,
            "반도체 수출 급증",
            "2026-03-20",
            "경제",
            "스트레이트",
            vec!["반도체", "삼성"],
            "반도체 수출이 전년 대비 30% 증가했다.",
        );

        let index = load_archive_index_from(&index_path);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0]["title"], "반도체 수출 급증");
        assert_eq!(index[0]["section"], "경제");
        assert_eq!(index[0]["tags"][0], "반도체");
        assert_eq!(index[0]["tags"][1], "삼성");
    }

    #[test]
    fn archive_search_finds_by_title() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "반도체 수출 급증", "2026-03-20", "경제", "", vec![], "본문");
        add_archive_entry(&index_path, &archive_dir, 2, "자동차 산업 동향", "2026-03-19", "산업", "", vec![], "본문");

        let index = load_archive_index_from(&index_path);
        let keyword = "반도체";
        let keyword_lower = keyword.to_lowercase();
        let results: Vec<_> = index
            .iter()
            .filter(|e| {
                e["title"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&keyword_lower)
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "반도체 수출 급증");
    }

    #[test]
    fn archive_search_finds_by_tag() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "", "", vec!["삼성", "반도체"], "");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "", "", vec!["현대", "자동차"], "");

        let index = load_archive_index_from(&index_path);
        let keyword_lower = "삼성";
        let results: Vec<_> = index
            .iter()
            .filter(|e| {
                if let Some(tags) = e["tags"].as_array() {
                    tags.iter().any(|t| t.as_str().unwrap_or("").contains(keyword_lower))
                } else {
                    false
                }
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "기사A");
    }

    #[test]
    fn archive_search_finds_by_body() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "", "", vec![], "반도체 수출이 급증했다");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "", "", vec![], "자동차 수출은 감소");

        let index = load_archive_index_from(&index_path);
        let keyword_lower = "반도체";
        let results: Vec<_> = index
            .iter()
            .filter(|e| {
                let filename = e["file"].as_str().unwrap_or("");
                if !filename.is_empty() {
                    let text_path = archive_dir.join(filename);
                    if let Ok(body) = std::fs::read_to_string(&text_path) {
                        return body.contains(keyword_lower);
                    }
                }
                false
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "기사A");
    }

    #[test]
    fn archive_view_finds_by_id() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "경제", "스트레이트", vec!["반도체"], "본문 내용");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "정치", "해설", vec!["국회"], "정치 본문");

        let index = load_archive_index_from(&index_path);
        let entry = index.iter().find(|e| e["id"].as_u64() == Some(2));
        assert!(entry.is_some());
        assert_eq!(entry.unwrap()["title"], "기사B");

        // Read body
        let filename = entry.unwrap()["file"].as_str().unwrap();
        let body = std::fs::read_to_string(archive_dir.join(filename)).unwrap();
        assert_eq!(body, "정치 본문");
    }

    #[test]
    fn archive_list_section_filter() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "경제", "", vec![], "");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "정치", "", vec![], "");
        add_archive_entry(&index_path, &archive_dir, 3, "기사C", "2026-03-18", "경제", "", vec![], "");

        let index = load_archive_index_from(&index_path);
        let filtered: Vec<_> = index
            .iter()
            .filter(|e| e["section"].as_str().unwrap_or("") == "경제")
            .collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn archive_parse_args_full() {
        let (title, section, article_type, tags, file_path) =
            parse_archive_save_args("반도체 수출 기사 --section 경제 --type 스트레이트 --tags 반도체,삼성 --file /tmp/article.txt");
        assert_eq!(title, "반도체 수출 기사");
        assert_eq!(section, "경제");
        assert_eq!(article_type, "스트레이트");
        assert_eq!(tags, vec!["반도체", "삼성"]);
        assert_eq!(file_path, Some("/tmp/article.txt".to_string()));
    }

    #[test]
    fn archive_parse_args_title_only() {
        let (title, section, article_type, tags, file_path) =
            parse_archive_save_args("단순 제목");
        assert_eq!(title, "단순 제목");
        assert!(section.is_empty());
        assert!(article_type.is_empty());
        assert!(tags.is_empty());
        assert!(file_path.is_none());
    }

    #[test]
    fn archive_empty_index_loads_empty() {
        let (_dir, index_path, _archive_dir) = temp_archive_paths();
        let index = load_archive_index_from(&index_path);
        assert!(index.is_empty());
    }

    #[test]
    fn publish_report_all_pass() {
        let results = vec![
            ("checklist", PublishStepResult::Pass("체크리스트 완료".into())),
            ("proofread", PublishStepResult::Pass("교열 완료".into())),
            ("legal", PublishStepResult::Pass("법적 점검 통과".into())),
            ("export", PublishStepResult::Pass("내보내기 완료".into())),
        ];
        // Should not panic
        print_publish_report(&results);
    }

    #[test]
    fn publish_report_blocked_by_legal() {
        let results = vec![
            ("checklist", PublishStepResult::Pass("체크리스트 완료".into())),
            ("proofread", PublishStepResult::Pass("교열 완료".into())),
            (
                "legal",
                PublishStepResult::Blocked(
                    "🚨 법적 리스크 '위험' 판정 — 파이프라인 중단".into(),
                ),
            ),
            (
                "export",
                PublishStepResult::Fail("이전 단계 중단으로 건너뜀".into()),
            ),
        ];
        print_publish_report(&results);
    }

    #[test]
    fn publish_step_result_variants() {
        let pass = PublishStepResult::Pass("ok".into());
        let fail = PublishStepResult::Fail("err".into());
        let blocked = PublishStepResult::Blocked("halt".into());

        assert_eq!(pass, PublishStepResult::Pass("ok".into()));
        assert_ne!(pass, fail);
        assert!(matches!(blocked, PublishStepResult::Blocked(_)));
    }

    #[test]
    fn build_anonymize_prompt_basic() {
        let prompt = build_anonymize_prompt("홍길동 기자가 취재한 내용입니다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("개인식별정보"));
        assert!(p.contains("홍길동 기자가 취재한 내용입니다."));
        assert!(p.contains("익명화 매핑표"));
    }

    #[test]
    fn build_anonymize_prompt_empty_returns_none() {
        assert!(build_anonymize_prompt("").is_none());
        assert!(build_anonymize_prompt("   ").is_none());
    }

    #[test]
    fn anonymize_file_path_with_topic() {
        let path = anonymize_file_path_with_date("탐사보도초안", "2026-03-21");
        let s = path.to_string_lossy();
        assert!(s.contains("anonymize"));
        assert!(s.contains("2026-03-21"));
        assert!(s.contains("탐사보도초안"));
    }

    #[test]
    fn anonymize_file_path_empty_slug() {
        let path = anonymize_file_path_with_date("", "2026-03-21");
        let s = path.to_string_lossy();
        assert!(s.contains("2026-03-21_anonymize.md"));
    }

    #[test]
    fn save_anonymize_creates_dir_and_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sub").join("anon.md");
        save_anonymize(&path, "익명화 결과").unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "익명화 결과");
    }

    #[test]
    fn readability_empty_text() {
        let m = compute_readability("");
        assert_eq!(m.sentence_count, 0);
        assert_eq!(m.paragraph_count, 0);
        assert_eq!(m.avg_sentence_len, 0.0);
        assert_eq!(m.grade, 'A'); // empty text = perfect score
    }

    #[test]
    fn readability_simple_short_sentences() {
        let text = "경제가 성장했다. 물가가 안정됐다. 고용이 늘었다.";
        let m = compute_readability(text);
        assert_eq!(m.sentence_count, 3);
        assert!(m.avg_sentence_len < 30.0);
        assert_eq!(m.long_sentence_ratio, 0.0);
        assert!(m.grade == 'A' || m.grade == 'B');
    }

    #[test]
    fn readability_detects_passive() {
        let text = "법안이 통과되었다. 예산이 삭감되었다. 결과가 발표되었다.";
        let m = compute_readability(text);
        assert!(m.passive_ratio > 0.5); // majority are passive
    }

    #[test]
    fn readability_detects_jargon() {
        let text = "거버넌스 체계와 컨센서스 형성이 중요하다. 패러다임 전환이 필요하다. 이니셔티브를 추진해야 한다.";
        let m = compute_readability(text);
        assert!(m.jargon_density > 0.05);
    }

    #[test]
    fn readability_long_sentence_detection() {
        // Create a sentence with >80 non-space characters
        let long = "가".repeat(90);
        let text = format!("{long}. 짧다.");
        let m = compute_readability(&text);
        assert_eq!(m.sentence_count, 2);
        assert!(m.long_sentence_ratio > 0.4); // 1 out of 2 is long
    }

    #[test]
    fn readability_paragraph_count() {
        let text = "첫 번째 문단이다.\n\n두 번째 문단이다.\n\n세 번째 문단이다.";
        let m = compute_readability(text);
        assert_eq!(m.paragraph_count, 3);
    }

    #[test]
    fn readability_grade_mapping() {
        // Very short, simple text → high score → good grade
        let text = "안녕하다.";
        let m = compute_readability(text);
        assert!(m.grade == 'A' || m.grade == 'B');
    }

    #[test]
    fn split_korean_sentences_basic() {
        let sentences = split_korean_sentences("첫 문장이다. 두 번째다! 세 번째다?");
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn split_paragraphs_basic() {
        let paras = split_paragraphs("문단1\n줄2\n\n문단2\n\n문단3");
        assert_eq!(paras.len(), 3);
    }

    #[test]
    fn parse_improve_args_inline_text() {
        let (file, text) = parse_improve_args("이것은 기사 텍스트입니다");
        assert!(file.is_none());
        assert_eq!(text, "이것은 기사 텍스트입니다");
    }

    #[test]
    fn parse_improve_args_with_file() {
        let (file, text) = parse_improve_args("--file article.md");
        assert_eq!(file.unwrap(), "article.md");
        assert!(text.is_empty());
    }

    #[test]
    fn parse_improve_args_file_and_text() {
        let (file, text) = parse_improve_args("--file draft.md 추가 지시사항");
        assert_eq!(file.unwrap(), "draft.md");
        assert_eq!(text, "추가 지시사항");
    }

    #[test]
    fn parse_improve_args_empty() {
        let (file, text) = parse_improve_args("");
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_improve_prompt_basic() {
        let prompt = build_improve_prompt("반도체 수출이 증가했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("리드문 흡인력"));
        assert!(p.contains("문단 전환"));
        assert!(p.contains("인용문 활용"));
        assert!(p.contains("구체성·수치"));
        assert!(p.contains("결론 완성도"));
        assert!(p.contains("반도체 수출이 증가했다."));
    }

    #[test]
    fn build_improve_prompt_empty_returns_none() {
        assert!(build_improve_prompt("").is_none());
        assert!(build_improve_prompt("   ").is_none());
    }

    #[test]
    fn improve_file_path_with_topic() {
        let path = improve_file_path_with_date("반도체 수출", "2026-03-21");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/improve/2026-03-21_반도체-수출.md"
        );
    }

    #[test]
    fn improve_file_path_empty_slug() {
        let path = improve_file_path_with_date("", "2026-03-21");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/improve/2026-03-21_improve.md"
        );
    }

    #[test]
    fn save_improve_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("improve").join("test.md");
        let result = save_improve(&path, "# 개선 제안\n\n기사 피드백");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("개선 제안"));
    }

    // ── /multiformat tests ──────────────────────────────────────────────

    #[test]
    fn parse_multiformat_args_format_and_file() {
        let (fmt, file, text) = parse_multiformat_args("article.md --format broadcast");
        assert_eq!(fmt.as_deref(), Some("broadcast"));
        assert_eq!(file.as_deref(), Some("article.md"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_multiformat_args_format_first() {
        let (fmt, file, text) = parse_multiformat_args("--format card article.md");
        assert_eq!(fmt.as_deref(), Some("card"));
        assert_eq!(file.as_deref(), Some("article.md"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_multiformat_args_empty() {
        let (fmt, file, text) = parse_multiformat_args("");
        assert!(fmt.is_none());
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn parse_multiformat_args_format_only() {
        let (fmt, file, text) = parse_multiformat_args("--format online");
        assert_eq!(fmt.as_deref(), Some("online"));
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_multiformat_prompt_broadcast() {
        let prompt = build_multiformat_prompt("반도체 수출 기사 내용", "broadcast").unwrap();
        assert!(prompt.contains("방송 원고"));
        assert!(prompt.contains("앵커 멘트"));
        assert!(prompt.contains("리포트"));
        assert!(prompt.contains("반도체 수출 기사 내용"));
    }

    #[test]
    fn build_multiformat_prompt_online() {
        let prompt = build_multiformat_prompt("기사 내용", "online").unwrap();
        assert!(prompt.contains("온라인 기사"));
        assert!(prompt.contains("소제목"));
        assert!(prompt.contains("역피라미드"));
    }

    #[test]
    fn build_multiformat_prompt_card() {
        let prompt = build_multiformat_prompt("기사 내용", "card").unwrap();
        assert!(prompt.contains("카드뉴스"));
        assert!(prompt.contains("5장 이내"));
        assert!(prompt.contains("[카드 N]"));
    }

    #[test]
    fn build_multiformat_prompt_brief() {
        let prompt = build_multiformat_prompt("기사 내용", "brief").unwrap();
        assert!(prompt.contains("3줄 요약"));
        assert!(prompt.contains("What"));
        assert!(prompt.contains("Why it matters"));
    }

    #[test]
    fn build_multiformat_prompt_empty_returns_none() {
        assert!(build_multiformat_prompt("", "broadcast").is_none());
        assert!(build_multiformat_prompt("   ", "card").is_none());
    }

    #[test]
    fn build_multiformat_prompt_unknown_format_returns_none() {
        assert!(build_multiformat_prompt("기사", "unknown").is_none());
    }

    #[test]
    fn multiformat_file_path_with_slug_and_format() {
        let path = multiformat_file_path_with_date("반도체 수출", "broadcast", "2026-03-22");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/multiformat/2026-03-22_반도체-수출_broadcast.md"
        );
    }

    #[test]
    fn multiformat_file_path_empty_slug() {
        let path = multiformat_file_path_with_date("", "card", "2026-03-22");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/multiformat/2026-03-22_card.md"
        );
    }

    #[test]
    fn save_multiformat_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("multiformat").join("test.md");
        let result = save_multiformat(&path, "# 방송 원고\n\n앵커 멘트");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("방송 원고"));
    }

    // ── /correction tests ──────────────────────────────────────────────

    #[test]
    fn parse_correction_add_args_all_flags() {
        let (article, error, fix) =
            parse_correction_add_args("--article 제목 --error 오류 --fix 정정");
        assert_eq!(article, "제목");
        assert_eq!(error, "오류");
        assert_eq!(fix, "정정");
    }

    #[test]
    fn parse_correction_add_args_missing_flags() {
        let (article, error, fix) = parse_correction_add_args("--article 제목");
        assert_eq!(article, "제목");
        assert!(error.is_empty());
        assert!(fix.is_empty());
    }

    #[test]
    fn parse_correction_add_args_empty() {
        let (article, error, fix) = parse_correction_add_args("");
        assert!(article.is_empty());
        assert!(error.is_empty());
        assert!(fix.is_empty());
    }

    #[test]
    fn append_and_load_corrections() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("corrections.jsonl");

        let record = CorrectionRecord {
            date: "2026-03-22".to_string(),
            article: "테스트 기사".to_string(),
            error: "잘못된 수치".to_string(),
            fix: "올바른 수치".to_string(),
            status: "pending".to_string(),
        };

        // Write directly to temp file
        let json = serde_json::to_string(&record).unwrap();
        std::fs::write(&file_path, format!("{json}\n")).unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        let records: Vec<CorrectionRecord> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].article, "테스트 기사");
        assert_eq!(records[0].error, "잘못된 수치");
        assert_eq!(records[0].fix, "올바른 수치");
        assert_eq!(records[0].status, "pending");
    }

    #[test]
    fn correction_report_path_with_slug() {
        let path = correction_report_path_with_date("테스트-기사", "2026-03-22");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/corrections/2026-03-22_테스트-기사.md"
        );
    }

    #[test]
    fn correction_report_path_empty_slug() {
        let path = correction_report_path_with_date("", "2026-03-22");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/corrections/2026-03-22_correction.md"
        );
    }

    #[test]
    fn build_correction_report_prompt_includes_law() {
        let records = vec![CorrectionRecord {
            date: "2026-03-22".to_string(),
            article: "반도체 수출".to_string(),
            error: "수치 오류".to_string(),
            fix: "정정된 수치".to_string(),
            status: "pending".to_string(),
        }];
        let prompt = build_correction_report_prompt("반도체 수출", &records);
        assert!(prompt.contains("언론중재법"));
        assert!(prompt.contains("원보도와 같은 크기"));
        assert!(prompt.contains("정정보도문"));
        assert!(prompt.contains("반도체 수출"));
        assert!(prompt.contains("수치 오류"));
    }

    #[test]
    fn save_correction_report_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("corrections").join("test.md");
        let result = save_correction_report(&path, "# 정정보도문\n\n내용");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("정정보도문"));
    }
}
