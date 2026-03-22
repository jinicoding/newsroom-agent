//! Workflow & management command handlers (워크플로우·관리 도메인)
//! Commands: /autopitch, /breaking, /briefing, /calendar, /collaborate, /compare, /coverage, /dashboard, /data, /deadline, /desk, /diary, /embargo, /interview, /morning, /performance, /recap, /rival, /timeline

use crate::commands::auto_compact_if_needed;
use crate::commands_project::*;
use crate::commands_research::{
    days_until, ensure_sources_dir_at, load_all_contact_logs, load_followups_from,
    load_notes_from, load_sources_from, notes_file_for_date, followups_path,
    ContactLog, Followup, SOURCES_FILE,
};
use crate::commands_writing::format_unix_timestamp;
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

// ── /briefing ────────────────────────────────────────────────────────────

/// Parse `/briefing` input to extract `--file <path>` and inline text.
/// Returns `(Option<file_path>, remaining_text)`.
pub fn parse_briefing_args(args: &str) -> (Option<String>, String) {
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

/// Build the prompt for the `/briefing` command (press release to article draft).
pub fn build_briefing_prompt(press_release: &str) -> Option<String> {
    if press_release.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 보도자료를 기사 초안으로 변환해주세요.\n\n\
         다음 단계를 따라주세요:\n\
         1. 보도자료에서 핵심 사실(누가, 무엇을, 언제, 어디서, 왜, 어떻게)을 추출하세요\n\
         2. 역피라미드 구조로 기사 초안을 작성하세요:\n\
         - **리드**: 가장 중요한 사실을 첫 문단에\n\
         - **본문**: 세부 사항을 중요도 순으로\n\
         - **배경**: 맥락과 부가 정보\n\
         3. 보도자료에서 직접 확인할 수 없는 사실에는 [확인 필요]를 표시하세요\n\
         4. 보도자료 원문의 홍보성 표현은 중립적으로 바꾸세요\n\n\
         ## 보도자료 원문\n\n\
         {press_release}"
    ))
}

/// Build the draft file path for briefing output.
pub fn briefing_draft_path(slug_source: &str) -> std::path::PathBuf {
    briefing_draft_path_with_date(slug_source, &today_str())
}

/// Build the draft file path with an explicit date string (for testing).
pub fn briefing_draft_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_briefing.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(DRAFTS_DIR).join(filename)
}

/// Handle the `/briefing` command: convert press release to article draft.
pub async fn handle_briefing(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/briefing").unwrap_or("").trim();
    let (file_path, inline_text) = parse_briefing_args(args);

    // Read press release content from file or inline
    let press_release = if let Some(ref path) = file_path {
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

    let prompt = match build_briefing_prompt(&press_release) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /briefing <보도자료 텍스트>{RESET}");
            println!("{DIM}  또는:   /briefing --file <경로>{RESET}");
            println!("{DIM}  예시:   /briefing --file press_release.txt{RESET}");
            println!("{DIM}  보도자료를 역피라미드 구조 기사 초안으로 변환합니다.{RESET}\n");
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save draft to .journalist/drafts/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "briefing".to_string())
        } else {
            let preview: String = press_release.chars().take(30).collect();
            if preview.is_empty() {
                "briefing".to_string()
            } else {
                preview
            }
        };
        let path = briefing_draft_path(&slug_source);
        match save_article_draft(&path, &response) {
            Ok(_) => {
                println!("{GREEN}  ✓ 초안 저장: {}{RESET}\n", path.display());
            }
            Err(e) => {
                eprintln!("{RED}  초안 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /interview ──────────────────────────────────────────────────────────

/// Directory for saved interview prep files.
const INTERVIEW_DIR: &str = ".journalist/interview";

/// Build the interview file path: `.journalist/interview/YYYY-MM-DD_<slug>.md`
pub fn interview_file_path(topic: &str) -> std::path::PathBuf {
    interview_file_path_with_date(topic, &today_str())
}

/// Build the interview file path with an explicit date string (for testing).
pub fn interview_file_path_with_date(topic: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(topic, 50);
    let filename = if slug.is_empty() {
        format!("{date}_interview.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(INTERVIEW_DIR).join(filename)
}

/// Save interview prep to file. Creates the interview directory if needed.
fn save_interview(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Parse `/interview` arguments: extract topic and optional `--source` name.
pub fn parse_interview_args(args: &str) -> (String, Option<String>) {
    let args = args.trim();
    if args.is_empty() {
        return (String::new(), None);
    }

    if let Some(idx) = args.find("--source") {
        let topic = args[..idx].trim().to_string();
        let source_name = args[idx + 8..].trim().to_string();
        let source_name = if source_name.is_empty() {
            None
        } else {
            Some(source_name)
        };
        (topic, source_name)
    } else {
        (args.to_string(), None)
    }
}

/// Look up a source by name from sources.json. Returns matching entry if found.
fn find_source_by_name(name: &str) -> Option<serde_json::Value> {
    find_source_by_name_in(name, std::path::Path::new(SOURCES_FILE))
}

/// Look up a source by name from a specific sources file (for testing).
pub fn find_source_by_name_in(name: &str, path: &std::path::Path) -> Option<serde_json::Value> {
    let sources = load_sources_from(path);
    let name_lower = name.to_lowercase();
    sources.into_iter().find(|s| {
        s["name"]
            .as_str()
            .map_or(false, |n| n.to_lowercase().contains(&name_lower))
    })
}

/// Build the interview prompt for the AI agent.
pub fn build_interview_prompt(
    topic: &str,
    source_info: Option<&serde_json::Value>,
    research_context: &[(String, String)],
) -> Option<String> {
    if topic.is_empty() {
        return None;
    }

    let mut prompt = format!(
        "당신은 숙련된 기자의 인터뷰 준비를 돕는 전문 어시스턴트입니다.\n\n\
         **주제**: {topic}\n\n"
    );

    if let Some(source) = source_info {
        let name = source["name"].as_str().unwrap_or("(이름 없음)");
        let org = source["org"].as_str().unwrap_or("");
        let beat = source["beat"].as_str().unwrap_or("");
        let note = source["note"].as_str().unwrap_or("");
        prompt.push_str(&format!("**취재원 정보**:\n"));
        prompt.push_str(&format!("- 이름: {name}\n"));
        if !org.is_empty() {
            prompt.push_str(&format!("- 소속: {org}\n"));
        }
        if !beat.is_empty() {
            prompt.push_str(&format!("- 분야: {beat}\n"));
        }
        if !note.is_empty() {
            prompt.push_str(&format!("- 메모: {note}\n"));
        }
        prompt.push('\n');
    }

    if !research_context.is_empty() {
        prompt.push_str("**관련 리서치 자료**:\n");
        for (filename, content) in research_context {
            let preview: String = content.chars().take(500).collect();
            prompt.push_str(&format!("--- {filename} ---\n{preview}\n\n"));
        }
    }

    prompt.push_str(
        "다음 구조로 인터뷰 질문지를 작성해 주세요:\n\n\
         1. **도입 질문** (2-3개): 인터뷰 분위기를 만들고 취재원의 전문성/입장을 파악하는 질문\n\
         2. **핵심 질문** (5-7개): 주제의 본질을 파고드는 구체적이고 날카로운 질문\n\
         3. **팔로업 질문** (3-4개): 예상 답변에 따른 후속 질문\n\
         4. **마무리 질문** (1-2개): 핵심 메시지 확인, 추가 취재 단서 확보\n\n\
         각 질문에 대해:\n\
         - 질문의 의도/목적을 괄호 안에 간략히 표기\n\
         - 예상되는 회피성 답변에 대한 재질문도 준비\n\
         - 숫자, 날짜 등 구체적 사실을 확인하는 질문 포함\n"
    );

    Some(prompt)
}

/// Handle the `/interview` command.
pub async fn handle_interview(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/interview").unwrap_or("").trim();
    let (topic, source_name) = parse_interview_args(args);

    if topic.is_empty() {
        println!("{DIM}  사용법: /interview <주제> [--source 취재원]{RESET}");
        println!("{DIM}  예시:   /interview 반도체 수출 규제 --source 김철수{RESET}");
        println!("{DIM}  인터뷰 주제에 맞는 구조화된 질문지를 생성합니다.{RESET}\n");
        return;
    }

    // Look up source if specified
    let source_info = if let Some(ref name) = source_name {
        let found = find_source_by_name(name);
        if let Some(ref info) = found {
            let display_name = info["name"].as_str().unwrap_or(name);
            println!("{GREEN}  📋 취재원 정보 로드: {display_name}{RESET}");
        } else {
            println!(
                "{YELLOW}  ⚠ 취재원 '{name}'을(를) sources.json에서 찾을 수 없습니다.{RESET}"
            );
        }
        found
    } else {
        None
    };

    // Search for related research files
    let research = find_related_research(&topic);
    if !research.is_empty() {
        println!(
            "{GREEN}  📎 관련 리서치 {}건 발견{RESET}",
            research.len()
        );
        for (filename, _) in &research {
            println!("     - {filename}");
        }
    }
    println!();

    let prompt = match build_interview_prompt(&topic, source_info.as_ref(), &research) {
        Some(p) => p,
        None => return,
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save interview prep to .journalist/interview/
    if !response.trim().is_empty() {
        let path = interview_file_path(&topic);
        match save_interview(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 인터뷰 질문지 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  인터뷰 질문지 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /compare ────────────────────────────────────────────────────────────

/// Parse `/compare` arguments: expects two file paths.
/// Returns `(Option<path1>, Option<path2>)`.
pub fn parse_compare_args(args: &str) -> (Option<String>, Option<String>) {
    let args = args.trim();
    let parts: Vec<&str> = args.split_whitespace().collect();
    match parts.len() {
        0 => (None, None),
        1 => (Some(parts[0].to_string()), None),
        _ => (Some(parts[0].to_string()), Some(parts[1].to_string())),
    }
}

/// Build the prompt for `/compare`: journalism-focused comparison of two article drafts.
pub fn build_compare_prompt(content1: &str, path1: &str, content2: &str, path2: &str) -> String {
    format!(
        "아래 두 기사 초안을 **저널리즘 관점**에서 비교 분석해주세요.\n\n\
         단순한 텍스트 diff가 아니라, 다음 항목을 중심으로 분석해주세요:\n\n\
         ## 비교 항목\n\n\
         ### 1. 사실(팩트) 변경\n\
         - 추가된 사실, 삭제된 사실, 수정된 사실을 각각 정리\n\
         - 사실관계 변경이 기사의 방향성에 미치는 영향 분석\n\n\
         ### 2. 톤/논조 변화\n\
         - 전체적인 톤이 어떻게 바뀌었는지 (객관적↔주관적, 긍정적↔부정적 등)\n\
         - 헤드라인이나 리드의 뉘앙스 변화\n\n\
         ### 3. 출처/인용 변경\n\
         - 추가/삭제/수정된 인용구나 취재원\n\
         - 출처 변경이 기사 신뢰도에 미치는 영향\n\n\
         ### 4. 구조 변경\n\
         - 단락 순서 변경, 내용 재배치\n\
         - 리드/본문/맺음 구조의 변화\n\n\
         ### 5. 법적/윤리적 리스크 변화\n\
         - 명예훼손, 개인정보 노출 등 리스크가 추가/해소되었는지\n\n\
         ## 종합 평가\n\n\
         수정이 기사 품질을 향상시켰는지, 주의가 필요한 부분은 무엇인지 정리해주세요.\n\n\
         ---\n\n\
         ## 초안 1: {path1}\n\n\
         {content1}\n\n\
         ---\n\n\
         ## 초안 2: {path2}\n\n\
         {content2}"
    )
}

/// Handle the `/compare` command: compare two article drafts from a journalism perspective.
pub async fn handle_compare(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/compare").unwrap_or("").trim();
    let (path1, path2) = parse_compare_args(args);

    let (p1, p2) = match (path1, path2) {
        (Some(a), Some(b)) => (a, b),
        _ => {
            println!("{DIM}  사용법: /compare <파일1> <파일2>{RESET}");
            println!("{DIM}  예시:   /compare draft_v1.md draft_v2.md{RESET}");
            println!(
                "{DIM}  두 기사 초안을 저널리즘 관점에서 비교 분석합니다.{RESET}"
            );
            println!(
                "{DIM}  (사실 추가/삭제, 톤 변화, 출처 변경, 구조, 법적 리스크){RESET}\n"
            );
            return;
        }
    };

    let content1 = match std::fs::read_to_string(&p1) {
        Ok(c) => {
            println!("{DIM}  파일 1 읽기: {p1} ({} bytes){RESET}", c.len());
            c
        }
        Err(e) => {
            eprintln!("{RED}  파일 읽기 실패: {p1} — {e}{RESET}\n");
            return;
        }
    };

    let content2 = match std::fs::read_to_string(&p2) {
        Ok(c) => {
            println!("{DIM}  파일 2 읽기: {p2} ({} bytes){RESET}", c.len());
            c
        }
        Err(e) => {
            eprintln!("{RED}  파일 읽기 실패: {p2} — {e}{RESET}\n");
            return;
        }
    };

    println!();

    let prompt = build_compare_prompt(&content1, &p1, &content2, &p2);
    run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);
}

// ── /timeline ────────────────────────────────────────────────────────────

const TIMELINE_DIR: &str = ".journalist/timeline";

/// Build the timeline file path using today's date.
pub fn timeline_file_path(topic: &str) -> std::path::PathBuf {
    timeline_file_path_with_date(topic, &today_str())
}

/// Build the timeline file path with an explicit date string (for testing).
pub fn timeline_file_path_with_date(topic: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(topic, 50);
    let filename = if slug.is_empty() {
        format!("{date}_timeline.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(TIMELINE_DIR).join(filename)
}

/// Save timeline to file. Creates the timeline directory if needed.
fn save_timeline(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Build the prompt for `/timeline`: generate a chronological event timeline.
pub fn build_timeline_prompt(topic: &str, research: &[(String, String)]) -> String {
    let mut prompt = format!(
        "주제 **\"{topic}\"**에 대한 **시간순 이벤트 타임라인**을 작성해주세요.\n\n\
         ## 작성 지침\n\n\
         1. 웹 검색을 통해 주제에 관한 주요 사건들을 조사하세요.\n\
         2. 각 이벤트를 **날짜(또는 시기) | 사건 | 의미** 형식으로 정리하세요.\n\
         3. 가능한 한 정확한 날짜를 사용하고, 불확실한 경우 \"경\" 또는 \"무렵\"으로 표시하세요.\n\
         4. 탐사보도나 사건 기사 작성에 활용할 수 있도록 인과관계를 포함하세요.\n\
         5. 출처가 확인된 사실만 포함하고, 불확실한 내용은 ⚠로 표시하세요.\n\n\
         ## 출력 형식\n\n\
         ```\n\
         # [주제] 타임라인\n\n\
         ## 배경\n\
         (주제에 대한 간략한 배경 설명)\n\n\
         ## 타임라인\n\
         | 날짜 | 사건 | 의미/영향 |\n\
         |------|------|----------|\n\
         | YYYY-MM-DD | 사건 설명 | 영향 설명 |\n\n\
         ## 핵심 쟁점\n\
         (현재 진행 중인 쟁점이나 향후 주목할 사항)\n\n\
         ## 출처\n\
         (참고한 주요 출처 목록)\n\
         ```\n"
    );

    if !research.is_empty() {
        prompt.push_str("\n## 참고할 기존 리서치 자료\n\n");
        for (filename, content) in research {
            prompt.push_str(&format!("### {filename}\n\n{content}\n\n---\n\n"));
        }
        prompt.push_str(
            "위 리서치 자료에서 날짜와 이벤트를 추출하고, 웹 검색으로 추가 사건을 보강해주세요.\n",
        );
    }

    prompt
}

/// Handle the `/timeline` command: generate a chronological event timeline for a topic.
pub async fn handle_timeline(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let topic = input.strip_prefix("/timeline").unwrap_or("").trim();

    if topic.is_empty() {
        println!("{DIM}  사용법: /timeline <주제>{RESET}");
        println!("{DIM}  예시:   /timeline 후쿠시마 오염수 방류{RESET}");
        println!("{DIM}  주제에 관한 시간순 이벤트 타임라인을 생성합니다.{RESET}");
        println!("{DIM}  리서치 자료에서 날짜/이벤트를 추출하고 웹 검색으로 보강합니다.{RESET}\n");
        return;
    }

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
    }
    println!();

    let prompt = build_timeline_prompt(topic, &research);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save timeline to .journalist/timeline/
    if !response.trim().is_empty() {
        let path = timeline_file_path(topic);
        match save_timeline(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 타임라인 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  타임라인 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /deadline ────────────────────────────────────────────────────────────

const DEADLINES_FILE: &str = ".journalist/deadlines.json";

/// A single deadline entry.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Deadline {
    title: String,
    /// ISO 8601 datetime string (e.g. "2026-03-20T09:00:00")
    datetime: String,
}

fn deadlines_path() -> std::path::PathBuf {
    std::path::PathBuf::from(DEADLINES_FILE)
}

fn load_deadlines_from(path: &std::path::Path) -> Vec<Deadline> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_deadlines_to(deadlines: &[Deadline], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(deadlines).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

/// Get today's date as "YYYY-MM-DD" string using local timezone.
pub fn today_date_string() -> String {
    // Use the `date` command output format or calculate from SystemTime
    // We'll compute from UNIX epoch + local offset
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_date_from_epoch(now.as_secs())
}

/// Format epoch seconds as "YYYY-MM-DD" (UTC).
fn format_date_from_epoch(secs: u64) -> String {
    // Reuse the existing format_unix_timestamp and take just the date part
    let ts = format_unix_timestamp(secs);
    ts.split(' ').next().unwrap_or("2026-01-01").to_string()
}

/// Parse a time/datetime string into an ISO 8601 datetime.
/// Accepts: "18:00", "2026-03-20 09:00", "2026-03-20T09:00"
fn parse_deadline_datetime(input: &str) -> Option<String> {
    parse_deadline_datetime_with_today(input, &today_date_string())
}

/// Testable version that accepts today's date as parameter.
fn parse_deadline_datetime_with_today(input: &str, today: &str) -> Option<String> {
    let input = input.trim();
    // Full datetime: "2026-03-20 09:00" or "2026-03-20T09:00"
    if input.len() >= 16 && (input.contains('T') || input.chars().filter(|c| *c == '-').count() >= 2)
    {
        let normalized = input.replace('T', " ");
        let parts: Vec<&str> = normalized.split(' ').collect();
        if parts.len() >= 2 {
            let date = parts[0];
            let time = parts[1];
            let date_parts: Vec<&str> = date.split('-').collect();
            if date_parts.len() == 3
                && date_parts[0].len() == 4
                && date_parts[1].len() == 2
                && date_parts[2].len() == 2
            {
                let time_parts: Vec<&str> = time.split(':').collect();
                if time_parts.len() >= 2 {
                    return Some(format!("{}T{}:00", date, time));
                }
            }
        }
        return None;
    }

    // Time only: "18:00" — use today's date
    if input.contains(':') && input.len() <= 5 {
        let time_parts: Vec<&str> = input.split(':').collect();
        if time_parts.len() == 2
            && time_parts[0].parse::<u32>().is_ok()
            && time_parts[1].parse::<u32>().is_ok()
        {
            return Some(format!("{today}T{input}:00"));
        }
    }

    None
}

/// Parse "YYYY-MM-DDTHH:MM:SS" into epoch seconds (UTC).
fn datetime_to_epoch(datetime: &str) -> Option<u64> {
    // Parse "YYYY-MM-DDTHH:MM:SS"
    let dt = datetime.replace('T', " ");
    let parts: Vec<&str> = dt.split(' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let date_parts: Vec<u64> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    let time_parts: Vec<u64> = parts[1].split(':').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() != 3 || time_parts.len() < 2 {
        return None;
    }
    let (year, month, day) = (date_parts[0], date_parts[1], date_parts[2]);
    let (hour, minute) = (time_parts[0], time_parts[1]);
    let second = time_parts.get(2).copied().unwrap_or(0);

    // Simple days-from-epoch calculation
    let mut total_days: i64 = 0;
    for y in 1970..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }
    let days_in_months = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += days_in_months[m as usize] as i64;
        if m == 2 && is_leap_year(year) {
            total_days += 1;
        }
    }
    total_days += (day as i64) - 1;

    let epoch = total_days * 86400 + (hour as i64) * 3600 + (minute as i64) * 60 + second as i64;
    if epoch >= 0 {
        Some(epoch as u64)
    } else {
        None
    }
}

fn is_leap_year(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

/// Calculate remaining time from now to a deadline datetime string.
/// Returns (total_seconds_remaining, human_readable_string).
fn remaining_time(datetime: &str) -> (i64, String) {
    let target_epoch = match datetime_to_epoch(datetime) {
        Some(e) => e as i64,
        None => return (0, "파싱 불가".to_string()),
    };

    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let total_secs = target_epoch - now_epoch;

    if total_secs <= 0 {
        let elapsed = -total_secs;
        let hours = elapsed / 3600;
        let mins = (elapsed % 3600) / 60;
        if hours > 0 {
            return (total_secs, format!("{hours}시간 {mins}분 초과"));
        }
        return (total_secs, format!("{mins}분 초과"));
    }

    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    if hours >= 24 {
        let days = hours / 24;
        let rem_hours = hours % 24;
        (total_secs, format!("{days}일 {rem_hours}시간 {mins}분 남음"))
    } else if hours > 0 {
        (total_secs, format!("{hours}시간 {mins}분 남음"))
    } else {
        (total_secs, format!("{mins}분 남음"))
    }
}

/// Handle `/deadline` command with subcommands: set, list, clear.
pub fn handle_deadline(input: &str) {
    let args = input.strip_prefix("/deadline").unwrap_or("").trim();

    if args.is_empty() {
        // Default to list
        handle_deadline_list();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "set" => handle_deadline_set(rest),
        "list" => handle_deadline_list(),
        "clear" => handle_deadline_clear(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_deadline_usage();
        }
    }
}

fn print_deadline_usage() {
    println!("{DIM}  사용법:");
    println!("    /deadline set <제목> <시간>   마감 설정 (예: 18:00, 2026-03-20 09:00)");
    println!("    /deadline list               활성 마감 목록 (남은 시간 순)");
    println!("    /deadline clear <제목>       마감 해제");
    println!("    /deadline                    (list와 동일){RESET}\n");
}

fn handle_deadline_set(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /deadline set <제목> <시간>{RESET}\n");
        return;
    }

    // Parse: last token(s) that look like time, rest is title
    // Try to find time at end: "제목 18:00" or "제목 2026-03-20 09:00"
    let parts: Vec<&str> = args.rsplitn(3, char::is_whitespace).collect();

    let (title, time_str) = if parts.len() >= 3 {
        // Try "title date time" pattern first
        let maybe_datetime = format!("{} {}", parts[1], parts[0]);
        if parse_deadline_datetime(&maybe_datetime).is_some() {
            let title_end = args.len() - parts[0].len() - parts[1].len() - 2;
            (&args[..title_end], maybe_datetime)
        } else if parse_deadline_datetime(parts[0]).is_some() {
            let title_end = args.len() - parts[0].len() - 1;
            (&args[..title_end], parts[0].to_string())
        } else {
            eprintln!("{RED}  시간 형식을 인식할 수 없습니다: {}{RESET}", parts[0]);
            eprintln!("{DIM}  예: 18:00, 2026-03-20 09:00{RESET}\n");
            return;
        }
    } else if parts.len() == 2 {
        if parse_deadline_datetime(parts[0]).is_some() {
            let title_end = args.len() - parts[0].len() - 1;
            (&args[..title_end], parts[0].to_string())
        } else {
            eprintln!("{RED}  시간 형식을 인식할 수 없습니다: {}{RESET}", parts[0]);
            eprintln!("{DIM}  예: 18:00, 2026-03-20 09:00{RESET}\n");
            return;
        }
    } else {
        eprintln!("{RED}  제목과 시간을 모두 지정하세요: /deadline set <제목> <시간>{RESET}\n");
        return;
    };

    let datetime = match parse_deadline_datetime(&time_str) {
        Some(dt) => dt,
        None => {
            eprintln!("{RED}  시간 형식을 인식할 수 없습니다: {time_str}{RESET}");
            eprintln!("{DIM}  예: 18:00, 2026-03-20 09:00{RESET}\n");
            return;
        }
    };

    let path = deadlines_path();
    let mut deadlines = load_deadlines_from(&path);

    // Update existing or add new
    if let Some(existing) = deadlines.iter_mut().find(|d| d.title == title) {
        existing.datetime = datetime.clone();
    } else {
        deadlines.push(Deadline {
            title: title.to_string(),
            datetime: datetime.clone(),
        });
    }

    save_deadlines_to(&deadlines, &path);

    let (_, remaining) = remaining_time(&datetime);
    println!(
        "{GREEN}  ⏰ 마감 설정: {title} → {datetime} ({remaining}){RESET}\n"
    );
}

fn handle_deadline_list() {
    let path = deadlines_path();
    let deadlines = load_deadlines_from(&path);

    if deadlines.is_empty() {
        println!("{DIM}  설정된 마감이 없습니다.{RESET}\n");
        return;
    }

    // Sort by remaining time (ascending — most urgent first)
    let mut with_remaining: Vec<(Deadline, i64, String)> = deadlines
        .iter()
        .map(|d| {
            let (secs, text) = remaining_time(&d.datetime);
            (d.clone(), secs, text)
        })
        .collect();
    with_remaining.sort_by_key(|(_, secs, _)| *secs);

    println!("{BOLD}  ⏰ 마감 목록{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");
    for (deadline, secs, remaining_text) in &with_remaining {
        if *secs <= 0 {
            // Overdue — highlight in red
            println!(
                "  {RED}🔴 {}: {} ({}){RESET}",
                deadline.title, deadline.datetime, remaining_text
            );
        } else if *secs <= 3600 {
            // Less than 1 hour — highlight in yellow
            println!(
                "  {YELLOW}🟡 {}: {} ({}){RESET}",
                deadline.title, deadline.datetime, remaining_text
            );
        } else {
            println!(
                "  {GREEN}🟢 {}: {} ({}){RESET}",
                deadline.title, deadline.datetime, remaining_text
            );
        }
    }
    println!();
}

fn handle_deadline_clear(title: &str) {
    if title.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /deadline clear <제목>{RESET}\n");
        return;
    }

    let path = deadlines_path();
    let mut deadlines = load_deadlines_from(&path);
    let before_len = deadlines.len();
    deadlines.retain(|d| d.title != title);

    if deadlines.len() == before_len {
        eprintln!("{DIM}  '{title}' 마감을 찾을 수 없습니다.{RESET}\n");
        return;
    }

    save_deadlines_to(&deadlines, &path);
    println!("{GREEN}  ✅ 마감 해제: {title}{RESET}\n");
}

// ── /embargo ────────────────────────────────────────────────────────────

const EMBARGOES_FILE: &str = ".journalist/embargoes.json";

/// A single embargo entry.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Embargo {
    title: String,
    /// ISO 8601 datetime string for embargo release (e.g. "2026-03-21T09:00:00")
    release_at: String,
}

fn embargoes_path() -> std::path::PathBuf {
    std::path::PathBuf::from(EMBARGOES_FILE)
}

fn load_embargoes_from(path: &std::path::Path) -> Vec<Embargo> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_embargoes_to(embargoes: &[Embargo], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(embargoes).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

/// Handle `/embargo` command with subcommands: set, list, clear.
pub fn handle_embargo(input: &str) {
    let args = input.strip_prefix("/embargo").unwrap_or("").trim();

    if args.is_empty() {
        handle_embargo_list();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "set" => handle_embargo_set(rest),
        "list" => handle_embargo_list(),
        "clear" => handle_embargo_clear(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_embargo_usage();
        }
    }
}

fn print_embargo_usage() {
    println!("{DIM}  사용법:");
    println!("    /embargo set <제목> <해제시각>   엠바고 등록 (예: 09:00, 2026-03-21 09:00)");
    println!("    /embargo list                    활성 엠바고 목록 (해제 시각 순)");
    println!("    /embargo clear <번호>            엠바고 삭제 (목록 번호)");
    println!("    /embargo                         (list와 동일){RESET}\n");
}

fn handle_embargo_set(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /embargo set <제목> <해제시각>{RESET}\n");
        return;
    }

    // Strip surrounding quotes from title if present
    let (title, time_str) = parse_embargo_args(args);

    if title.is_empty() || time_str.is_empty() {
        eprintln!("{RED}  제목과 해제 시각을 모두 지정하세요: /embargo set <제목> <시각>{RESET}\n");
        return;
    }

    let datetime = match parse_deadline_datetime(&time_str) {
        Some(dt) => dt,
        None => {
            eprintln!("{RED}  시간 형식을 인식할 수 없습니다: {time_str}{RESET}");
            eprintln!("{DIM}  예: 09:00, 2026-03-21 09:00{RESET}\n");
            return;
        }
    };

    let path = embargoes_path();
    let mut embargoes = load_embargoes_from(&path);

    // Update existing or add new
    if let Some(existing) = embargoes.iter_mut().find(|e| e.title == title) {
        existing.release_at = datetime.clone();
    } else {
        embargoes.push(Embargo {
            title: title.to_string(),
            release_at: datetime.clone(),
        });
    }

    save_embargoes_to(&embargoes, &path);

    let (_, remaining) = remaining_time(&datetime);
    println!(
        "{GREEN}  🔒 엠바고 등록: {title} → {datetime} ({remaining}){RESET}\n"
    );
}

/// Parse embargo set arguments, handling quoted titles.
/// Returns (title, time_string).
fn parse_embargo_args(args: &str) -> (String, String) {
    // Check for quoted title: "제목" 2026-03-21 09:00
    if args.starts_with('"') {
        if let Some(end_quote) = args[1..].find('"') {
            let title = &args[1..end_quote + 1];
            let rest = args[end_quote + 2..].trim();
            return (title.to_string(), rest.to_string());
        }
    }

    // Unquoted: same logic as deadline — time tokens at the end
    let parts: Vec<&str> = args.rsplitn(3, char::is_whitespace).collect();

    if parts.len() >= 3 {
        let maybe_datetime = format!("{} {}", parts[1], parts[0]);
        if parse_deadline_datetime(&maybe_datetime).is_some() {
            let title_end = args.len() - parts[0].len() - parts[1].len() - 2;
            return (args[..title_end].to_string(), maybe_datetime);
        }
        if parse_deadline_datetime(parts[0]).is_some() {
            let title_end = args.len() - parts[0].len() - 1;
            return (args[..title_end].to_string(), parts[0].to_string());
        }
    } else if parts.len() == 2 {
        if parse_deadline_datetime(parts[0]).is_some() {
            let title_end = args.len() - parts[0].len() - 1;
            return (args[..title_end].to_string(), parts[0].to_string());
        }
    }

    (String::new(), String::new())
}

fn handle_embargo_list() {
    let path = embargoes_path();
    let embargoes = load_embargoes_from(&path);

    if embargoes.is_empty() {
        println!("{DIM}  등록된 엠바고가 없습니다.{RESET}\n");
        return;
    }

    // Sort by release time (ascending — earliest release first)
    let mut with_remaining: Vec<(usize, &Embargo, i64, String)> = embargoes
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (secs, text) = remaining_time(&e.release_at);
            (i + 1, e, secs, text)
        })
        .collect();
    with_remaining.sort_by_key(|(_, _, secs, _)| *secs);

    println!("{BOLD}  🔒 엠바고 목록{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");
    for (idx, embargo, secs, remaining_text) in &with_remaining {
        if *secs <= 0 {
            // Released
            println!(
                "  {GREEN}🟢 [{idx}] {}: {} (해제됨 — {}){RESET}",
                embargo.title, embargo.release_at, remaining_text
            );
        } else if *secs <= 3600 {
            // Less than 1 hour until release
            println!(
                "  {YELLOW}🟡 [{idx}] {}: {} ({}){RESET}",
                embargo.title, embargo.release_at, remaining_text
            );
        } else {
            // Active embargo
            println!(
                "  {RED}🔴 [{idx}] {}: {} ({}){RESET}",
                embargo.title, embargo.release_at, remaining_text
            );
        }
    }
    println!();
}

fn handle_embargo_clear(num_str: &str) {
    if num_str.is_empty() {
        eprintln!("{RED}  번호를 지정하세요: /embargo clear <번호>{RESET}\n");
        return;
    }

    let idx: usize = match num_str.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  유효한 번호를 지정하세요: /embargo clear <번호>{RESET}\n");
            return;
        }
    };

    let path = embargoes_path();
    let mut embargoes = load_embargoes_from(&path);

    if idx < 1 || idx > embargoes.len() {
        eprintln!(
            "{RED}  번호 {idx}에 해당하는 엠바고가 없습니다. (총 {}개){RESET}\n",
            embargoes.len()
        );
        return;
    }

    let removed = embargoes.remove(idx - 1);
    save_embargoes_to(&embargoes, &path);
    println!(
        "{GREEN}  ✅ 엠바고 삭제: [{}] {}{RESET}\n",
        idx, removed.title
    );
}

// ── /data — 데이터 저널리즘 분석 지원 ─────────────────────────────────

const DATA_DIR: &str = ".journalist/data";

/// Handle the /data command: data journalism analysis support.
pub async fn handle_data(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/data").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("help") {
        "analyze" => {
            let rest = args.strip_prefix("analyze").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /data analyze <파일경로>     CSV/데이터 파일 AI 분석{RESET}");
                println!("{DIM}  예시:   /data analyze sales_2025.csv{RESET}");
                println!("{DIM}  결과:   핵심 수치, 추세, 이상치, 기사 앵글 제안{RESET}\n");
            } else {
                data_analyze(agent, rest, session_total, model).await;
            }
        }
        "summarize" => {
            let rest = args.strip_prefix("summarize").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /data summarize <파일경로>   기본 통계 요약 (로컬){RESET}");
                println!("{DIM}  예시:   /data summarize data.csv{RESET}");
                println!("{DIM}  결과:   행/열 수, 수치 칼럼 통계, 결측치 현황{RESET}\n");
            } else {
                data_summarize(rest);
            }
        }
        "compare" => {
            let rest = args.strip_prefix("compare").unwrap_or("").trim();
            let files: Vec<&str> = rest.split_whitespace().collect();
            if files.len() < 2 {
                println!("{DIM}  사용법: /data compare <파일1> <파일2>   두 데이터셋 비교 분석{RESET}");
                println!("{DIM}  예시:   /data compare 2024.csv 2025.csv{RESET}\n");
            } else {
                data_compare(agent, files[0], files[1], session_total, model).await;
            }
        }
        "help" | _ if args.is_empty() || args == "help" => {
            println!("{DIM}  /data — 데이터 저널리즘 분석 지원{RESET}");
            println!("{DIM}  하위 커맨드:{RESET}");
            println!("{DIM}    analyze  <파일>          AI 분석 (핵심 수치, 추세, 이상치, 기사 앵글){RESET}");
            println!("{DIM}    summarize <파일>         로컬 기본 통계 (행/열, 수치 통계, 결측치){RESET}");
            println!("{DIM}    compare  <파일1> <파일2> 두 데이터셋 차이 분석{RESET}\n");
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /data [analyze|summarize|compare]{RESET}\n");
        }
    }
}

/// Parse CSV content into headers and rows. Returns (headers, rows).
pub fn parse_csv(content: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut lines = content.lines();
    let headers: Vec<String> = match lines.next() {
        Some(line) => line.split(',').map(|s| s.trim().to_string()).collect(),
        None => return (vec![], vec![]),
    };

    let rows: Vec<Vec<String>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|line| line.split(',').map(|s| s.trim().to_string()).collect())
        .collect();

    (headers, rows)
}

/// Compute basic stats for a numeric column: count, min, max, mean.
pub fn compute_column_stats(values: &[f64]) -> (usize, f64, f64, f64) {
    let count = values.len();
    if count == 0 {
        return (0, 0.0, 0.0, 0.0);
    }
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = values.iter().sum::<f64>() / count as f64;
    (count, min, max, mean)
}

/// Build a summary report from CSV content (local computation, no AI).
pub fn build_csv_summary(content: &str) -> String {
    let (headers, rows) = parse_csv(content);
    let num_rows = rows.len();
    let num_cols = headers.len();

    let mut report = format!("## 데이터 요약\n\n- 행 수: {num_rows}\n- 열 수: {num_cols}\n- 칼럼: {}\n\n", headers.join(", "));

    // Identify numeric columns and compute stats
    let mut numeric_stats: Vec<(String, usize, f64, f64, f64, usize)> = Vec::new();

    for (col_idx, header) in headers.iter().enumerate() {
        let mut values: Vec<f64> = Vec::new();
        let mut missing = 0usize;

        for row in &rows {
            if col_idx < row.len() {
                let cell = row[col_idx].trim();
                if cell.is_empty() || cell == "NA" || cell == "N/A" || cell == "-" {
                    missing += 1;
                } else if let Ok(v) = cell.replace(['_', ' '], "").parse::<f64>() {
                    values.push(v);
                }
            } else {
                missing += 1;
            }
        }

        if !values.is_empty() {
            let (count, min, max, mean) = compute_column_stats(&values);
            numeric_stats.push((header.clone(), count, min, max, mean, missing));
        } else if missing > 0 {
            // Non-numeric column with missing values
            report.push_str(&format!("### {header}\n- 결측치: {missing}건\n\n"));
        }
    }

    if !numeric_stats.is_empty() {
        report.push_str("## 수치 칼럼 통계\n\n");
        report.push_str("| 칼럼 | 유효값 | 최솟값 | 최댓값 | 평균 | 결측치 |\n");
        report.push_str("|------|--------|--------|--------|------|--------|\n");
        for (name, count, min, max, mean, missing) in &numeric_stats {
            report.push_str(&format!(
                "| {name} | {count} | {min:.2} | {max:.2} | {mean:.2} | {missing} |\n"
            ));
        }
        report.push('\n');
    }

    report
}

/// Local summarize: read CSV and display basic stats.
fn data_summarize(file_path: &str) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
            return;
        }
    };

    let summary = build_csv_summary(&content);
    println!("\n{summary}");

    // Save result
    let save_path = std::path::Path::new(DATA_DIR).join("last_summary.md");
    ensure_sources_dir_at(&save_path);
    match std::fs::write(&save_path, &summary) {
        Ok(_) => println!("{GREEN}  ✓ 요약 저장: {}{RESET}\n", save_path.display()),
        Err(e) => eprintln!("{RED}  저장 실패: {e}{RESET}\n"),
    }
}

/// Build AI prompt for data analysis.
pub fn build_data_analyze_prompt(file_path: &str, content: &str) -> String {
    let summary = build_csv_summary(content);
    format!(
        "데이터 파일 '{file_path}'을(를) 분석해주세요.\n\n\
         === 기본 통계 ===\n{summary}\n\
         === 원본 데이터 (앞부분) ===\n{data}\n\n\
         다음 항목을 포함해 분석해주세요:\n\n\
         ## 1. 핵심 수치\n\
         가장 눈에 띄는 수치와 그 의미를 설명하세요.\n\n\
         ## 2. 추세 분석\n\
         시계열적 변화나 패턴이 있다면 식별하세요.\n\n\
         ## 3. 이상치 식별\n\
         평균에서 크게 벗어나는 값이나 눈에 띄는 특이점을 찾으세요.\n\n\
         ## 4. 기사 앵글 제안\n\
         이 데이터에서 뽑을 수 있는 기사 앵글을 3~5개 제안하세요. \
         각 앵글의 독자 관심도와 뉴스 가치를 한 줄로 설명하세요.\n\n\
         ## 5. 추가 취재 포인트\n\
         이 데이터만으로는 부족한 점, 추가로 확인해야 할 사항을 제시하세요.",
        data = content.lines().take(50).collect::<Vec<_>>().join("\n")
    )
}

/// Build AI prompt for comparing two datasets.
pub fn build_data_compare_prompt(
    file1: &str,
    content1: &str,
    file2: &str,
    content2: &str,
) -> String {
    let summary1 = build_csv_summary(content1);
    let summary2 = build_csv_summary(content2);
    format!(
        "두 데이터셋을 비교 분석해주세요.\n\n\
         === 데이터셋 1: '{file1}' ===\n{summary1}\n\
         원본 (앞부분):\n{data1}\n\n\
         === 데이터셋 2: '{file2}' ===\n{summary2}\n\
         원본 (앞부분):\n{data2}\n\n\
         다음 항목을 포함해 분석해주세요:\n\n\
         ## 1. 구조 비교\n\
         두 데이터셋의 칼럼 구성, 행 수, 데이터 형태 차이를 비교하세요.\n\n\
         ## 2. 수치 변화\n\
         공통 칼럼의 주요 수치(합계, 평균 등) 변화를 분석하세요.\n\n\
         ## 3. 주목할 변화\n\
         가장 큰 증감이나 역전 현상을 식별하세요.\n\n\
         ## 4. 기사 앵글 제안\n\
         두 데이터셋의 비교에서 뽑을 수 있는 기사 앵글을 3~5개 제안하세요.\n\n\
         ## 5. 주의사항\n\
         비교 시 주의할 점(단위 차이, 기간 차이 등)을 제시하세요.",
        data1 = content1.lines().take(30).collect::<Vec<_>>().join("\n"),
        data2 = content2.lines().take(30).collect::<Vec<_>>().join("\n"),
    )
}

/// AI-powered data analysis.
async fn data_analyze(
    agent: &mut Agent,
    file_path: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
            return;
        }
    };

    println!("{DIM}  '{file_path}' 데이터 분석 중...{RESET}");

    let prompt = build_data_analyze_prompt(file_path, &content);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if !response.trim().is_empty() {
        let save_path = std::path::Path::new(DATA_DIR).join("last_analysis.md");
        ensure_sources_dir_at(&save_path);
        match std::fs::write(&save_path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 분석 결과 저장: {}{RESET}\n",
                    save_path.display()
                );
            }
            Err(e) => eprintln!("{RED}  저장 실패: {e}{RESET}\n"),
        }
    }
}

/// AI-powered comparison of two datasets.
async fn data_compare(
    agent: &mut Agent,
    file1: &str,
    file2: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let content1 = match std::fs::read_to_string(file1) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  파일 1 읽기 실패 ({file1}): {e}{RESET}\n");
            return;
        }
    };
    let content2 = match std::fs::read_to_string(file2) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  파일 2 읽기 실패 ({file2}): {e}{RESET}\n");
            return;
        }
    };

    println!("{DIM}  '{file1}' vs '{file2}' 비교 분석 중...{RESET}");

    let prompt = build_data_compare_prompt(file1, &content1, file2, &content2);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if !response.trim().is_empty() {
        let save_path = std::path::Path::new(DATA_DIR).join("last_compare.md");
        ensure_sources_dir_at(&save_path);
        match std::fs::write(&save_path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 비교 분석 저장: {}{RESET}\n",
                    save_path.display()
                );
            }
            Err(e) => eprintln!("{RED}  저장 실패: {e}{RESET}\n"),
        }
    }
}

// ── /desk ────────────────────────────────────────────────────────────────

const DESK_ASSIGNMENTS_FILE: &str = ".journalist/desk/assignments.json";

/// Status of a desk assignment.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
enum DeskStatus {
    Pending,
    Done,
}

/// A single desk assignment (데스크 → 기자 업무 지시).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct DeskAssignment {
    reporter: String,
    content: String,
    deadline: Option<String>,
    status: DeskStatus,
    feedback: Vec<String>,
    /// true if this was a reporter pitch rather than a desk assignment
    #[serde(default)]
    is_pitch: bool,
    created_at: String,
}

fn desk_path() -> std::path::PathBuf {
    std::path::PathBuf::from(DESK_ASSIGNMENTS_FILE)
}

fn load_desk_from(path: &std::path::Path) -> Vec<DeskAssignment> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_desk_to(assignments: &[DeskAssignment], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(assignments).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

/// Handle `/desk` command with subcommands: assign, list, done, feedback, pitch.
pub fn handle_desk(input: &str) {
    let args = input.strip_prefix("/desk").unwrap_or("").trim();

    if args.is_empty() {
        handle_desk_list("");
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "assign" => handle_desk_assign(rest),
        "list" => handle_desk_list(rest),
        "done" => handle_desk_done(rest),
        "feedback" => handle_desk_feedback(rest),
        "pitch" => handle_desk_pitch(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_desk_usage();
        }
    }
}

fn print_desk_usage() {
    println!("{DIM}  사용법:");
    println!("    /desk assign <기자> <내용> [--deadline HH:MM]  업무 지시");
    println!("    /desk list [--reporter 기자명]                 업무 목록 (마감순)");
    println!("    /desk done <번호>                              완료 처리");
    println!("    /desk feedback <번호> <내용>                   피드백 추가");
    println!("    /desk pitch <제목> <내용>                      기사 아이디어 제안");
    println!("    /desk                                          (list와 동일){RESET}\n");
}

/// Parse reporter, content, and optional --deadline from assign args.
fn parse_desk_assign_args(args: &str) -> Option<(String, String, Option<String>)> {
    // First token is reporter name
    let (reporter, rest) = match args.split_once(char::is_whitespace) {
        Some((r, rest)) => (r.trim().to_string(), rest.trim()),
        None => return None, // need at least reporter + content
    };

    if rest.is_empty() {
        return None;
    }

    // Check for --deadline flag
    if let Some(dl_pos) = rest.find("--deadline") {
        let content = rest[..dl_pos].trim().to_string();
        let deadline_str = rest[dl_pos + 10..].trim().to_string();
        let deadline = if deadline_str.is_empty() {
            None
        } else {
            Some(deadline_str)
        };
        if content.is_empty() {
            return None;
        }
        Some((reporter, content, deadline))
    } else {
        Some((reporter, rest.to_string(), None))
    }
}

fn handle_desk_assign(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /desk assign <기자> <내용> [--deadline HH:MM]{RESET}\n");
        return;
    }

    let (reporter, content, deadline) = match parse_desk_assign_args(args) {
        Some(v) => v,
        None => {
            eprintln!("{RED}  사용법: /desk assign <기자> <내용> [--deadline HH:MM]{RESET}\n");
            return;
        }
    };

    // Validate deadline format (HH:MM) if provided
    if let Some(ref dl) = deadline {
        if !is_valid_time(dl) {
            eprintln!("{RED}  시간 형식이 올바르지 않습니다: {dl}{RESET}");
            eprintln!("{DIM}  예: 15:30{RESET}\n");
            return;
        }
    }

    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let date = format_unix_timestamp(secs);
        date.replace(' ', "T") + ":00"
    };

    let path = desk_path();
    let mut assignments = load_desk_from(&path);

    assignments.push(DeskAssignment {
        reporter: reporter.clone(),
        content: content.clone(),
        deadline: deadline.clone(),
        status: DeskStatus::Pending,
        feedback: Vec::new(),
        is_pitch: false,
        created_at: now,
    });

    save_desk_to(&assignments, &path);

    let dl_text = deadline
        .as_deref()
        .map(|d| format!(" (마감: {d})"))
        .unwrap_or_default();
    println!("{GREEN}  📋 업무 지시: {reporter} ← {content}{dl_text}{RESET}\n");
}

fn handle_desk_list(args: &str) {
    // Parse --reporter filter
    let reporter_filter = if let Some(pos) = args.find("--reporter") {
        let after = args[pos + 10..].trim();
        if after.is_empty() {
            None
        } else {
            Some(after.split_whitespace().next().unwrap_or("").to_string())
        }
    } else {
        None
    };

    let path = desk_path();
    let assignments = load_desk_from(&path);

    let active: Vec<(usize, &DeskAssignment)> = assignments
        .iter()
        .enumerate()
        .filter(|(_, a)| a.status == DeskStatus::Pending)
        .filter(|(_, a)| {
            reporter_filter
                .as_ref()
                .map_or(true, |r| a.reporter == *r)
        })
        .collect();

    if active.is_empty() {
        if let Some(ref r) = reporter_filter {
            println!("{DIM}  {r} 기자의 대기 중인 업무가 없습니다.{RESET}\n");
        } else {
            println!("{DIM}  대기 중인 업무가 없습니다.{RESET}\n");
        }
        return;
    }

    // Sort by deadline (entries with deadline first, then ascending; no-deadline last)
    let mut sorted = active;
    sorted.sort_by(|(_, a), (_, b)| match (&a.deadline, &b.deadline) {
        (Some(da), Some(db)) => da.cmp(db),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.created_at.cmp(&b.created_at),
    });

    println!("{BOLD}  📋 데스크 업무 목록{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");

    for (idx, assignment) in &sorted {
        let num = idx + 1;
        let dl_text = assignment
            .deadline
            .as_deref()
            .map(|d| format!(" [마감: {d}]"))
            .unwrap_or_default();

        let kind = if assignment.is_pitch {
            "💡"
        } else {
            "📝"
        };

        let fb_count = assignment.feedback.len();
        let fb_text = if fb_count > 0 {
            format!(" ({fb_count}건 피드백)")
        } else {
            String::new()
        };

        // Color based on deadline urgency
        if assignment.deadline.is_some() {
            println!(
                "  {YELLOW}{kind} #{num} [{reporter}] {content}{dl_text}{fb_text}{RESET}",
                reporter = assignment.reporter,
                content = assignment.content
            );
        } else {
            println!(
                "  {GREEN}{kind} #{num} [{reporter}] {content}{dl_text}{fb_text}{RESET}",
                reporter = assignment.reporter,
                content = assignment.content
            );
        }
    }
    println!();
}

fn handle_desk_done(num_str: &str) {
    if num_str.is_empty() {
        eprintln!("{RED}  번호를 지정하세요: /desk done <번호>{RESET}\n");
        return;
    }

    let num: usize = match num_str.trim().parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요: {num_str}{RESET}\n");
            return;
        }
    };

    let path = desk_path();
    let mut assignments = load_desk_from(&path);
    let idx = num - 1;

    if idx >= assignments.len() {
        eprintln!("{RED}  #{num}번 업무를 찾을 수 없습니다.{RESET}\n");
        return;
    }

    if assignments[idx].status == DeskStatus::Done {
        println!("{DIM}  #{num}번은 이미 완료 처리되었습니다.{RESET}\n");
        return;
    }

    assignments[idx].status = DeskStatus::Done;
    let content = assignments[idx].content.clone();
    let reporter = assignments[idx].reporter.clone();
    save_desk_to(&assignments, &path);
    println!("{GREEN}  ✅ 업무 완료: #{num} [{reporter}] {content}{RESET}\n");
}

fn handle_desk_feedback(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /desk feedback <번호> <내용>{RESET}\n");
        return;
    }

    let (num_str, feedback) = match args.split_once(char::is_whitespace) {
        Some((n, f)) => (n.trim(), f.trim()),
        None => {
            eprintln!("{RED}  사용법: /desk feedback <번호> <내용>{RESET}\n");
            return;
        }
    };

    if feedback.is_empty() {
        eprintln!("{RED}  피드백 내용을 입력하세요: /desk feedback <번호> <내용>{RESET}\n");
        return;
    }

    let num: usize = match num_str.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요: {num_str}{RESET}\n");
            return;
        }
    };

    let path = desk_path();
    let mut assignments = load_desk_from(&path);
    let idx = num - 1;

    if idx >= assignments.len() {
        eprintln!("{RED}  #{num}번 업무를 찾을 수 없습니다.{RESET}\n");
        return;
    }

    assignments[idx].feedback.push(feedback.to_string());
    let content = assignments[idx].content.clone();
    save_desk_to(&assignments, &path);
    println!("{GREEN}  💬 피드백 추가: #{num} {content}{RESET}");
    println!("{DIM}  → {feedback}{RESET}\n");
}

fn handle_desk_pitch(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /desk pitch <제목> <내용>{RESET}\n");
        return;
    }

    let (title, description) = match args.split_once(char::is_whitespace) {
        Some((t, d)) => (t.trim().to_string(), d.trim().to_string()),
        None => {
            eprintln!("{RED}  사용법: /desk pitch <제목> <내용>{RESET}\n");
            return;
        }
    };

    if description.is_empty() {
        eprintln!("{RED}  내용을 입력하세요: /desk pitch <제목> <내용>{RESET}\n");
        return;
    }

    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let date = format_unix_timestamp(secs);
        date.replace(' ', "T") + ":00"
    };

    let path = desk_path();
    let mut assignments = load_desk_from(&path);

    assignments.push(DeskAssignment {
        reporter: "제안".to_string(),
        content: format!("[{title}] {description}"),
        deadline: None,
        status: DeskStatus::Pending,
        feedback: Vec::new(),
        is_pitch: true,
        created_at: now,
    });

    save_desk_to(&assignments, &path);
    println!("{GREEN}  💡 기사 아이디어 제안: {title}{RESET}");
    println!("{DIM}  → {description}{RESET}\n");
}

/// Validate HH:MM time format.
fn is_valid_time(s: &str) -> bool {
    if s.len() != 5 {
        return false;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    parts[0].len() == 2
        && parts[1].len() == 2
        && parts[0].parse::<u32>().map_or(false, |h| h < 24)
        && parts[1].parse::<u32>().map_or(false, |m| m < 60)
}

// ── /collaborate ─────────────────────────────────────────────────────────

const COLLABORATE_DIR: &str = ".journalist/collaborate";

/// A collaborative reporting project.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct CollabProject {
    name: String,
    reporters: Vec<String>,
    notes: Vec<CollabNote>,
    status: CollabStatus,
    created_at: String,
}

/// A single note within a collaborative project.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct CollabNote {
    reporter: String,
    content: String,
    timestamp: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
enum CollabStatus {
    Active,
    Closed,
}

fn collab_project_path(project_name: &str) -> std::path::PathBuf {
    std::path::Path::new(COLLABORATE_DIR).join(format!("{project_name}.json"))
}

#[cfg(test)]
fn collab_project_path_in(dir: &std::path::Path, project_name: &str) -> std::path::PathBuf {
    dir.join(format!("{project_name}.json"))
}

fn load_collab_project_from(path: &std::path::Path) -> Option<CollabProject> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).ok(),
        _ => None,
    }
}

fn save_collab_project_to(project: &CollabProject, path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(project).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

fn list_collab_projects_in(dir: &std::path::Path) -> Vec<CollabProject> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut projects = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(proj) = load_collab_project_from(&path) {
                projects.push(proj);
            }
        }
    }
    projects.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    projects
}

fn now_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ts = format_unix_timestamp(secs);
    ts.replace(' ', "T") + ":00"
}

pub fn handle_collaborate(input: &str) {
    let args = input.strip_prefix("/collaborate").unwrap_or("").trim();

    if args.is_empty() {
        collab_list_impl(std::path::Path::new(COLLABORATE_DIR));
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "start" => collab_start(rest),
        "note" => collab_note(rest),
        "list" => collab_list_impl(std::path::Path::new(COLLABORATE_DIR)),
        "view" => collab_view(rest),
        "close" => collab_close(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_collaborate_usage();
        }
    }
}

fn print_collaborate_usage() {
    println!("{DIM}  사용법:");
    println!("    /collaborate start <프로젝트명> [--reporters 기자1,기자2]  공동취재 프로젝트 생성");
    println!("    /collaborate note <프로젝트명> <내용> [--reporter 기자명]  메모 추가");
    println!("    /collaborate list                                          활성 프로젝트 목록");
    println!("    /collaborate view <프로젝트명>                             프로젝트 메모 조회");
    println!("    /collaborate close <프로젝트명>                            프로젝트 종료");
    println!("    /collaborate                                               (list와 동일){RESET}\n");
}

fn collab_start(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /collaborate start <프로젝트명> [--reporters 기자1,기자2]{RESET}\n");
        return;
    }

    let (name, reporters) = parse_collab_start_args(args);

    if name.is_empty() {
        eprintln!("{RED}  프로젝트명을 입력하세요.{RESET}\n");
        return;
    }

    let path = collab_project_path(&name);
    if let Some(existing) = load_collab_project_from(&path) {
        if existing.status == CollabStatus::Active {
            eprintln!("{RED}  이미 활성 프로젝트가 존재합니다: {name}{RESET}\n");
            return;
        }
    }

    let project = CollabProject {
        name: name.clone(),
        reporters: reporters.clone(),
        notes: Vec::new(),
        status: CollabStatus::Active,
        created_at: now_timestamp(),
    };

    save_collab_project_to(&project, &path);

    println!("{DIM}  공동취재 프로젝트 생성: {name}{RESET}");
    if !reporters.is_empty() {
        println!("{DIM}  참여 기자: {}{RESET}", reporters.join(", "));
    }
    println!();
}

fn parse_collab_start_args(args: &str) -> (String, Vec<String>) {
    let mut name = String::new();
    let mut reporters: Vec<String> = Vec::new();

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        if parts[i] == "--reporters" {
            if i + 1 < parts.len() {
                reporters = parts[i + 1]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                i += 2;
            } else {
                i += 1;
            }
        } else {
            if name.is_empty() {
                name = parts[i].to_string();
            }
            i += 1;
        }
    }

    (name, reporters)
}

fn collab_note(args: &str) {
    if args.is_empty() {
        eprintln!(
            "{RED}  사용법: /collaborate note <프로젝트명> <내용> [--reporter 기자명]{RESET}\n"
        );
        return;
    }

    let (project_name, content, reporter) = match parse_collab_note_args(args) {
        Some(v) => v,
        None => {
            eprintln!("{RED}  사용법: /collaborate note <프로젝트명> <내용> [--reporter 기자명]{RESET}\n");
            return;
        }
    };

    let path = collab_project_path(&project_name);
    let mut project = match load_collab_project_from(&path) {
        Some(p) => p,
        None => {
            eprintln!("{RED}  프로젝트를 찾을 수 없습니다: {project_name}{RESET}\n");
            return;
        }
    };

    if project.status == CollabStatus::Closed {
        eprintln!("{RED}  종료된 프로젝트입니다: {project_name}{RESET}\n");
        return;
    }

    let note = CollabNote {
        reporter: reporter.clone(),
        content: content.clone(),
        timestamp: now_timestamp(),
    };

    project.notes.push(note);
    save_collab_project_to(&project, &path);

    let reporter_display = if reporter.is_empty() {
        "익명".to_string()
    } else {
        reporter
    };
    println!(
        "{DIM}  메모 추가 ({reporter_display}): {content}{RESET}\n"
    );
}

fn parse_collab_note_args(args: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let project_name = parts[0].to_string();
    let mut content_parts: Vec<&str> = Vec::new();
    let mut reporter = String::new();

    let mut i = 1;
    while i < parts.len() {
        if parts[i] == "--reporter" {
            if i + 1 < parts.len() {
                reporter = parts[i + 1].to_string();
                i += 2;
            } else {
                i += 1;
            }
        } else {
            content_parts.push(parts[i]);
            i += 1;
        }
    }

    let content = content_parts.join(" ");
    if content.is_empty() {
        return None;
    }

    Some((project_name, content, reporter))
}

fn collab_list_impl(dir: &std::path::Path) {
    let projects = list_collab_projects_in(dir);

    let active: Vec<&CollabProject> = projects
        .iter()
        .filter(|p| p.status == CollabStatus::Active)
        .collect();

    if active.is_empty() {
        println!("{DIM}  활성 공동취재 프로젝트가 없습니다.{RESET}\n");
        return;
    }

    println!("{DIM}  ── 활성 공동취재 프로젝트 ──{RESET}");
    for (i, proj) in active.iter().enumerate() {
        let reporters_str = if proj.reporters.is_empty() {
            String::new()
        } else {
            format!(" [{}]", proj.reporters.join(", "))
        };
        println!(
            "{DIM}  {}. {}{} — 메모 {}건{RESET}",
            i + 1,
            proj.name,
            reporters_str,
            proj.notes.len()
        );
    }
    println!();
}

fn collab_view(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /collaborate view <프로젝트명>{RESET}\n");
        return;
    }

    let project_name = args.split_whitespace().next().unwrap_or("");
    let path = collab_project_path(project_name);
    let project = match load_collab_project_from(&path) {
        Some(p) => p,
        None => {
            eprintln!("{RED}  프로젝트를 찾을 수 없습니다: {project_name}{RESET}\n");
            return;
        }
    };

    let status_str = match project.status {
        CollabStatus::Active => "활성",
        CollabStatus::Closed => "종료",
    };
    println!(
        "{DIM}  ── {} ({}) ──{RESET}",
        project.name, status_str
    );
    if !project.reporters.is_empty() {
        println!(
            "{DIM}  참여 기자: {}{RESET}",
            project.reporters.join(", ")
        );
    }
    println!(
        "{DIM}  생성: {}{RESET}",
        project.created_at
    );

    if project.notes.is_empty() {
        println!("{DIM}  (메모 없음){RESET}");
    } else {
        println!("{DIM}  ── 메모 ({}) ──{RESET}", project.notes.len());
        for (i, note) in project.notes.iter().enumerate() {
            let reporter_str = if note.reporter.is_empty() {
                "익명"
            } else {
                &note.reporter
            };
            println!(
                "{DIM}  {}. [{reporter_str}] {} — {}{RESET}",
                i + 1,
                note.content,
                note.timestamp
            );
        }
    }
    println!();
}

fn collab_close(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /collaborate close <프로젝트명>{RESET}\n");
        return;
    }

    let project_name = args.split_whitespace().next().unwrap_or("");
    let path = collab_project_path(project_name);
    let mut project = match load_collab_project_from(&path) {
        Some(p) => p,
        None => {
            eprintln!("{RED}  프로젝트를 찾을 수 없습니다: {project_name}{RESET}\n");
            return;
        }
    };

    if project.status == CollabStatus::Closed {
        println!("{DIM}  이미 종료된 프로젝트입니다: {project_name}{RESET}\n");
        return;
    }

    project.status = CollabStatus::Closed;
    save_collab_project_to(&project, &path);
    println!(
        "{DIM}  프로젝트 종료: {project_name} (메모 {}건 보존){RESET}\n",
        project.notes.len()
    );
}

// ── /coverage ─────────────────────────────────────────────────────────────

const COVERAGE_FILE: &str = ".journalist/coverage.json";

/// A single coverage claim (속보 취재 영역 선점).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct CoverageClaim {
    topic: String,
    reporter: String,
    /// Optional expiry time in "HH:MM" format.
    until: Option<String>,
    active: bool,
    created_at: String,
}

fn coverage_path() -> std::path::PathBuf {
    std::path::PathBuf::from(COVERAGE_FILE)
}

fn load_coverage_from(path: &std::path::Path) -> Vec<CoverageClaim> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_coverage_to(claims: &[CoverageClaim], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(claims).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

/// Check if a claim has expired based on its `until` time and current HH:MM.
fn is_claim_expired(claim: &CoverageClaim, now_hhmm: &str) -> bool {
    match &claim.until {
        Some(until) => now_hhmm >= until.as_str(),
        None => false,
    }
}

/// Get current time as "HH:MM" (UTC).
fn current_hhmm() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    format!("{h:02}:{m:02}")
}

/// Mark expired claims as inactive (mutates in place, returns count of newly expired).
fn expire_claims(claims: &mut [CoverageClaim], now_hhmm: &str) -> usize {
    let mut count = 0;
    for claim in claims.iter_mut() {
        if claim.active && is_claim_expired(claim, now_hhmm) {
            claim.active = false;
            count += 1;
        }
    }
    count
}

/// Parse claim args: `<주제> [--reporter 기자명] [--until HH:MM]`
fn parse_coverage_claim_args(args: &str) -> (String, String, Option<String>) {
    let mut topic_parts = Vec::new();
    let mut reporter = String::new();
    let mut until: Option<String> = None;

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "--reporter" {
            if i + 1 < tokens.len() {
                reporter = tokens[i + 1].to_string();
                i += 2;
            } else {
                i += 1;
            }
        } else if tokens[i] == "--until" {
            if i + 1 < tokens.len() {
                until = Some(tokens[i + 1].to_string());
                i += 2;
            } else {
                i += 1;
            }
        } else {
            topic_parts.push(tokens[i]);
            i += 1;
        }
    }

    let topic = topic_parts.join(" ");
    (topic, reporter, until)
}

/// Handle `/coverage` command with subcommands: claim, list, release, check.
pub fn handle_coverage(input: &str) {
    let args = input.strip_prefix("/coverage").unwrap_or("").trim();

    if args.is_empty() {
        handle_coverage_list();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "claim" => handle_coverage_claim(rest),
        "list" => handle_coverage_list(),
        "release" => handle_coverage_release(rest),
        "check" => handle_coverage_check(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_coverage_usage();
        }
    }
}

fn print_coverage_usage() {
    println!("{DIM}  사용법:");
    println!("    /coverage claim <주제> [--reporter 기자명] [--until HH:MM]  취재 영역 선점");
    println!("    /coverage list                                              현재 취재 목록");
    println!("    /coverage release <번호>                                    취재 영역 해제");
    println!("    /coverage check <키워드>                                    중복 취재 확인");
    println!("    /coverage                                                   (list와 동일){RESET}\n");
}

fn handle_coverage_claim(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /coverage claim <주제> [--reporter 기자명] [--until HH:MM]{RESET}\n");
        return;
    }

    let (topic, reporter, until) = parse_coverage_claim_args(args);

    if topic.is_empty() {
        eprintln!("{RED}  주제를 지정하세요: /coverage claim <주제>{RESET}\n");
        return;
    }

    // Validate until time format if provided
    if let Some(ref t) = until {
        if !is_valid_time(t) {
            eprintln!("{RED}  시간 형식이 올바르지 않습니다: {t}{RESET}");
            eprintln!("{DIM}  예: 18:00{RESET}\n");
            return;
        }
    }

    let path = coverage_path();
    let mut claims = load_coverage_from(&path);

    // Auto-expire old claims
    let now = current_hhmm();
    expire_claims(&mut claims, &now);

    let reporter_name = if reporter.is_empty() {
        "(미지정)".to_string()
    } else {
        reporter.clone()
    };

    claims.push(CoverageClaim {
        topic: topic.clone(),
        reporter: reporter_name.clone(),
        until: until.clone(),
        active: true,
        created_at: now_timestamp(),
    });

    save_coverage_to(&claims, &path);

    let until_text = until
        .as_deref()
        .map(|t| format!(" (만료: {t})"))
        .unwrap_or_default();
    println!(
        "{GREEN}  🚨 취재 영역 선점: {topic} — {reporter_name}{until_text}{RESET}\n"
    );
}

fn handle_coverage_list() {
    let path = coverage_path();
    let mut claims = load_coverage_from(&path);

    // Auto-expire
    let now = current_hhmm();
    let expired_count = expire_claims(&mut claims, &now);
    if expired_count > 0 {
        save_coverage_to(&claims, &path);
    }

    let active: Vec<(usize, &CoverageClaim)> = claims
        .iter()
        .enumerate()
        .filter(|(_, c)| c.active)
        .collect();

    if active.is_empty() && claims.iter().all(|c| !c.active) && claims.is_empty() {
        println!("{DIM}  등록된 취재 영역이 없습니다.{RESET}\n");
        return;
    }

    println!("{BOLD}  🚨 속보 취재 현황{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");

    if active.is_empty() {
        println!("{DIM}  현재 활성 취재 영역이 없습니다.{RESET}");
    } else {
        for (idx, claim) in &active {
            let num = idx + 1;
            let until_text = claim
                .until
                .as_deref()
                .map(|t| {
                    // Color-code based on proximity to expiry
                    let remaining = time_diff_minutes(t, &now);
                    match remaining {
                        Some(m) if m < 0 => format!(" {RED}[만료: {t} — 시간 초과]{RESET}"),
                        Some(m) if m <= 30 => format!(" {YELLOW}[만료: {t} — {m}분 남음]{RESET}"),
                        Some(m) => format!(" {GREEN}[만료: {t} — {m}분 남음]{RESET}"),
                        None => format!(" [만료: {t}]"),
                    }
                })
                .unwrap_or_default();

            println!(
                "  {GREEN}#{num}{RESET} {BOLD}{}{RESET} — {}{until_text}",
                claim.topic, claim.reporter
            );
        }
    }

    // Show recently expired
    let inactive: Vec<(usize, &CoverageClaim)> = claims
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.active)
        .collect();
    if !inactive.is_empty() {
        println!("{DIM}  ── 만료/해제된 항목 ──{RESET}");
        for (idx, claim) in &inactive {
            let num = idx + 1;
            println!("{DIM}  #{num} {} — {}{RESET}", claim.topic, claim.reporter);
        }
    }

    println!();
}

fn handle_coverage_release(num_str: &str) {
    if num_str.is_empty() {
        eprintln!("{RED}  번호를 지정하세요: /coverage release <번호>{RESET}\n");
        return;
    }

    let num: usize = match num_str.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요: {num_str}{RESET}\n");
            return;
        }
    };

    let path = coverage_path();
    let mut claims = load_coverage_from(&path);
    let idx = num - 1;

    if idx >= claims.len() {
        eprintln!("{RED}  #{num}번 취재 영역을 찾을 수 없습니다.{RESET}\n");
        return;
    }

    if !claims[idx].active {
        println!("{DIM}  #{num}번은 이미 비활성 상태입니다.{RESET}\n");
        return;
    }

    claims[idx].active = false;
    let topic = claims[idx].topic.clone();
    save_coverage_to(&claims, &path);
    println!("{GREEN}  ✅ 취재 영역 해제: #{num} {topic}{RESET}\n");
}

fn handle_coverage_check(keyword: &str) {
    if keyword.is_empty() {
        eprintln!("{RED}  키워드를 지정하세요: /coverage check <키워드>{RESET}\n");
        return;
    }

    let path = coverage_path();
    let mut claims = load_coverage_from(&path);

    // Auto-expire
    let now = current_hhmm();
    let expired_count = expire_claims(&mut claims, &now);
    if expired_count > 0 {
        save_coverage_to(&claims, &path);
    }

    let keyword_lower = keyword.to_lowercase();
    let matches: Vec<(usize, &CoverageClaim)> = claims
        .iter()
        .enumerate()
        .filter(|(_, c)| c.active && c.topic.to_lowercase().contains(&keyword_lower))
        .collect();

    if matches.is_empty() {
        println!(
            "{GREEN}  ✅ \"{keyword}\" 관련 진행 중인 취재가 없습니다. 취재 가능합니다.{RESET}\n"
        );
    } else {
        println!(
            "{YELLOW}  ⚠️  \"{keyword}\" 관련 취재가 이미 진행 중입니다:{RESET}"
        );
        for (idx, claim) in &matches {
            let num = idx + 1;
            let until_text = claim
                .until
                .as_deref()
                .map(|t| format!(" [만료: {t}]"))
                .unwrap_or_default();
            println!(
                "  {YELLOW}  #{num} {} — {}{until_text}{RESET}",
                claim.topic, claim.reporter
            );
        }
        println!();
    }
}

/// Calculate difference in minutes between two HH:MM times. Returns None if parsing fails.
fn time_diff_minutes(target: &str, now: &str) -> Option<i32> {
    let parse_hhmm = |s: &str| -> Option<i32> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return None;
        }
        let h: i32 = parts[0].parse().ok()?;
        let m: i32 = parts[1].parse().ok()?;
        Some(h * 60 + m)
    };
    let target_mins = parse_hhmm(target)?;
    let now_mins = parse_hhmm(now)?;
    Some(target_mins - now_mins)
}

// ── /dashboard ──────────────────────────────────────────────────────────

/// Handle `/dashboard` — newsroom status board showing active items across all systems.
/// No AI call; purely local JSON reads.
pub fn handle_dashboard() {
    handle_dashboard_impl(
        &deadlines_path(),
        &embargoes_path(),
        &desk_path(),
        &followups_path(),
        std::path::Path::new(COLLABORATE_DIR),
        &coverage_path(),
    );
}

fn handle_dashboard_impl(
    deadlines_path: &std::path::Path,
    embargoes_path: &std::path::Path,
    desk_path: &std::path::Path,
    followups_path: &std::path::Path,
    collab_dir: &std::path::Path,
    coverage_path: &std::path::Path,
) {
    println!("\n{BOLD}  ══════════════════════════════════════{RESET}");
    println!("{BOLD}   📋 뉴스룸 현황판{RESET}");
    println!("{BOLD}  ══════════════════════════════════════{RESET}\n");

    let mut total_items = 0u32;

    // 1. Deadlines (마감 임박)
    let deadlines = load_deadlines_from(deadlines_path);
    if deadlines.is_empty() {
        println!("{DIM}  ⏰ 마감: 없음{RESET}");
    } else {
        println!("{YELLOW}  ⏰ 마감 ({} 건){RESET}", deadlines.len());
        for dl in &deadlines {
            println!("     • {BOLD}{}{RESET}  → {}", dl.title, dl.datetime);
        }
        total_items += deadlines.len() as u32;
    }
    println!();

    // 2. Embargoes (활성 엠바고)
    let embargoes = load_embargoes_from(embargoes_path);
    if embargoes.is_empty() {
        println!("{DIM}  🔒 엠바고: 없음{RESET}");
    } else {
        println!("{RED}  🔒 엠바고 ({} 건){RESET}", embargoes.len());
        for em in &embargoes {
            println!("     • {BOLD}{}{RESET}  → 해제: {}", em.title, em.release_at);
        }
        total_items += embargoes.len() as u32;
    }
    println!();

    // 3. Desk assignments (대기 중인 데스크 지시)
    let assignments = load_desk_from(desk_path);
    let pending: Vec<&DeskAssignment> = assignments
        .iter()
        .filter(|a| a.status == DeskStatus::Pending)
        .collect();
    if pending.is_empty() {
        println!("{DIM}  📝 데스크 지시: 대기 없음{RESET}");
    } else {
        println!("{CYAN}  📝 데스크 지시 — 대기 ({} 건){RESET}", pending.len());
        for a in &pending {
            let dl_info = a
                .deadline
                .as_deref()
                .map(|d| format!(" [마감 {d}]"))
                .unwrap_or_default();
            let kind = if a.is_pitch { "제안" } else { "지시" };
            println!(
                "     • {BOLD}{}{RESET} → {} ({kind}){dl_info}",
                a.reporter, a.content
            );
        }
        total_items += pending.len() as u32;
    }
    println!();

    // 4. Follow-ups due soon (후속 보도 임박)
    let followups = load_followups_from(followups_path);
    let active_followups: Vec<&Followup> = followups.iter().filter(|f| !f.done).collect();
    if active_followups.is_empty() {
        println!("{DIM}  🔄 후속 보도: 없음{RESET}");
    } else {
        println!(
            "{MAGENTA}  🔄 후속 보도 ({} 건){RESET}",
            active_followups.len()
        );
        for f in &active_followups {
            let due_info = f
                .due
                .as_deref()
                .map(|d| format!(" [기한 {d}]"))
                .unwrap_or_default();
            println!("     • {BOLD}{}{RESET}{due_info}", f.topic);
        }
        total_items += active_followups.len() as u32;
    }
    println!();

    // 5. Collaborate projects (활성 공동취재)
    let collab_projects = list_collab_projects_in(collab_dir);
    let active_collabs: Vec<&CollabProject> = collab_projects
        .iter()
        .filter(|p| p.status == CollabStatus::Active)
        .collect();
    if active_collabs.is_empty() {
        println!("{DIM}  🤝 공동취재: 없음{RESET}");
    } else {
        println!("{GREEN}  🤝 공동취재 ({} 건){RESET}", active_collabs.len());
        for p in &active_collabs {
            let reporters = p.reporters.join(", ");
            println!(
                "     • {BOLD}{}{RESET}  참여: {reporters} (메모 {} 건)",
                p.name,
                p.notes.len()
            );
        }
        total_items += active_collabs.len() as u32;
    }
    println!();

    // 6. Coverage claims (취재 선점 현황)
    let claims = load_coverage_from(coverage_path);
    let active_claims: Vec<&CoverageClaim> = claims.iter().filter(|c| c.active).collect();
    if active_claims.is_empty() {
        println!("{DIM}  🏷️  취재 선점: 없음{RESET}");
    } else {
        println!("{BOLD_CYAN}  🏷️  취재 선점 ({} 건){RESET}", active_claims.len());
        for c in &active_claims {
            let until_info = c
                .until
                .as_deref()
                .map(|u| format!(" (~{u})"))
                .unwrap_or_default();
            println!(
                "     • {BOLD}{}{RESET} — {}{until_info}",
                c.topic, c.reporter
            );
        }
        total_items += active_claims.len() as u32;
    }

    println!();
    println!("{BOLD}  ──────────────────────────────────────{RESET}");
    println!("{BOLD}   활성 항목 합계: {total_items} 건{RESET}");
    println!("{BOLD}  ══════════════════════════════════════{RESET}\n");
}

// ── /calendar — 취재 일정 관리 ──────────────────────────────────────────

const CALENDAR_FILE: &str = ".journalist/calendar.json";

/// A single calendar event entry.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
struct CalendarEvent {
    /// Unique numeric ID (1-based, assigned at creation)
    id: u32,
    /// Date string "YYYY-MM-DD"
    date: String,
    /// Time string "HH:MM"
    time: String,
    /// Event description
    description: String,
    /// Whether the event is completed
    #[serde(default)]
    done: bool,
}

fn calendar_path() -> std::path::PathBuf {
    std::path::PathBuf::from(CALENDAR_FILE)
}

fn load_calendar_from(path: &std::path::Path) -> Vec<CalendarEvent> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_calendar_to(events: &[CalendarEvent], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(events).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

/// Compute next available ID for a calendar event list.
fn next_calendar_id(events: &[CalendarEvent]) -> u32 {
    events.iter().map(|e| e.id).max().unwrap_or(0) + 1
}

/// Parse a date string. Accepts "YYYY-MM-DD".
/// Returns Some("YYYY-MM-DD") if valid, None otherwise.
fn parse_calendar_date(input: &str) -> Option<String> {
    let parts: Vec<&str> = input.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    if parts[0].parse::<u32>().is_err()
        || parts[1].parse::<u32>().is_err()
        || parts[2].parse::<u32>().is_err()
    {
        return None;
    }
    let month: u32 = parts[1].parse().unwrap();
    let day: u32 = parts[2].parse().unwrap();
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(input.to_string())
}

/// Parse a time string. Accepts "HH:MM".
fn parse_calendar_time(input: &str) -> Option<String> {
    let parts: Vec<&str> = input.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hour: u32 = parts[0].parse().ok()?;
    let minute: u32 = parts[1].parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{:02}:{:02}", hour, minute))
}

/// Determine color coding index for a date relative to today.
/// Returns 0=today(red), 1=tomorrow(yellow), 2=past(dim), 3=future(none).
fn date_color_index(date: &str, today: &str) -> u8 {
    if date == today {
        0
    } else if let Some(tomorrow) = next_day(today) {
        if date == tomorrow {
            1
        } else if date < today {
            2
        } else {
            3
        }
    } else if date < today {
        2
    } else {
        3
    }
}

/// Compute the next day from a "YYYY-MM-DD" string. Simple implementation.
fn next_day(date: &str) -> Option<String> {
    let parts: Vec<u32> = date.split('-').filter_map(|s| s.parse().ok()).collect();
    if parts.len() != 3 {
        return None;
    }
    let (year, month, day) = (parts[0], parts[1], parts[2]);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year as u64) {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if day < days_in_month {
        Some(format!("{:04}-{:02}-{:02}", year, month, day + 1))
    } else if month < 12 {
        Some(format!("{:04}-{:02}-01", year, month + 1))
    } else {
        Some(format!("{:04}-01-01", year + 1))
    }
}

/// Get the day-of-week (0=Mon .. 6=Sun) for a "YYYY-MM-DD" date using Zeller-like calculation.
fn day_of_week(date: &str) -> Option<u32> {
    let epoch = datetime_to_epoch(&format!("{date}T00:00:00"))?;
    // 1970-01-01 was a Thursday (3 in 0=Mon..6=Sun)
    let days = epoch / 86400;
    Some(((days + 3) % 7) as u32) // 0=Mon
}

/// Get the Monday of the week containing the given date.
fn week_start(date: &str) -> Option<String> {
    let dow = day_of_week(date)?;
    let epoch = datetime_to_epoch(&format!("{date}T00:00:00"))?;
    let monday_epoch = epoch - (dow as u64) * 86400;
    Some(format_date_from_epoch(monday_epoch))
}

/// Get the Sunday of the week containing the given date.
fn week_end(date: &str) -> Option<String> {
    let dow = day_of_week(date)?;
    let epoch = datetime_to_epoch(&format!("{date}T00:00:00"))?;
    let sunday_epoch = epoch + ((6 - dow) as u64) * 86400;
    Some(format_date_from_epoch(sunday_epoch))
}

/// Handle `/calendar` command with subcommands: add, list, today, week, done, remove.
pub fn handle_calendar(input: &str) {
    let args = input.strip_prefix("/calendar").unwrap_or("").trim();

    if args.is_empty() {
        // Default to list
        handle_calendar_list();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "add" => handle_calendar_add(rest),
        "list" => handle_calendar_list(),
        "today" => handle_calendar_today(),
        "week" => handle_calendar_week(),
        "done" => handle_calendar_done(rest),
        "remove" => handle_calendar_remove(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_calendar_usage();
        }
    }
}

fn print_calendar_usage() {
    println!("\n{BOLD}  📅 /calendar — 취재 일정 관리{RESET}\n");
    println!("    /calendar add <날짜> <시각> <설명>  일정 등록");
    println!("    /calendar list                     전체 목록 (날짜순)");
    println!("    /calendar today                    오늘 일정");
    println!("    /calendar week                     이번 주 일정");
    println!("    /calendar done <번호>              완료 처리");
    println!("    /calendar remove <번호>            삭제");
    println!("    /calendar                          (list와 동일){RESET}\n");
}

fn handle_calendar_add(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /calendar add <날짜> <시각> <설명>{RESET}");
        eprintln!("{DIM}  예: /calendar add 2026-03-25 14:00 기자간담회 — 삼성전자 실적{RESET}\n");
        return;
    }

    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    if parts.len() < 3 {
        eprintln!("{RED}  날짜, 시각, 설명을 모두 입력하세요.{RESET}");
        eprintln!("{DIM}  예: /calendar add 2026-03-25 14:00 기자간담회{RESET}\n");
        return;
    }

    let date_str = parts[0];
    let time_str = parts[1];
    let description = parts[2].trim().to_string();

    let date = match parse_calendar_date(date_str) {
        Some(d) => d,
        None => {
            eprintln!("{RED}  날짜 형식 오류: {date_str}{RESET}");
            eprintln!("{DIM}  예: 2026-03-25{RESET}\n");
            return;
        }
    };

    let time = match parse_calendar_time(time_str) {
        Some(t) => t,
        None => {
            eprintln!("{RED}  시각 형식 오류: {time_str}{RESET}");
            eprintln!("{DIM}  예: 14:00{RESET}\n");
            return;
        }
    };

    if description.is_empty() {
        eprintln!("{RED}  설명을 입력하세요.{RESET}\n");
        return;
    }

    let path = calendar_path();
    let mut events = load_calendar_from(&path);
    let id = next_calendar_id(&events);

    events.push(CalendarEvent {
        id,
        date: date.clone(),
        time: time.clone(),
        description: description.clone(),
        done: false,
    });

    save_calendar_to(&events, &path);
    println!("{GREEN}  📅 일정 등록 (#{id}): {date} {time} — {description}{RESET}\n");
}

fn handle_calendar_list() {
    let path = calendar_path();
    let mut events = load_calendar_from(&path);

    if events.is_empty() {
        println!("{DIM}  등록된 일정이 없습니다.{RESET}\n");
        return;
    }

    // Sort by date then time
    events.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));

    let today = today_date_string();
    println!("\n{BOLD}  📅 전체 일정 ({} 건){RESET}\n", events.len());
    print_calendar_events(&events, &today);
}

fn handle_calendar_today() {
    let path = calendar_path();
    let events = load_calendar_from(&path);
    let today = today_date_string();

    let mut today_events: Vec<&CalendarEvent> = events.iter().filter(|e| e.date == today).collect();
    today_events.sort_by(|a, b| a.time.cmp(&b.time));

    if today_events.is_empty() {
        println!("{DIM}  오늘({today}) 일정이 없습니다.{RESET}\n");
        return;
    }

    println!(
        "\n{BOLD}  📅 오늘 일정 — {today} ({} 건){RESET}\n",
        today_events.len()
    );
    for event in &today_events {
        let done_mark = if event.done { "✅" } else { "⬜" };
        println!(
            "    {RED}#{:<3}{RESET} {done_mark} {RED}{}{RESET}  {}{}",
            event.id,
            event.time,
            event.description,
            if event.done {
                format!(" {DIM}(완료){RESET}")
            } else {
                String::new()
            },
        );
    }
    println!();
}

fn handle_calendar_week() {
    let path = calendar_path();
    let events = load_calendar_from(&path);
    let today = today_date_string();

    let mon = match week_start(&today) {
        Some(d) => d,
        None => {
            eprintln!("{RED}  날짜 계산 오류{RESET}\n");
            return;
        }
    };
    let sun = match week_end(&today) {
        Some(d) => d,
        None => {
            eprintln!("{RED}  날짜 계산 오류{RESET}\n");
            return;
        }
    };

    let mut week_events: Vec<&CalendarEvent> = events
        .iter()
        .filter(|e| e.date >= mon && e.date <= sun)
        .collect();
    week_events.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));

    if week_events.is_empty() {
        println!("{DIM}  이번 주({mon} ~ {sun}) 일정이 없습니다.{RESET}\n");
        return;
    }

    println!(
        "\n{BOLD}  📅 이번 주 일정 — {mon} ~ {sun} ({} 건){RESET}\n",
        week_events.len()
    );
    let owned: Vec<CalendarEvent> = week_events.into_iter().cloned().collect();
    print_calendar_events(&owned, &today);
}

fn handle_calendar_done(num_str: &str) {
    let id: u32 = match num_str.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  번호를 입력하세요: /calendar done <번호>{RESET}\n");
            return;
        }
    };

    let path = calendar_path();
    let mut events = load_calendar_from(&path);

    if let Some(event) = events.iter_mut().find(|e| e.id == id) {
        event.done = true;
        let desc = event.description.clone();
        save_calendar_to(&events, &path);
        println!("{GREEN}  ✅ 완료 처리: #{id} — {desc}{RESET}\n");
    } else {
        eprintln!("{RED}  #{id} 일정을 찾을 수 없습니다.{RESET}\n");
    }
}

fn handle_calendar_remove(num_str: &str) {
    let id: u32 = match num_str.trim().parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  번호를 입력하세요: /calendar remove <번호>{RESET}\n");
            return;
        }
    };

    let path = calendar_path();
    let mut events = load_calendar_from(&path);
    let original_len = events.len();
    events.retain(|e| e.id != id);

    if events.len() < original_len {
        save_calendar_to(&events, &path);
        println!("{GREEN}  🗑️ 일정 삭제: #{id}{RESET}\n");
    } else {
        eprintln!("{RED}  #{id} 일정을 찾을 수 없습니다.{RESET}\n");
    }
}

/// Print a list of calendar events with color coding.
fn print_calendar_events(events: &[CalendarEvent], today: &str) {
    for event in events {
        let ci = date_color_index(&event.date, today);
        let done_mark = if event.done { "✅" } else { "⬜" };
        let done_suffix = if event.done {
            format!(" {DIM}(완료){RESET}")
        } else {
            String::new()
        };
        match ci {
            0 => println!(
                "    {RED}#{:<3}{RESET} {done_mark} {RED}{} {}{RESET}  {}{done_suffix}",
                event.id, event.date, event.time, event.description,
            ),
            1 => println!(
                "    {YELLOW}#{:<3}{RESET} {done_mark} {YELLOW}{} {}{RESET}  {}{done_suffix}",
                event.id, event.date, event.time, event.description,
            ),
            2 => println!(
                "    {DIM}#{:<3}{RESET} {done_mark} {DIM}{} {}{RESET}  {}{done_suffix}",
                event.id, event.date, event.time, event.description,
            ),
            _ => println!(
                "    #{:<3} {done_mark} {} {}  {}{done_suffix}",
                event.id, event.date, event.time, event.description,
            ),
        }
    }
    println!();
}

// ── /performance — 기사 퍼포먼스 추적 ──────────────────────────────────

const PERFORMANCE_FILE: &str = ".journalist/performance.json";

fn load_performance_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_performance_to(data: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = std::fs::write(path, json);
    }
}

/// Parse --views N, --comments N, --shares N flags from argument string.
/// Returns (title, views, comments, shares).
fn parse_performance_args(args: &str) -> (String, Option<u64>, Option<u64>, Option<u64>) {
    let mut title_parts: Vec<&str> = Vec::new();
    let mut views: Option<u64> = None;
    let mut comments: Option<u64> = None;
    let mut shares: Option<u64> = None;

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--views" => {
                if i + 1 < parts.len() {
                    views = parts[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--comments" => {
                if i + 1 < parts.len() {
                    comments = parts[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--shares" => {
                if i + 1 < parts.len() {
                    shares = parts[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                title_parts.push(parts[i]);
                i += 1;
            }
        }
    }

    (title_parts.join(" "), views, comments, shares)
}

fn performance_add(args: &str, perf_path: &std::path::Path) {
    let (title, views, comments, shares) = parse_performance_args(args);

    if title.is_empty() {
        println!("{DIM}  사용법: /performance add <제목> --views N --comments N --shares N{RESET}\n");
        return;
    }

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);
    let date = &timestamp[..10];

    let mut data = load_performance_from(perf_path);
    let id = data.len() + 1;

    let entry = serde_json::json!({
        "id": id,
        "title": title,
        "date": date,
        "views": views.unwrap_or(0),
        "comments": comments.unwrap_or(0),
        "shares": shares.unwrap_or(0),
    });

    data.push(entry);
    save_performance_to(&data, perf_path);

    println!(
        "{DIM}  #{id} 성과 등록: \"{title}\" [{date}] — 조회 {}, 댓글 {}, 공유 {}{RESET}\n",
        views.unwrap_or(0),
        comments.unwrap_or(0),
        shares.unwrap_or(0),
    );
}

fn performance_update(args: &str, perf_path: &std::path::Path) {
    let (id_str, views, comments, shares) = parse_performance_args(args);

    let id: usize = match id_str.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            println!("{DIM}  사용법: /performance update <번호> --views N --comments N --shares N{RESET}\n");
            return;
        }
    };

    let mut data = load_performance_from(perf_path);
    if id > data.len() {
        eprintln!("{RED}  #{id}번 항목이 없습니다. (총 {}건){RESET}\n", data.len());
        return;
    }

    {
        let entry = &mut data[id - 1];
        if let Some(v) = views {
            entry["views"] = serde_json::json!(v);
        }
        if let Some(c) = comments {
            entry["comments"] = serde_json::json!(c);
        }
        if let Some(s) = shares {
            entry["shares"] = serde_json::json!(s);
        }
    }

    save_performance_to(&data, perf_path);

    let entry = &data[id - 1];
    let title = entry["title"].as_str().unwrap_or("?");
    println!(
        "{DIM}  #{id} 성과 업데이트: \"{title}\" — 조회 {}, 댓글 {}, 공유 {}{RESET}\n",
        entry["views"], entry["comments"], entry["shares"],
    );
}

fn performance_list(perf_path: &std::path::Path) {
    let data = load_performance_from(perf_path);
    if data.is_empty() {
        println!("{DIM}  등록된 성과 데이터가 없습니다.");
        println!("  /performance add <제목> --views N 으로 등록하세요.{RESET}\n");
        return;
    }

    // Sort by total engagement (views + comments + shares) descending
    let mut sorted: Vec<(usize, &serde_json::Value)> = data.iter().enumerate().collect();
    sorted.sort_by(|a, b| {
        let total_a = a.1["views"].as_u64().unwrap_or(0)
            + a.1["comments"].as_u64().unwrap_or(0)
            + a.1["shares"].as_u64().unwrap_or(0);
        let total_b = b.1["views"].as_u64().unwrap_or(0)
            + b.1["comments"].as_u64().unwrap_or(0)
            + b.1["shares"].as_u64().unwrap_or(0);
        total_b.cmp(&total_a)
    });

    println!("{DIM}  ── 기사 퍼포먼스 (성과순) ──{RESET}");
    for (idx, entry) in &sorted {
        let id = idx + 1;
        let title = entry["title"].as_str().unwrap_or("?");
        let date = entry["date"].as_str().unwrap_or("?");
        let views = entry["views"].as_u64().unwrap_or(0);
        let comments = entry["comments"].as_u64().unwrap_or(0);
        let shares = entry["shares"].as_u64().unwrap_or(0);
        let total = views + comments + shares;
        println!(
            "{DIM}  #{id} [{date}] \"{title}\" — 조회 {views} / 댓글 {comments} / 공유 {shares} (합계 {total}){RESET}"
        );
    }
    println!();
}

fn performance_top(perf_path: &std::path::Path) {
    let data = load_performance_from(perf_path);
    if data.is_empty() {
        println!("{DIM}  등록된 성과 데이터가 없습니다.{RESET}\n");
        return;
    }

    let best = data
        .iter()
        .enumerate()
        .max_by_key(|(_, e)| {
            e["views"].as_u64().unwrap_or(0)
                + e["comments"].as_u64().unwrap_or(0)
                + e["shares"].as_u64().unwrap_or(0)
        })
        .unwrap();

    let id = best.0 + 1;
    let entry = best.1;
    let title = entry["title"].as_str().unwrap_or("?");
    let date = entry["date"].as_str().unwrap_or("?");
    let views = entry["views"].as_u64().unwrap_or(0);
    let comments = entry["comments"].as_u64().unwrap_or(0);
    let shares = entry["shares"].as_u64().unwrap_or(0);
    let total = views + comments + shares;

    println!("{DIM}  🏆 베스트 기사: #{id} \"{title}\" [{date}]{RESET}");
    println!("{DIM}     조회 {views} / 댓글 {comments} / 공유 {shares} (합계 {total}){RESET}\n");
}

pub fn performance_report_prompt(data: &[serde_json::Value]) -> String {
    let mut lines = Vec::new();
    lines.push("아래는 기자의 기사별 퍼포먼스 데이터입니다. 주간/월간 퍼포먼스 리포트를 작성해 주세요.\n".to_string());
    lines.push("데이터:".to_string());
    for (i, entry) in data.iter().enumerate() {
        let id = i + 1;
        let title = entry["title"].as_str().unwrap_or("?");
        let date = entry["date"].as_str().unwrap_or("?");
        let views = entry["views"].as_u64().unwrap_or(0);
        let comments = entry["comments"].as_u64().unwrap_or(0);
        let shares = entry["shares"].as_u64().unwrap_or(0);
        lines.push(format!(
            "  #{id} [{date}] \"{title}\" — 조회 {views}, 댓글 {comments}, 공유 {shares}"
        ));
    }
    lines.push(String::new());
    lines.push("리포트에 포함할 내용:".to_string());
    lines.push("1. 전체 성과 요약 (총 조회수, 평균 인게이지먼트)".to_string());
    lines.push("2. 베스트 기사와 그 성공 요인 분석".to_string());
    lines.push("3. 기사 유형별 성과 패턴".to_string());
    lines.push("4. 개선 제안 (다음 기사 전략)".to_string());
    lines.join("\n")
}

/// Handle the /performance command: article performance tracking.
pub async fn handle_performance(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/performance").unwrap_or("").trim();
    let perf_path = std::path::Path::new(PERFORMANCE_FILE);

    match args.split_whitespace().next().unwrap_or("list") {
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            performance_add(rest, perf_path);
        }
        "update" => {
            let rest = args.strip_prefix("update").unwrap_or("").trim();
            performance_update(rest, perf_path);
        }
        "list" => {
            performance_list(perf_path);
        }
        "top" => {
            performance_top(perf_path);
        }
        "report" => {
            let data = load_performance_from(perf_path);
            if data.is_empty() {
                println!("{DIM}  등록된 성과 데이터가 없습니다.{RESET}\n");
                return;
            }
            let prompt = performance_report_prompt(&data);
            auto_compact_if_needed(agent);
            run_prompt(agent, &prompt, session_total, model).await;
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /performance [add|update|list|top|report]{RESET}");
            println!("{DIM}    /performance add <제목> --views N --comments N --shares N{RESET}");
            println!("{DIM}    /performance update <번호> --views N{RESET}");
            println!("{DIM}    /performance list{RESET}");
            println!("{DIM}    /performance top{RESET}");
            println!("{DIM}    /performance report{RESET}\n");
        }
    }
}

// ── /autopitch — AI 기사 아이디어 제안 ──────────────────────────────────

const PITCHES_DIR: &str = ".journalist/pitches";

/// Parse `/autopitch` arguments, extracting `--beat <분야>` if present.
/// Returns (beat, remaining_args).
pub fn parse_autopitch_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    if args.is_empty() {
        return (None, String::new());
    }
    let mut beat: Option<String> = None;
    let mut remaining: Vec<String> = Vec::new();
    let mut iter = args.split_whitespace().peekable();
    while let Some(token) = iter.next() {
        if token == "--beat" {
            if let Some(val) = iter.next() {
                beat = Some(val.to_string());
            }
        } else {
            remaining.push(token.to_string());
        }
    }
    (beat, remaining.join(" "))
}

/// Collect journalist data from `.journalist/` subdirectories for pitch generation.
/// Returns a summary string of available data.
pub fn collect_journalist_data() -> String {
    let mut sections = Vec::new();

    // Recent research
    let research_dir = std::path::Path::new(".journalist/research");
    if research_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(research_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
                .collect();
            files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
            files.truncate(10);
            if !files.is_empty() {
                let mut s = String::from("## 최근 리서치\n");
                for f in &files {
                    if let Ok(content) = std::fs::read_to_string(f.path()) {
                        let preview: String = content.chars().take(500).collect();
                        s.push_str(&format!(
                            "\n### {}\n{}\n",
                            f.file_name().to_string_lossy(),
                            preview
                        ));
                    }
                }
                sections.push(s);
            }
        }
    }

    // Recent clips
    let clips_dir = std::path::Path::new(".journalist/clips");
    if clips_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(clips_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
                .collect();
            files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
            files.truncate(10);
            if !files.is_empty() {
                let mut s = String::from("## 최근 스크랩 기사\n");
                for f in &files {
                    if let Ok(content) = std::fs::read_to_string(f.path()) {
                        let preview: String = content.chars().take(300).collect();
                        s.push_str(&format!(
                            "\n### {}\n{}\n",
                            f.file_name().to_string_lossy(),
                            preview
                        ));
                    }
                }
                sections.push(s);
            }
        }
    }

    // SNS trends
    let sns_dir = std::path::Path::new(".journalist/sns");
    if sns_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(sns_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
                .collect();
            files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
            files.truncate(5);
            if !files.is_empty() {
                let mut s = String::from("## SNS 트렌드\n");
                for f in &files {
                    if let Ok(content) = std::fs::read_to_string(f.path()) {
                        let preview: String = content.chars().take(300).collect();
                        s.push_str(&format!(
                            "\n### {}\n{}\n",
                            f.file_name().to_string_lossy(),
                            preview
                        ));
                    }
                }
                sections.push(s);
            }
        }
    }

    // Sources
    let sources_path = std::path::Path::new(".journalist/sources.json");
    if sources_path.exists() {
        if let Ok(content) = std::fs::read_to_string(sources_path) {
            if let Ok(sources) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                if !sources.is_empty() {
                    let mut s = String::from("## 취재원 목록\n");
                    for src in sources.iter().take(20) {
                        let name = src.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let beat = src.get("beat").and_then(|v| v.as_str()).unwrap_or("");
                        let org = src.get("org").and_then(|v| v.as_str()).unwrap_or("");
                        s.push_str(&format!("- {name} ({org}) [beat: {beat}]\n"));
                    }
                    sections.push(s);
                }
            }
        }
    }

    // Archive
    let archive_dir = std::path::Path::new(".journalist/archive");
    if archive_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(archive_dir) {
            let mut files: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .map_or(false, |ext| ext == "md" || ext == "json")
                })
                .collect();
            files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
            files.truncate(10);
            if !files.is_empty() {
                let mut s = String::from("## 기사 아카이브\n");
                for f in &files {
                    s.push_str(&format!("- {}\n", f.file_name().to_string_lossy()));
                }
                sections.push(s);
            }
        }
    }

    if sections.is_empty() {
        String::from("(취재 데이터가 아직 없습니다.)")
    } else {
        sections.join("\n---\n\n")
    }
}

/// Build the autopitch prompt for the AI.
pub fn build_autopitch_prompt(beat: Option<&str>, data: &str) -> String {
    let beat_context = match beat {
        Some(b) => format!("\n\n출입처/분야: {b}\n이 분야에 특화된 기사 아이디어를 중심으로 제안하세요."),
        None => String::new(),
    };
    format!(
        "당신은 한국 신문사의 선배 기자입니다. 아래 취재 데이터를 분석하고 기사 아이디어를 제안하세요.{beat_context}

다음 세 가지 카테고리로 각 2-3개씩 제안해주세요:

### 1. 미발굴 각도
기존 취재 주제에서 아직 다루지 않은 새로운 시각이나 각도

### 2. 후속 보도 기회
기존 취재/기사를 발전시킬 수 있는 후속 보도 아이디어

### 3. 시의성 있는 주제
현재 시점에서 시의성이 높은 기사 주제

각 아이디어마다:
- **제목** (가제)
- **핵심 앵글** (1-2문장)
- **취재 방향** (어떤 취재원, 어떤 데이터가 필요한지)
- **시의성** (왜 지금 이 기사가 필요한지)

---

{data}"
    )
}

/// Save autopitch result to `.journalist/pitches/` directory.
fn save_autopitch(content: &str, beat: Option<&str>) -> std::path::PathBuf {
    let dir = std::path::Path::new(PITCHES_DIR);
    std::fs::create_dir_all(dir).ok();

    let beat_suffix = match beat {
        Some(b) => format!("_{b}"),
        None => String::new(),
    };
    let filename = format!("pitch{beat_suffix}.md");
    let path = dir.join(&filename);
    std::fs::write(&path, content).ok();
    path
}

/// Handle the `/autopitch` command: AI-powered article idea generation.
pub async fn handle_autopitch(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let raw_args = input.strip_prefix("/autopitch").unwrap_or("").trim();
    let (beat, _extra) = parse_autopitch_args(raw_args);

    if raw_args == "help" || raw_args == "--help" {
        autopitch_print_help();
        return;
    }

    // Collect journalist data
    let data = collect_journalist_data();

    let beat_display = beat.as_deref().unwrap_or("전체");
    println!("{DIM}  📰 기사 아이디어 생성 중... (분야: {beat_display}){RESET}");

    let prompt = build_autopitch_prompt(beat.as_deref(), &data);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if !response.trim().is_empty() {
        let path = save_autopitch(&response, beat.as_deref());
        println!(
            "{DIM}  💡 결과가 {}에 저장되었습니다.{RESET}\n",
            path.display()
        );
    }
}

fn autopitch_print_help() {
    println!("{DIM}  /autopitch — AI 기사 아이디어 제안{RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /autopitch                  전체 분야 기사 아이디어 제안{RESET}");
    println!("{DIM}    /autopitch --beat <분야>    특정 출입처/분야 맞춤 제안{RESET}");
    println!("{DIM}  예시:{RESET}");
    println!("{DIM}    /autopitch{RESET}");
    println!("{DIM}    /autopitch --beat 경제{RESET}");
    println!(
        "{DIM}    /autopitch --beat 정치{RESET}\n"
    );
}

// ── /morning — 아침 브리핑 루틴 원커맨드 ──────────────────────────────

const MORNING_DIR: &str = ".journalist/morning";

/// Collect local data for the morning briefing from various .journalist/ sources.
pub fn collect_morning_data() -> String {
    let mut sections = Vec::new();
    let today = today_date_string();

    // 1. Calendar: today's events
    let calendar = load_calendar_from(&calendar_path());
    let mut today_events: Vec<&CalendarEvent> =
        calendar.iter().filter(|e| e.date == today && !e.done).collect();
    today_events.sort_by(|a, b| a.time.cmp(&b.time));
    if !today_events.is_empty() {
        let mut s = format!("## 오늘 일정 ({today})\n");
        for ev in &today_events {
            s.push_str(&format!("- {} {}\n", ev.time, ev.description));
        }
        sections.push(s);
    } else {
        sections.push(format!("## 오늘 일정 ({today})\n등록된 일정 없음\n"));
    }

    // 2. Deadlines: within 3 days
    let deadlines = load_deadlines_from(&deadlines_path());
    let mut urgent_deadlines: Vec<(&Deadline, String)> = Vec::new();
    for dl in &deadlines {
        let date_part = dl.datetime.split('T').next().unwrap_or(&dl.datetime);
        if let Some(days) = days_until(date_part, &today) {
            if days <= 3 && days >= 0 {
                let label = if days == 0 {
                    "오늘".to_string()
                } else {
                    format!("{days}일 후")
                };
                urgent_deadlines.push((dl, label));
            }
        }
    }
    if !urgent_deadlines.is_empty() {
        let mut s = String::from("## 마감 임박 (3일 이내)\n");
        for (dl, label) in &urgent_deadlines {
            s.push_str(&format!("- [{}] {} ({})\n", label, dl.title, dl.datetime));
        }
        sections.push(s);
    } else {
        sections.push("## 마감 임박 (3일 이내)\n임박한 마감 없음\n".to_string());
    }

    // 3. Follow-up reminders: within 3 days
    let followups = load_followups_from(&followups_path());
    let mut urgent_follows: Vec<(&Followup, i64)> = Vec::new();
    for f in &followups {
        if f.done {
            continue;
        }
        if let Some(ref due) = f.due {
            if let Some(days) = days_until(due, &today) {
                if days <= 3 {
                    urgent_follows.push((f, days));
                }
            }
        }
    }
    urgent_follows.sort_by_key(|(_, d)| *d);
    if !urgent_follows.is_empty() {
        let mut s = String::from("## 후속보도 리마인드\n");
        for (f, days) in &urgent_follows {
            let label = if *days < 0 {
                "기한 초과".to_string()
            } else if *days == 0 {
                "오늘 마감".to_string()
            } else {
                format!("{days}일 남음")
            };
            let due_str = f.due.as_deref().unwrap_or("");
            s.push_str(&format!("- {} [{}] (마감: {})\n", f.topic, label, due_str));
        }
        sections.push(s);
    } else {
        sections.push("## 후속보도 리마인드\n3일 이내 임박 건 없음\n".to_string());
    }

    // 4. Desk: pending assignments
    let desk = load_desk_from(&desk_path());
    let pending: Vec<&DeskAssignment> = desk
        .iter()
        .filter(|a| a.status == DeskStatus::Pending)
        .collect();
    if !pending.is_empty() {
        let mut s = String::from("## 데스크 지시 대기 건\n");
        for a in &pending {
            let dl_info = a.deadline.as_deref().unwrap_or("마감 미정");
            s.push_str(&format!(
                "- [{}] {} (마감: {})\n",
                a.reporter, a.content, dl_info
            ));
        }
        sections.push(s);
    } else {
        sections.push("## 데스크 지시 대기 건\n대기 중인 업무 없음\n".to_string());
    }

    // 5. Recent journalist context (reuse existing function)
    let journalist_data = collect_journalist_data();
    if !journalist_data.contains("(데이터 없음)") {
        sections.push(format!("## 최근 취재 컨텍스트\n{journalist_data}"));
    }

    sections.join("\n")
}

/// Build the prompt for the morning briefing AI call.
pub fn build_morning_prompt(data: &str) -> String {
    format!(
        "당신은 한국 신문사 기자의 아침 브리핑 비서입니다. \
아래 데이터를 바탕으로 오늘 하루 업무를 시작하기 위한 종합 아침 브리핑을 작성하세요.

다음 항목을 포함하세요:

### 📅 오늘 일정 요약
일정이 있으면 시간순으로 정리하고, 준비 사항이 있으면 알려주세요.

### ⏰ 마감 임박 경고
3일 이내 마감이 있으면 우선순위를 매기고 조언하세요.

### 📰 후속보도 리마인드
임박한 후속보도가 있으면 간략히 상기시키세요.

### 📋 데스크 지시 대기 건
대기 중인 업무가 있으면 요약하고 우선순위를 제안하세요.

### 🌐 오늘의 주요 이슈
최근 취재 맥락을 고려하여, 오늘 주목할 만한 이슈나 뉴스 각도를 2-3개 제안하세요.

### 🎯 오늘의 추천 액션
위 모든 정보를 종합하여, 오늘 가장 먼저 해야 할 일 3가지를 제안하세요.

간결하고 실용적으로 작성하세요. 기자가 5분 안에 읽고 바로 업무에 들어갈 수 있어야 합니다.

---

{data}"
    )
}

/// Save morning briefing to `.journalist/morning/` directory.
fn save_morning_briefing(content: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(MORNING_DIR);
    std::fs::create_dir_all(dir).ok();

    let today = today_date_string();
    let filename = format!("{today}_briefing.md");
    let path = dir.join(&filename);
    std::fs::write(&path, content).ok();
    path
}

/// Handle the `/morning` command: one-command morning briefing routine.
pub async fn handle_morning(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/morning").unwrap_or("").trim();

    if args == "help" || args == "--help" {
        morning_print_help();
        return;
    }

    println!("{DIM}  ☀️ 아침 브리핑 준비 중...{RESET}");

    let data = collect_morning_data();

    println!("{DIM}  📊 데이터 수집 완료. AI 브리핑 생성 중...{RESET}");

    let prompt = build_morning_prompt(&data);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if !response.trim().is_empty() {
        let path = save_morning_briefing(&response);
        println!(
            "{DIM}  ☀️ 브리핑이 {}에 저장되었습니다.{RESET}\n",
            path.display()
        );
    }
}

fn morning_print_help() {
    println!("{DIM}  /morning — 아침 브리핑 루틴 원커맨드{RESET}");
    println!("{DIM}  출근하면 가장 먼저 실행하세요!{RESET}");
    println!("{DIM}  다음 정보를 종합하여 AI 브리핑을 생성합니다:{RESET}");
    println!("{DIM}    • 오늘 일정 (/calendar today){RESET}");
    println!("{DIM}    • 마감 임박 (/deadline 3일 이내){RESET}");
    println!("{DIM}    • 후속보도 리마인드 (/follow remind){RESET}");
    println!("{DIM}    • 데스크 지시 대기 건 (/desk list pending){RESET}");
    println!("{DIM}    • 오늘의 주요 이슈 요약 (AI 기반){RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /morning              아침 브리핑 실행{RESET}");
    println!("{DIM}    /morning help         도움말{RESET}\n");
}

// ── /breaking ────────────────────────────────────────────────────────────

const BREAKING_DIR: &str = ".journalist/breaking";

fn breaking_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(BREAKING_DIR)
}

/// Generate a timestamped filename for a breaking news draft.
fn breaking_file_path(topic: &str) -> std::path::PathBuf {
    breaking_file_path_with_ts(topic, &now_ts())
}

fn now_ts() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Format as YYYY-MM-DD_HHMMSS (UTC)
    let days = secs / 86400;
    let day_secs = secs % 86400;
    let h = day_secs / 3600;
    let m = (day_secs % 3600) / 60;
    let s = day_secs % 60;
    // days since epoch to date
    let (y, mo, d) = epoch_days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02}_{h:02}{m:02}{s:02}")
}

fn epoch_days_to_ymd(mut days: i64) -> (i64, i64, i64) {
    // Civil from days algorithm (Howard Hinnant)
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn breaking_file_path_with_ts(topic: &str, ts: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(topic, 40);
    let filename = if slug.is_empty() {
        format!("{ts}_breaking.md")
    } else {
        format!("{ts}_{slug}.md")
    };
    breaking_dir().join(filename)
}

fn save_breaking(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// List breaking news files, most recent first.
fn list_breaking_files() -> Vec<std::path::PathBuf> {
    let dir = breaking_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    files.reverse();
    files
}

/// Build the breaking news prompt for AI.
fn build_breaking_prompt(topic: &str) -> String {
    format!(
        "속보 기사 작성 보조 요청.\n\n\
         속보 주제: {topic}\n\n\
         다음 4가지를 빠르고 정확하게 생성해주세요:\n\n\
         ## 1. 핵심 팩트 정리 프레임워크\n\
         - 5W1H (누가, 언제, 어디서, 무엇을, 왜, 어떻게) 기반으로 현재 확인된 사실과 미확인 사항을 구분하여 정리\n\
         - 각 항목에 '확인됨/미확인' 표시\n\n\
         ## 2. 속보 기사 초안\n\
         - 역피라미드 구조 (가장 중요한 정보부터)\n\
         - 첫 문장에 핵심 팩트 압축\n\
         - 리드 → 본문 → 배경 순서\n\
         - 400~600자 분량\n\
         - [속보] 태그 포함\n\n\
         ## 3. 후속 취재 포인트\n\
         - 추가로 확인해야 할 사항 5개 이상\n\
         - 각 포인트에 대한 취재 방향 제안\n\
         - 우선순위 표시 (긴급/중요/참고)\n\n\
         ## 4. 확인 필요 사항 체크리스트\n\
         - 팩트체크 항목 (출처 확인, 수치 검증 등)\n\
         - 법적 주의사항 (명예훼손, 개인정보 등)\n\
         - 추가 취재원 목록\n\n\
         한국어로 작성하세요. 속보 상황이므로 간결하고 정확하게."
    )
}

/// Build prompt for a breaking news update.
fn build_breaking_update_prompt(original: &str, update_info: &str) -> String {
    format!(
        "속보 업데이트 요청.\n\n\
         ## 기존 속보 기사:\n{original}\n\n\
         ## 추가 정보:\n{update_info}\n\n\
         위 추가 정보를 반영하여 다음을 생성해주세요:\n\n\
         ## 1. 업데이트된 속보 기사\n\
         - 새로운 정보를 반영한 전체 기사 (역피라미드 구조 유지)\n\
         - [속보 업데이트] 태그\n\
         - 변경/추가된 부분 명시\n\n\
         ## 2. 갱신된 확인 필요 사항\n\
         - 새로 확인된 팩트 체크\n\
         - 여전히 미확인인 사항\n\n\
         ## 3. 추가 후속 취재 포인트\n\
         - 새 정보로 인해 발생한 추가 취재 방향\n\n\
         한국어로 작성하세요. 속보 상황이므로 간결하고 정확하게."
    )
}

/// Parse the `/breaking` command input and determine the subcommand.
pub enum BreakingAction {
    /// New breaking news: `/breaking <topic>`
    New(String),
    /// Update latest breaking: `/breaking update <info>`
    Update(String),
    /// List recent breaking news: `/breaking list`
    List,
    /// Help
    Help,
}

pub fn parse_breaking_input(input: &str) -> BreakingAction {
    let args = input.strip_prefix("/breaking").unwrap_or("").trim();

    if args.is_empty() || args == "help" || args == "--help" {
        return BreakingAction::Help;
    }

    if args == "list" {
        return BreakingAction::List;
    }

    if let Some(rest) = args.strip_prefix("update") {
        let info = rest.trim();
        return BreakingAction::Update(info.to_string());
    }

    BreakingAction::New(args.to_string())
}

pub async fn handle_breaking(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    match parse_breaking_input(input) {
        BreakingAction::Help => {
            print_breaking_help();
        }
        BreakingAction::List => {
            print_breaking_list();
        }
        BreakingAction::New(topic) => {
            println!("{DIM}  🚨 속보 워크플로우 시작: {topic}{RESET}");

            let prompt = build_breaking_prompt(&topic);
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);

            if !response.trim().is_empty() {
                let path = breaking_file_path(&topic);
                match save_breaking(&path, &response) {
                    Ok(_) => {
                        println!(
                            "{GREEN}  ✓ 속보 초안 저장: {}{RESET}\n",
                            path.display()
                        );
                    }
                    Err(e) => {
                        eprintln!("{RED}  속보 저장 실패: {e}{RESET}\n");
                    }
                }
            }
        }
        BreakingAction::Update(info) => {
            if info.is_empty() {
                eprintln!("{RED}  사용법: /breaking update <추가 정보>{RESET}\n");
                return;
            }

            // Find the most recent breaking file
            let files = list_breaking_files();
            if files.is_empty() {
                eprintln!("{RED}  업데이트할 속보가 없습니다. /breaking <주제>로 먼저 속보를 작성하세요.{RESET}\n");
                return;
            }

            let latest = &files[0];
            let original = match std::fs::read_to_string(latest) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            };

            println!(
                "{DIM}  🔄 속보 업데이트 중... (기반: {}){RESET}",
                latest.file_name().unwrap_or_default().to_string_lossy()
            );

            let prompt = build_breaking_update_prompt(&original, &info);
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);

            if !response.trim().is_empty() {
                // Extract topic from the original filename
                let stem = latest
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy();
                // Remove the timestamp prefix (YYYY-MM-DD_HHMMSS_)
                let topic_part = if stem.len() > 16 {
                    &stem[16..]
                } else {
                    "update"
                };
                let path = breaking_file_path(topic_part);
                match save_breaking(&path, &response) {
                    Ok(_) => {
                        println!(
                            "{GREEN}  ✓ 속보 업데이트 저장: {}{RESET}\n",
                            path.display()
                        );
                    }
                    Err(e) => {
                        eprintln!("{RED}  저장 실패: {e}{RESET}\n");
                    }
                }
            }
        }
    }
}

fn print_breaking_help() {
    println!("{DIM}  /breaking — 속보 워크플로우 원커맨드{RESET}");
    println!("{DIM}  속보 발생 시 취재·작성·출고를 단축합니다.{RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /breaking <속보 주제>             속보 초안 생성 (팩트 프레임워크 + 기사 + 후속취재 + 체크리스트){RESET}");
    println!("{DIM}    /breaking update <추가 정보>      최근 속보에 추가 정보 반영하여 업데이트{RESET}");
    println!("{DIM}    /breaking list                   최근 속보 이력 조회{RESET}");
    println!("{DIM}    /breaking help                   도움말{RESET}\n");
}

fn print_breaking_list() {
    let files = list_breaking_files();
    if files.is_empty() {
        println!("{DIM}  속보 기록이 없습니다.{RESET}\n");
        return;
    }

    println!("{DIM}  📋 최근 속보 이력 (최신순):{RESET}");
    for (i, path) in files.iter().enumerate().take(20) {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        println!("  {: >3}. {name}", i + 1);
    }
    println!();
}

// ── /recap ──────────────────────────────────────────────────────────────

const RECAP_DIR: &str = ".journalist/recap";

/// Collect today's journalist data for the daily recap.
///
/// Gathers: notes, contacts, calendar (done/undone), drafts modified today,
/// deadline status changes, and desk assignment status.
pub fn collect_recap_data() -> String {
    let mut sections = Vec::new();
    let today = today_date_string();

    // 1. Notes written today
    let notes_file = notes_file_for_date(&today);
    let notes = load_notes_from(&notes_file);
    if !notes.is_empty() {
        let mut s = format!("## 오늘 작성한 메모 ({today}, {}건)\n", notes.len());
        for n in &notes {
            let tags = [
                n.source.as_deref().map(|s| format!("[취재원: {s}]")),
                n.topic.as_deref().map(|t| format!("[주제: {t}]")),
            ];
            let tag_str: String = tags.iter().flatten().cloned().collect::<Vec<_>>().join(" ");
            if tag_str.is_empty() {
                s.push_str(&format!("- {}\n", n.content));
            } else {
                s.push_str(&format!("- {} {tag_str}\n", n.content));
            }
        }
        sections.push(s);
    } else {
        sections.push(format!("## 오늘 작성한 메모 ({today})\n메모 없음\n"));
    }

    // 2. Contacts logged today
    let all_contacts = load_all_contact_logs();
    let today_contacts: Vec<&ContactLog> = all_contacts
        .iter()
        .filter(|c| c.timestamp.starts_with(&today))
        .collect();
    if !today_contacts.is_empty() {
        let mut s = format!(
            "## 오늘 접촉한 취재원 ({}건)\n",
            today_contacts.len()
        );
        for c in &today_contacts {
            s.push_str(&format!("- {} — {}\n", c.name, c.summary));
        }
        sections.push(s);
    } else {
        sections.push("## 오늘 접촉한 취재원\n접촉 기록 없음\n".to_string());
    }

    // 3. Calendar: today's events with completion status
    let calendar = load_calendar_from(&calendar_path());
    let today_events: Vec<&CalendarEvent> =
        calendar.iter().filter(|e| e.date == today).collect();
    if !today_events.is_empty() {
        let done_count = today_events.iter().filter(|e| e.done).count();
        let total = today_events.len();
        let mut s = format!("## 오늘 일정 (완료 {done_count}/{total})\n");
        for ev in &today_events {
            let mark = if ev.done { "✅" } else { "⬜" };
            s.push_str(&format!("- {mark} {} {}\n", ev.time, ev.description));
        }
        sections.push(s);
    } else {
        sections.push("## 오늘 일정\n등록된 일정 없음\n".to_string());
    }

    // 4. Drafts modified today (check filesystem mtime)
    let drafts_dir = std::path::Path::new(DRAFTS_DIR);
    if drafts_dir.exists() {
        let mut today_drafts: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(drafts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            let secs = modified
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let days = (secs / 86400) as i64;
                            let (y, m, d) = epoch_days_to_ymd(days);
                            let mod_date = format!("{y:04}-{m:02}-{d:02}");
                            if mod_date == today {
                                let name = path
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                today_drafts.push(name);
                            }
                        }
                    }
                }
            }
        }
        if !today_drafts.is_empty() {
            today_drafts.sort();
            let mut s = format!("## 오늘 작업한 초고 ({}건)\n", today_drafts.len());
            for name in &today_drafts {
                s.push_str(&format!("- {name}\n"));
            }
            sections.push(s);
        } else {
            sections.push("## 오늘 작업한 초고\n작업한 초고 없음\n".to_string());
        }
    } else {
        sections.push("## 오늘 작업한 초고\n작업한 초고 없음\n".to_string());
    }

    // 5. Deadline status
    let deadlines = load_deadlines_from(&deadlines_path());
    let mut deadline_info: Vec<String> = Vec::new();
    for dl in &deadlines {
        let date_part = dl.datetime.split('T').next().unwrap_or(&dl.datetime);
        if let Some(days) = days_until(date_part, &today) {
            if days <= 3 && days >= -1 {
                let status = if days < 0 {
                    "⚠️ 기한 초과"
                } else if days == 0 {
                    "🔴 오늘 마감"
                } else {
                    "🟡 임박"
                };
                deadline_info.push(format!(
                    "- {status} {} ({})",
                    dl.title, dl.datetime
                ));
            }
        }
    }
    if !deadline_info.is_empty() {
        let mut s = String::from("## 마감 상태\n");
        for info in &deadline_info {
            s.push_str(&format!("{info}\n"));
        }
        sections.push(s);
    } else {
        sections.push("## 마감 상태\n임박한 마감 없음\n".to_string());
    }

    // 6. Desk assignment status
    let desk = load_desk_from(&desk_path());
    if !desk.is_empty() {
        let completed: Vec<&DeskAssignment> = desk
            .iter()
            .filter(|a| a.status == DeskStatus::Done)
            .collect();
        let pending: Vec<&DeskAssignment> = desk
            .iter()
            .filter(|a| a.status == DeskStatus::Pending)
            .collect();
        let mut s = format!(
            "## 데스크 지시 현황 (완료 {}, 대기 {})\n",
            completed.len(),
            pending.len()
        );
        for a in &completed {
            s.push_str(&format!("- ✅ {}\n", a.content));
        }
        for a in &pending {
            s.push_str(&format!("- ⬜ {}\n", a.content));
        }
        sections.push(s);
    } else {
        sections.push("## 데스크 지시 현황\n데스크 지시 없음\n".to_string());
    }

    sections.join("\n")
}

/// Build the prompt for the daily recap AI call.
pub fn build_recap_prompt(data: &str) -> String {
    format!(
        "당신은 한국 신문사 기자의 하루 마감 회고 비서입니다. \
아래 데이터를 바탕으로 오늘 하루를 정리하는 마감 회고를 작성하세요.

다음 항목을 포함하세요:

### 📝 오늘 한 일 요약
오늘 수행한 취재 활동, 작성한 메모, 접촉한 취재원, 작업한 초고를 종합적으로 정리하세요.

### ⏳ 미완료 사항 & 내일 이월 항목
완료하지 못한 일정, 대기 중인 데스크 지시, 임박한 마감 등을 정리하세요.

### 🏆 오늘의 취재 성과
오늘 특별히 잘한 점이나 의미 있는 취재 진전을 짚어주세요. \
성과가 작더라도 긍정적으로 평가하세요.

### 🎯 내일 우선순위 제안
위 모든 정보를 종합하여, 내일 가장 먼저 해야 할 일 3가지를 우선순위와 함께 제안하세요.

간결하고 실용적으로 작성하세요. 기자가 퇴근 전 3분 안에 읽고 내일을 준비할 수 있어야 합니다.

---

{data}"
    )
}

/// Save daily recap to `.journalist/recap/YYYY-MM-DD.md`.
fn save_recap(content: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(RECAP_DIR);
    std::fs::create_dir_all(dir).ok();

    let today = today_date_string();
    let path = dir.join(format!("{today}.md"));
    std::fs::write(&path, content).ok();
    path
}

/// Handle the `/recap` command: daily wrap-up review.
pub async fn handle_recap(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/recap").unwrap_or("").trim();

    if args == "help" || args == "--help" {
        recap_print_help();
        return;
    }

    if args == "list" {
        recap_print_list();
        return;
    }

    println!("{DIM}  🌙 하루 마감 회고 준비 중...{RESET}");

    let data = collect_recap_data();

    println!("{DIM}  📊 데이터 수집 완료. AI 회고 생성 중...{RESET}");

    let prompt = build_recap_prompt(&data);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if !response.trim().is_empty() {
        let path = save_recap(&response);
        println!(
            "{DIM}  🌙 회고가 {}에 저장되었습니다.{RESET}\n",
            path.display()
        );
    }
}

fn recap_print_help() {
    println!("{DIM}  /recap — 하루 마감 회고{RESET}");
    println!("{DIM}  퇴근 전 실행하면 오늘 하루를 자동 정리합니다.{RESET}");
    println!("{DIM}  다음 정보를 종합하여 AI 회고를 생성합니다:{RESET}");
    println!("{DIM}    • 오늘 작성한 메모 (notes){RESET}");
    println!("{DIM}    • 오늘 접촉한 취재원 (contacts){RESET}");
    println!("{DIM}    • 오늘 일정 완료 여부 (calendar){RESET}");
    println!("{DIM}    • 오늘 작업한 초고 (drafts){RESET}");
    println!("{DIM}    • 마감 상태 변화 (deadlines){RESET}");
    println!("{DIM}    • 데스크 지시 처리 현황 (desk){RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /recap              하루 마감 회고 실행{RESET}");
    println!("{DIM}    /recap list         과거 회고 목록{RESET}");
    println!("{DIM}    /recap help         도움말{RESET}\n");
}

fn recap_print_list() {
    let dir = std::path::Path::new(RECAP_DIR);
    if !dir.exists() {
        println!("{DIM}  회고 기록이 없습니다.{RESET}\n");
        return;
    }
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    files.reverse();
    if files.is_empty() {
        println!("{DIM}  회고 기록이 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  📋 과거 회고 목록 (최신순):{RESET}");
    for (i, path) in files.iter().enumerate().take(20) {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        println!("  {: >3}. {name}", i + 1);
    }
    println!();
}

// ── /diary — 취재 일지 자동 생성 ────────────────────────────────────────

const DIARY_DIR: &str = ".journalist/diary";

/// Collect today's journalist data for the official diary.
///
/// Gathers: notes, contacts, calendar, sources, drafts — structured for
/// institutional reporting diary format.
pub fn collect_diary_data() -> String {
    let mut sections = Vec::new();
    let today = today_date_string();

    // 1. Notes written today
    let notes_file = notes_file_for_date(&today);
    let notes = load_notes_from(&notes_file);
    if !notes.is_empty() {
        let mut s = format!("## 취재 메모 ({today}, {}건)\n", notes.len());
        for n in &notes {
            let tags = [
                n.source.as_deref().map(|s| format!("[취재원: {s}]")),
                n.topic.as_deref().map(|t| format!("[주제: {t}]")),
            ];
            let tag_str: String = tags.iter().flatten().cloned().collect::<Vec<_>>().join(" ");
            if tag_str.is_empty() {
                s.push_str(&format!("- {}\n", n.content));
            } else {
                s.push_str(&format!("- {} {tag_str}\n", n.content));
            }
        }
        sections.push(s);
    } else {
        sections.push(format!("## 취재 메모 ({today})\n메모 없음\n"));
    }

    // 2. Contact logs today
    let all_contacts = load_all_contact_logs();
    let today_contacts: Vec<&ContactLog> = all_contacts
        .iter()
        .filter(|c| c.timestamp.starts_with(&today))
        .collect();
    if !today_contacts.is_empty() {
        let mut s = format!("## 취재원 접촉 기록 ({}건)\n", today_contacts.len());
        for c in &today_contacts {
            s.push_str(&format!("- {} — {}\n", c.name, c.summary));
        }
        sections.push(s);
    } else {
        sections.push("## 취재원 접촉 기록\n접촉 기록 없음\n".to_string());
    }

    // 3. Calendar: today's events
    let calendar = load_calendar_from(&calendar_path());
    let today_events: Vec<&CalendarEvent> =
        calendar.iter().filter(|e| e.date == today).collect();
    if !today_events.is_empty() {
        let done_count = today_events.iter().filter(|e| e.done).count();
        let total = today_events.len();
        let mut s = format!("## 일정 (완료 {done_count}/{total})\n");
        for ev in &today_events {
            let mark = if ev.done { "✅" } else { "⬜" };
            s.push_str(&format!("- {mark} {} {}\n", ev.time, ev.description));
        }
        sections.push(s);
    } else {
        sections.push("## 일정\n등록된 일정 없음\n".to_string());
    }

    // 4. Sources (full list for reference)
    let sources = load_sources_from(std::path::Path::new(SOURCES_FILE));
    if !sources.is_empty() {
        let mut s = format!("## 등록 취재원 ({}명)\n", sources.len());
        for src in &sources {
            let name = src["name"].as_str().unwrap_or("-");
            let org = src["org"].as_str().unwrap_or("");
            let note = src["note"].as_str().unwrap_or("");
            if note.is_empty() {
                s.push_str(&format!("- {name} ({org})\n"));
            } else {
                s.push_str(&format!("- {name} ({org}) — {note}\n"));
            }
        }
        sections.push(s);
    }

    // 5. Drafts modified today
    let drafts_dir = std::path::Path::new(DRAFTS_DIR);
    if drafts_dir.exists() {
        let mut today_drafts: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(drafts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Ok(meta) = path.metadata() {
                        if let Ok(modified) = meta.modified() {
                            let secs = modified
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let days = (secs / 86400) as i64;
                            let (y, m, d) = epoch_days_to_ymd(days);
                            let mod_date = format!("{y:04}-{m:02}-{d:02}");
                            if mod_date == today {
                                let name = path
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                today_drafts.push(name);
                            }
                        }
                    }
                }
            }
        }
        if !today_drafts.is_empty() {
            today_drafts.sort();
            let mut s = format!("## 작업 초고 ({}건)\n", today_drafts.len());
            for name in &today_drafts {
                s.push_str(&format!("- {name}\n"));
            }
            sections.push(s);
        }
    }

    sections.join("\n")
}

/// Build the AI prompt for diary generation.
fn build_diary_prompt(data: &str, format: &str) -> String {
    if format == "brief" {
        format!(
            "당신은 한국 신문사 기자의 업무 보조 AI입니다.
아래 데이터를 바탕으로 **간략 취재 일지**를 작성하세요.

### 양식
- 날짜
- 주요 취재 내용 (3줄 이내 요약)
- 접촉 취재원 목록
- 비고

간결하게, 핵심만 적으세요.

---

{data}"
        )
    } else {
        format!(
            "당신은 한국 신문사 기자의 업무 보조 AI입니다.
아래 데이터를 바탕으로 **공식 취재 일지**를 작성하세요.
편집국에 제출할 수 있는 기관 양식에 맞춰 정리합니다.

### 양식 (표 형태)

| 항목 | 내용 |
|------|------|
| **날짜** | (오늘 날짜) |
| **기자명** | (기자 이름 — 데이터에 없으면 빈칸) |
| **소속** | (부서 — 데이터에 없으면 빈칸) |

### 취재 활동 상세

| 시간 | 취재처 | 취재 내용 | 취재원 | 비고 |
|------|--------|-----------|--------|------|
| ... | ... | ... | ... | ... |

일정, 메모, 접촉 기록을 시간순으로 정리하여 표에 채우세요.
시간 정보가 없으면 빈칸으로 두세요.

### 취재 성과 요약
- 오늘 핵심 취재 성과 (2~3문장)
- 후속 취재 필요 사항

### 특이사항/비고
- 취재 과정에서 특이사항이 있으면 기록

실제 데이터에 있는 내용만 작성하고, 없는 내용은 만들어내지 마세요.

---

{data}"
        )
    }
}

/// Save diary to `.journalist/diary/YYYY-MM-DD.md`.
fn save_diary(content: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(DIARY_DIR);
    std::fs::create_dir_all(dir).ok();

    let today = today_date_string();
    let path = dir.join(format!("{today}.md"));
    std::fs::write(&path, content).ok();
    path
}

/// Handle the `/diary` command: generate daily reporting diary.
pub async fn handle_diary(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/diary").unwrap_or("").trim();

    if args == "help" || args == "--help" {
        diary_print_help();
        return;
    }

    if args == "list" {
        diary_print_list();
        return;
    }

    let format = if args == "--format brief" || args.contains("brief") {
        "brief"
    } else {
        "official"
    };

    println!(
        "{DIM}  📋 취재 일지 생성 중... (양식: {format}){RESET}"
    );

    let data = collect_diary_data();

    println!("{DIM}  📊 데이터 수집 완료. AI 일지 작성 중...{RESET}");

    let prompt = build_diary_prompt(&data, format);
    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if !response.trim().is_empty() {
        let path = save_diary(&response);
        println!(
            "{DIM}  📋 취재 일지가 {}에 저장되었습니다.{RESET}\n",
            path.display()
        );
    }
}

fn diary_print_help() {
    println!("{DIM}  /diary — 취재 일지 자동 생성{RESET}");
    println!("{DIM}  편집국에 제출할 수 있는 공식 취재 일지를 자동 생성합니다.{RESET}");
    println!("{DIM}  .journalist/notes, contacts, calendar, sources, drafts 데이터를 종합합니다.{RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /diary                   공식 양식으로 일지 생성{RESET}");
    println!("{DIM}    /diary --format official  공식 양식 (기본값){RESET}");
    println!("{DIM}    /diary --format brief     간략 양식{RESET}");
    println!("{DIM}    /diary list               과거 일지 목록{RESET}");
    println!("{DIM}    /diary help               도움말{RESET}\n");
}

fn diary_print_list() {
    let dir = std::path::Path::new(DIARY_DIR);
    if !dir.exists() {
        println!("{DIM}  취재 일지가 없습니다.{RESET}\n");
        return;
    }
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "md"))
        .collect();
    files.sort();
    files.reverse();
    if files.is_empty() {
        println!("{DIM}  취재 일지가 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  📋 과거 취재 일지 (최신순):{RESET}");
    for (i, path) in files.iter().enumerate().take(20) {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        println!("  {: >3}. {name}", i + 1);
    }
    println!();
}

// ── /rival ───────────────────────────────────────────────────────────────

const RIVAL_DIR: &str = ".journalist/rival";

/// Parsed rival command action.
pub enum RivalAction {
    Help,
    Search(String),
    Compare { my_file: String, rival: String },
}

/// Parse `/rival` input into an action.
pub fn parse_rival_input(input: &str) -> RivalAction {
    let args = input.strip_prefix("/rival").unwrap_or("").trim();

    if args.is_empty() || args == "help" || args == "--help" {
        return RivalAction::Help;
    }

    if let Some(keyword) = args.strip_prefix("search") {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            return RivalAction::Help;
        }
        return RivalAction::Search(keyword.to_string());
    }

    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() >= 2 {
        RivalAction::Compare {
            my_file: parts[0].to_string(),
            rival: parts[1..].join(" "),
        }
    } else {
        RivalAction::Help
    }
}

/// Build file path for rival analysis results.
pub fn rival_file_path(topic: &str) -> std::path::PathBuf {
    rival_file_path_with_date(topic, &today_str())
}

/// Build file path for rival analysis with explicit date (for testing).
pub fn rival_file_path_with_date(topic: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(topic, 50);
    let name = if slug.is_empty() {
        "rival".to_string()
    } else {
        slug
    };
    std::path::PathBuf::from(RIVAL_DIR).join(format!("{date}_{name}.md"))
}

/// Build the prompt for `/rival` direct comparison mode.
pub fn build_rival_prompt(my_content: &str, my_path: &str, rival_content: &str, rival_source: &str) -> String {
    format!(
        "아래 두 기사를 **경쟁 분석 관점**에서 비교해주세요.\n\n\
         '내 기사'와 '경쟁사 기사'를 다음 항목별로 분석해주세요:\n\n\
         ## 분석 항목\n\n\
         ### 1. 기사 각도(프레임) 차이\n\
         - 같은 사안을 어떤 각도에서 접근했는지\n\
         - 리드(첫 문단)의 초점 차이\n\
         - 헤드라인의 뉘앙스 차이\n\n\
         ### 2. 취재원 비교\n\
         - 내 기사에만 등장하는 취재원\n\
         - 경쟁사에만 등장하는 취재원\n\
         - 공통 취재원의 발언 차이\n\n\
         ### 3. 빠진 정보 (경쟁사가 다뤘는데 내가 놓친 것)\n\
         - 사실(팩트) 차이\n\
         - 배경 설명이나 맥락 차이\n\
         - 데이터나 수치 차이\n\n\
         ### 4. 강점 (내가 독점한 정보)\n\
         - 내 기사에만 있는 독자적 정보\n\
         - 더 깊이 다룬 부분\n\
         - 차별화된 시각이나 분석\n\n\
         ### 5. 구조·분량 비교\n\
         - 전체 분량 비교\n\
         - 기사 구조(역피라미드/내러티브 등) 비교\n\
         - 멀티미디어 활용 차이 (사진, 그래픽, 표 등)\n\n\
         ## 종합 평가\n\n\
         경쟁사 대비 강점/약점을 요약하고, 보완 기사나 후속 보도 방향을 제안해주세요.\n\n\
         ---\n\n\
         ## 내 기사: {my_path}\n\n\
         {my_content}\n\n\
         ---\n\n\
         ## 경쟁사 기사: {rival_source}\n\n\
         {rival_content}"
    )
}

/// Build the prompt for `/rival search` mode.
pub fn build_rival_search_prompt(keyword: &str) -> String {
    format!(
        "다음 키워드로 경쟁사 기사를 검색하고 비교 분석 대상을 추천해주세요.\n\n\
         **키워드**: {keyword}\n\n\
         ## 요청사항\n\n\
         1. 이 키워드와 관련된 주요 언론사 기사를 검색해주세요\n\
         2. 각 기사의 제목, 언론사, 발행일, 핵심 프레임을 정리해주세요\n\
         3. 비교 분석에 적합한 기사 2-3건을 추천해주세요\n\
         4. 추천 이유를 간략히 설명해주세요\n\n\
         결과를 표 형식으로 정리해주세요."
    )
}

/// Print help text for `/rival`.
fn print_rival_help() {
    println!("{DIM}  /rival — 경쟁사 기사 비교 분석{RESET}");
    println!();
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /rival <내 기사 파일> <경쟁사 기사 파일 또는 URL>{RESET}");
    println!("{DIM}    /rival search <키워드>{RESET}");
    println!();
    println!("{DIM}  예시:{RESET}");
    println!("{DIM}    /rival my_article.md rival_article.md{RESET}");
    println!("{DIM}    /rival draft.md https://news.example.com/article/123{RESET}");
    println!("{DIM}    /rival search 삼성전자 반도체{RESET}");
    println!();
    println!(
        "{DIM}  분석 항목: 프레임 차이, 취재원 비교, 빠진 정보, 강점, 구조·분량{RESET}"
    );
    println!("{DIM}  결과는 .journalist/rival/ 에 저장됩니다.{RESET}\n");
}

/// Save rival analysis result to file.
fn save_rival_result(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/rival` command.
pub async fn handle_rival(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    match parse_rival_input(input) {
        RivalAction::Help => {
            print_rival_help();
        }
        RivalAction::Search(keyword) => {
            println!("{DIM}  경쟁사 기사 검색: {keyword}{RESET}\n");
            let prompt = build_rival_search_prompt(&keyword);
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);

            if !response.trim().is_empty() {
                let path = rival_file_path(&keyword);
                match save_rival_result(&path, &response) {
                    Ok(_) => {
                        println!(
                            "\n{GREEN}  ✓ 검색 결과 저장: {}{RESET}\n",
                            path.display()
                        );
                    }
                    Err(e) => {
                        eprintln!("{RED}  결과 저장 실패: {e}{RESET}\n");
                    }
                }
            }
        }
        RivalAction::Compare { my_file, rival } => {
            // Read my article
            let my_content = match std::fs::read_to_string(&my_file) {
                Ok(c) => {
                    println!("{DIM}  내 기사 읽기: {my_file} ({} bytes){RESET}", c.len());
                    c
                }
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {my_file} — {e}{RESET}\n");
                    return;
                }
            };

            // Read rival article (file or URL)
            let (rival_content, rival_source) = if rival.starts_with("http://")
                || rival.starts_with("https://")
            {
                // URL: let the agent fetch it via the prompt
                println!("{DIM}  경쟁사 기사 URL: {rival}{RESET}");
                (
                    format!("[URL에서 기사를 가져와주세요: {rival}]"),
                    rival.clone(),
                )
            } else {
                match std::fs::read_to_string(&rival) {
                    Ok(c) => {
                        println!(
                            "{DIM}  경쟁사 기사 읽기: {rival} ({} bytes){RESET}",
                            c.len()
                        );
                        (c, rival.clone())
                    }
                    Err(e) => {
                        eprintln!("{RED}  파일 읽기 실패: {rival} — {e}{RESET}\n");
                        return;
                    }
                }
            };

            println!();

            let prompt =
                build_rival_prompt(&my_content, &my_file, &rival_content, &rival_source);
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);

            if !response.trim().is_empty() {
                let slug = format!("{my_file}_vs_{rival}");
                let path = rival_file_path(&slug);
                match save_rival_result(&path, &response) {
                    Ok(_) => {
                        println!(
                            "\n{GREEN}  ✓ 비교 분석 저장: {}{RESET}\n",
                            path.display()
                        );
                    }
                    Err(e) => {
                        eprintln!("{RED}  결과 저장 실패: {e}{RESET}\n");
                    }
                }
            }
        }
    }
}

// ── /pipeline ────────────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PipelineDef {
    pub name: String,
    pub steps: Vec<String>,
    pub created: String,
}

pub fn pipelines_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(".journalist/pipelines")
}

fn pipeline_path(name: &str) -> std::path::PathBuf {
    pipelines_dir().join(format!("{name}.json"))
}

fn ensure_pipelines_dir() {
    let dir = pipelines_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
}

/// Parse pipeline step definitions from a string. Handles both quoted and unquoted steps.
/// Example: `"research 반도체 수출" "factcheck" "article --type analysis 반도체"`
pub fn parse_pipeline_steps(input: &str) -> Vec<String> {
    let mut steps = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '"' {
            chars.next();
            let mut step = String::new();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                step.push(c);
            }
            let trimmed = step.trim().to_string();
            if !trimmed.is_empty() {
                steps.push(trimmed);
            }
        } else {
            let mut step = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '"' {
                    break;
                }
                step.push(c);
                chars.next();
            }
            let trimmed = step.trim().to_string();
            if !trimmed.is_empty() {
                steps.push(trimmed);
            }
        }
    }

    steps
}

pub fn save_pipeline_to_file(name: &str, steps: &[String]) -> std::io::Result<()> {
    ensure_pipelines_dir();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let def = PipelineDef {
        name: name.to_string(),
        steps: steps.to_vec(),
        created: format_unix_timestamp(secs),
    };
    let json = serde_json::to_string_pretty(&def)?;
    std::fs::write(pipeline_path(name), json)
}

pub fn load_pipeline_from_file(name: &str) -> Option<PipelineDef> {
    let path = pipeline_path(name);
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn list_pipelines() -> Vec<String> {
    list_pipelines_in(&pipelines_dir())
}

pub fn list_pipelines_in(dir: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry
                .file_name()
                .to_string_lossy()
                .strip_suffix(".json")
            {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names
}

/// Build a prompt for the AI to execute a pipeline's steps in sequence.
pub fn build_pipeline_run_prompt(def: &PipelineDef) -> String {
    let mut prompt = String::from(
        "다음 파이프라인을 순서대로 실행해주세요. \
         각 단계의 결과(특히 저장된 파일 경로)를 다음 단계의 입력으로 활용하세요.\n\n",
    );
    prompt.push_str(&format!("파이프라인: {}\n\n", def.name));
    for (i, step) in def.steps.iter().enumerate() {
        prompt.push_str(&format!("단계 {}: /{}\n", i + 1, step));
    }
    prompt.push_str("\n각 단계를 실행할 때:\n");
    prompt.push_str("1. 해당 단계의 커맨드를 실행하듯 작업을 수행하세요\n");
    prompt.push_str("2. 이전 단계에서 생성된 파일이 있으면 해당 파일을 참조하세요\n");
    prompt.push_str("3. 각 단계 완료 후 결과를 간략히 요약하세요\n");
    prompt.push_str("4. 모든 단계 완료 후 전체 파이프라인 실행 결과를 정리하세요\n");
    prompt
}

/// Handle `/pipeline` command. Returns Some(prompt) for `run`, None for local subcommands.
pub fn handle_pipeline(input: &str) -> Option<String> {
    let args = input.strip_prefix("/pipeline").unwrap_or("").trim();

    if args.is_empty() {
        println!("{DIM}  사용법:");
        println!("    /pipeline save <이름> <\"단계1\"> <\"단계2\"> ...");
        println!("    /pipeline run <이름>");
        println!("    /pipeline list");
        println!("    /pipeline show <이름>");
        println!("    /pipeline remove <이름>");
        println!();
        println!("  예시:");
        println!(
            "    /pipeline save 반도체속보 \"research 반도체 수출\" \"factcheck\" \"article --type analysis 반도체\""
        );
        println!("    /pipeline run 반도체속보{RESET}");
        return None;
    }

    let subcmd = args.split_whitespace().next().unwrap_or("");
    let rest = args[subcmd.len()..].trim();

    match subcmd {
        "save" => {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let name = match parts.next() {
                Some(n) if !n.is_empty() => n,
                _ => {
                    println!("{RED}  파이프라인 이름을 지정하세요.{RESET}");
                    println!(
                        "{DIM}  예: /pipeline save 반도체속보 \"research 반도체\" \"article\"{RESET}"
                    );
                    return None;
                }
            };
            let steps_str = parts.next().unwrap_or("");
            let steps = parse_pipeline_steps(steps_str);
            if steps.is_empty() {
                println!("{RED}  파이프라인 단계를 지정하세요.{RESET}");
                println!(
                    "{DIM}  예: /pipeline save {name} \"research 반도체\" \"factcheck\" \"article\"{RESET}"
                );
                return None;
            }
            match save_pipeline_to_file(name, &steps) {
                Ok(_) => {
                    println!(
                        "{GREEN}  ✓ 파이프라인 '{name}' 저장 ({} 단계){RESET}",
                        steps.len()
                    );
                    for (i, step) in steps.iter().enumerate() {
                        println!("{DIM}    {}. /{}{RESET}", i + 1, step);
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!("{RED}  파이프라인 저장 실패: {e}{RESET}\n");
                }
            }
            None
        }
        "list" => {
            let names = list_pipelines();
            if names.is_empty() {
                println!("{DIM}  저장된 파이프라인이 없습니다.{RESET}\n");
            } else {
                println!("{BOLD}  📋 파이프라인 목록 ({} 개){RESET}", names.len());
                for name in &names {
                    if let Some(def) = load_pipeline_from_file(name) {
                        println!("{DIM}    • {name} ({} 단계){RESET}", def.steps.len());
                    } else {
                        println!("{DIM}    • {name}{RESET}");
                    }
                }
                println!();
            }
            None
        }
        "show" => {
            if rest.is_empty() {
                println!("{RED}  파이프라인 이름을 지정하세요.{RESET}\n");
                return None;
            }
            match load_pipeline_from_file(rest) {
                Some(def) => {
                    println!("{BOLD}  🔗 파이프라인: {}{RESET}", def.name);
                    println!("{DIM}    생성: {}{RESET}", def.created);
                    println!("{DIM}    단계:{RESET}");
                    for (i, step) in def.steps.iter().enumerate() {
                        println!("      {}. /{}", i + 1, step);
                    }
                    println!();
                }
                None => {
                    eprintln!(
                        "{RED}  파이프라인 '{rest}'을(를) 찾을 수 없습니다.{RESET}\n"
                    );
                }
            }
            None
        }
        "remove" => {
            if rest.is_empty() {
                println!("{RED}  파이프라인 이름을 지정하세요.{RESET}\n");
                return None;
            }
            let path = pipeline_path(rest);
            if path.exists() {
                match std::fs::remove_file(&path) {
                    Ok(_) => println!("{GREEN}  ✓ 파이프라인 '{rest}' 삭제{RESET}\n"),
                    Err(e) => eprintln!("{RED}  삭제 실패: {e}{RESET}\n"),
                }
            } else {
                eprintln!(
                    "{RED}  파이프라인 '{rest}'을(를) 찾을 수 없습니다.{RESET}\n"
                );
            }
            None
        }
        "run" => {
            if rest.is_empty() {
                println!("{RED}  실행할 파이프라인 이름을 지정하세요.{RESET}\n");
                return None;
            }
            match load_pipeline_from_file(rest) {
                Some(def) => {
                    println!(
                        "{BOLD}  ▶ 파이프라인 '{rest}' 실행 ({} 단계){RESET}",
                        def.steps.len()
                    );
                    for (i, step) in def.steps.iter().enumerate() {
                        println!("{DIM}    {}. /{}{RESET}", i + 1, step);
                    }
                    println!();
                    Some(build_pipeline_run_prompt(&def))
                }
                None => {
                    eprintln!(
                        "{RED}  파이프라인 '{rest}'을(를) 찾을 수 없습니다.{RESET}\n"
                    );
                    None
                }
            }
        }
        other => {
            println!("{RED}  알 수 없는 하위 명령: {other}{RESET}");
            println!("{DIM}  사용 가능: save, run, list, show, remove{RESET}\n");
            None
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────




#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::commands_project::*;
    use crate::commands_research::*;
    use crate::commands_writing::*;
    use crate::commands_workflow::*;

    fn save_pipeline_to_dir(
        dir: &std::path::Path,
        name: &str,
        steps: &[String],
    ) -> std::io::Result<()> {
        let _ = std::fs::create_dir_all(dir);
        let def = PipelineDef {
            name: name.to_string(),
            steps: steps.to_vec(),
            created: "2026-03-22T16:00:00".to_string(),
        };
        let json = serde_json::to_string_pretty(&def)?;
        std::fs::write(dir.join(format!("{name}.json")), json)
    }

    fn load_pipeline_from_dir(dir: &std::path::Path, name: &str) -> Option<PipelineDef> {
        let path = dir.join(format!("{name}.json"));
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn temp_deadlines_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("deadlines.json");
        (dir, path)
    }

    fn temp_embargoes_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("embargoes.json");
        (dir, path)
    }

    fn temp_desk_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("desk").join("assignments.json");
        (dir, path)
    }

    fn temp_collab_dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    fn temp_coverage_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("coverage.json");
        (dir, path)
    }

    fn temp_calendar_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("calendar.json");
        (dir, path)
    }

    fn temp_performance_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("performance.json");
        (dir, path)
    }


    #[test]
    fn briefing_prompt_with_text() {
        let prompt = build_briefing_prompt("삼성전자가 새로운 반도체를 발표했다");
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("역피라미드"));
        assert!(prompt.contains("[확인 필요]"));
        assert!(prompt.contains("삼성전자가 새로운 반도체를 발표했다"));
    }

    #[test]
    fn briefing_prompt_empty_returns_none() {
        assert!(build_briefing_prompt("").is_none());
        assert!(build_briefing_prompt("   ").is_none());
    }

    #[test]
    fn briefing_parse_args_inline() {
        let (file, text) = parse_briefing_args("삼성전자 보도자료 내용");
        assert!(file.is_none());
        assert_eq!(text, "삼성전자 보도자료 내용");
    }

    #[test]
    fn briefing_parse_args_file() {
        let (file, text) = parse_briefing_args("--file press.txt");
        assert_eq!(file.as_deref(), Some("press.txt"));
        assert_eq!(text, "");
    }

    #[test]
    fn briefing_parse_args_file_with_extra() {
        let (file, text) = parse_briefing_args("--file press.txt 추가 지시사항");
        assert_eq!(file.as_deref(), Some("press.txt"));
        assert_eq!(text, "추가 지시사항");
    }

    #[test]
    fn briefing_parse_args_file_empty() {
        let (file, text) = parse_briefing_args("--file");
        assert!(file.is_none());
        assert_eq!(text, "");
    }

    #[test]
    fn briefing_draft_path_with_slug() {
        let path = briefing_draft_path_with_date("보도자료", "2026-03-18");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/drafts/2026-03-18_보도자료.md"
        );
    }

    #[test]
    fn briefing_draft_path_empty_slug() {
        let path = briefing_draft_path_with_date("", "2026-03-18");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/drafts/2026-03-18_briefing.md"
        );
    }

    #[test]
    fn briefing_file_read_integration() {
        let dir = tempfile::TempDir::new().unwrap();
        let press_file = dir.path().join("press.txt");
        std::fs::write(&press_file, "보도자료 내용입니다").unwrap();
        let content = std::fs::read_to_string(&press_file).unwrap();
        assert_eq!(content, "보도자료 내용입니다");
        let prompt = build_briefing_prompt(&content);
        assert!(prompt.is_some());
        assert!(prompt.unwrap().contains("보도자료 내용입니다"));
    }

    #[test]
    fn interview_file_path_with_topic() {
        let path = interview_file_path_with_date("반도체 수출 규제", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/interview/2026-03-18_반도체-수출-규제.md")
        );
    }

    #[test]
    fn interview_file_path_empty_topic() {
        let path = interview_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/interview/2026-03-18_interview.md")
        );
    }

    #[test]
    fn parse_interview_args_topic_only() {
        let (topic, source) = parse_interview_args("반도체 수출 규제");
        assert_eq!(topic, "반도체 수출 규제");
        assert!(source.is_none());
    }

    #[test]
    fn parse_interview_args_with_source() {
        let (topic, source) = parse_interview_args("반도체 수출 규제 --source 김철수");
        assert_eq!(topic, "반도체 수출 규제");
        assert_eq!(source, Some("김철수".to_string()));
    }

    #[test]
    fn parse_interview_args_empty() {
        let (topic, source) = parse_interview_args("");
        assert!(topic.is_empty());
        assert!(source.is_none());
    }

    #[test]
    fn parse_interview_args_source_only() {
        let (topic, source) = parse_interview_args("--source 김철수");
        assert!(topic.is_empty());
        assert_eq!(source, Some("김철수".to_string()));
    }

    #[test]
    fn build_interview_prompt_with_topic() {
        let prompt = build_interview_prompt("AI 규제", None, &[]);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("AI 규제"));
        assert!(p.contains("도입 질문"));
        assert!(p.contains("핵심 질문"));
        assert!(p.contains("팔로업 질문"));
        assert!(p.contains("마무리 질문"));
    }

    #[test]
    fn build_interview_prompt_empty_topic() {
        let prompt = build_interview_prompt("", None, &[]);
        assert!(prompt.is_none());
    }

    #[test]
    fn build_interview_prompt_with_source() {
        let source = serde_json::json!({
            "name": "김철수",
            "org": "산업통상자원부",
            "beat": "통상",
            "note": "반도체 정책 담당"
        });
        let prompt = build_interview_prompt("반도체 수출", Some(&source), &[]);
        let p = prompt.unwrap();
        assert!(p.contains("김철수"));
        assert!(p.contains("산업통상자원부"));
        assert!(p.contains("통상"));
        assert!(p.contains("반도체 정책 담당"));
    }

    #[test]
    fn build_interview_prompt_with_research() {
        let research = vec![
            ("2026-03-17_반도체.md".to_string(), "반도체 시장 동향 내용".to_string()),
        ];
        let prompt = build_interview_prompt("반도체", None, &research);
        let p = prompt.unwrap();
        assert!(p.contains("관련 리서치 자료"));
        assert!(p.contains("반도체 시장 동향 내용"));
    }

    #[test]
    fn find_source_by_name_in_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let sources_path = dir.path().join("sources.json");
        let sources = serde_json::json!([
            {"name": "김철수", "org": "산업부", "contact": "010-1234", "note": ""},
            {"name": "이영희", "org": "기재부", "contact": "010-5678", "note": ""}
        ]);
        std::fs::write(&sources_path, serde_json::to_string(&sources).unwrap()).unwrap();

        let found = find_source_by_name_in("김철수", &sources_path);
        assert!(found.is_some());
        assert_eq!(found.unwrap()["name"].as_str().unwrap(), "김철수");
    }

    #[test]
    fn find_source_by_name_in_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let sources_path = dir.path().join("sources.json");
        let sources = serde_json::json!([
            {"name": "김철수", "org": "산업부", "contact": "010-1234", "note": ""}
        ]);
        std::fs::write(&sources_path, serde_json::to_string(&sources).unwrap()).unwrap();

        let found = find_source_by_name_in("박지성", &sources_path);
        assert!(found.is_none());
    }

    #[test]
    fn find_source_by_name_partial_match() {
        let dir = tempfile::TempDir::new().unwrap();
        let sources_path = dir.path().join("sources.json");
        let sources = serde_json::json!([
            {"name": "김철수 과장", "org": "산업부", "contact": "010-1234", "note": ""}
        ]);
        std::fs::write(&sources_path, serde_json::to_string(&sources).unwrap()).unwrap();

        let found = find_source_by_name_in("김철수", &sources_path);
        assert!(found.is_some());
    }

    #[test]
    fn save_interview_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("interview").join("test.md");
        let result = save_interview(&path, "# 인터뷰 질문지\n\n1. 질문");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("인터뷰 질문지"));
    }

    #[test]
    fn parse_compare_args_two_files() {
        let (a, b) = parse_compare_args("draft_v1.md draft_v2.md");
        assert_eq!(a.as_deref(), Some("draft_v1.md"));
        assert_eq!(b.as_deref(), Some("draft_v2.md"));
    }

    #[test]
    fn parse_compare_args_one_file() {
        let (a, b) = parse_compare_args("draft_v1.md");
        assert_eq!(a.as_deref(), Some("draft_v1.md"));
        assert!(b.is_none());
    }

    #[test]
    fn parse_compare_args_empty() {
        let (a, b) = parse_compare_args("");
        assert!(a.is_none());
        assert!(b.is_none());
    }

    #[test]
    fn build_compare_prompt_contains_both_contents() {
        let prompt = build_compare_prompt("기사 내용 1", "v1.md", "기사 내용 2", "v2.md");
        assert!(prompt.contains("기사 내용 1"));
        assert!(prompt.contains("기사 내용 2"));
        assert!(prompt.contains("v1.md"));
        assert!(prompt.contains("v2.md"));
        assert!(prompt.contains("사실(팩트) 변경"));
        assert!(prompt.contains("톤/논조 변화"));
        assert!(prompt.contains("출처/인용 변경"));
        assert!(prompt.contains("법적/윤리적 리스크"));
    }

    #[test]
    fn timeline_file_path_with_topic() {
        let path = timeline_file_path_with_date("후쿠시마 오염수 방류", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/timeline/2026-03-18_후쿠시마-오염수-방류.md")
        );
    }

    #[test]
    fn timeline_file_path_empty_topic() {
        let path = timeline_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/timeline/2026-03-18_timeline.md")
        );
    }

    #[test]
    fn save_timeline_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("timeline").join("test.md");
        let result = save_timeline(&path, "# 타임라인\n\n| 날짜 | 사건 |");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("타임라인"));
    }

    #[test]
    fn build_timeline_prompt_contains_topic() {
        let prompt = build_timeline_prompt("반도체 수출 규제", &[]);
        assert!(prompt.contains("반도체 수출 규제"));
        assert!(prompt.contains("시간순 이벤트 타임라인"));
        assert!(prompt.contains("날짜"));
        assert!(prompt.contains("사건"));
        assert!(prompt.contains("출처"));
    }

    #[test]
    fn build_timeline_prompt_includes_research() {
        let research = vec![
            ("2026-03-17_반도체.md".to_string(), "리서치 내용 1".to_string()),
            ("2026-03-16_수출.md".to_string(), "리서치 내용 2".to_string()),
        ];
        let prompt = build_timeline_prompt("반도체 수출", &research);
        assert!(prompt.contains("리서치 내용 1"));
        assert!(prompt.contains("리서치 내용 2"));
        assert!(prompt.contains("기존 리서치 자료"));
    }

    #[test]
    fn build_timeline_prompt_no_research_section_when_empty() {
        let prompt = build_timeline_prompt("테스트 주제", &[]);
        assert!(!prompt.contains("기존 리서치 자료"));
    }

    #[test]
    fn deadline_load_empty_returns_empty() {
        let (_dir, path) = temp_deadlines_path();
        let deadlines = load_deadlines_from(&path);
        assert!(deadlines.is_empty());
    }

    #[test]
    fn deadline_save_and_load_roundtrip() {
        let (_dir, path) = temp_deadlines_path();
        let deadlines = vec![
            Deadline {
                title: "반도체 기사".to_string(),
                datetime: "2026-03-20T18:00:00".to_string(),
            },
            Deadline {
                title: "사설".to_string(),
                datetime: "2026-03-20T09:00:00".to_string(),
            },
        ];
        save_deadlines_to(&deadlines, &path);
        let loaded = load_deadlines_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "반도체 기사");
        assert_eq!(loaded[1].datetime, "2026-03-20T09:00:00");
    }

    #[test]
    fn deadline_parse_time_only() {
        let result = parse_deadline_datetime_with_today("18:00", "2026-03-19");
        assert_eq!(result, Some("2026-03-19T18:00:00".to_string()));
    }

    #[test]
    fn deadline_parse_full_datetime_space() {
        let result = parse_deadline_datetime_with_today("2026-03-20 09:00", "2026-03-19");
        assert_eq!(result, Some("2026-03-20T09:00:00".to_string()));
    }

    #[test]
    fn deadline_parse_full_datetime_t() {
        let result = parse_deadline_datetime_with_today("2026-03-20T09:00", "2026-03-19");
        assert_eq!(result, Some("2026-03-20T09:00:00".to_string()));
    }

    #[test]
    fn deadline_parse_invalid_returns_none() {
        assert!(parse_deadline_datetime_with_today("invalid", "2026-03-19").is_none());
        assert!(parse_deadline_datetime_with_today("", "2026-03-19").is_none());
    }

    #[test]
    fn deadline_datetime_to_epoch_roundtrip() {
        // 2026-03-20T09:00:00 UTC
        let epoch = datetime_to_epoch("2026-03-20T09:00:00");
        assert!(epoch.is_some());
        let e = epoch.unwrap();
        // 2026-03-20 should be > 2025-01-01 epoch
        assert!(e > 1_735_689_600);
    }

    #[test]
    fn deadline_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
    }

    #[test]
    fn deadline_remaining_time_future() {
        // Use a date far in the future
        let (secs, text) = remaining_time("2099-12-31T23:59:00");
        assert!(secs > 0);
        assert!(text.contains("남음"));
    }

    #[test]
    fn deadline_remaining_time_past() {
        let (secs, text) = remaining_time("2020-01-01T00:00:00");
        assert!(secs <= 0);
        assert!(text.contains("초과"));
    }

    #[test]
    fn deadline_clear_removes_entry() {
        let (_dir, path) = temp_deadlines_path();
        let deadlines = vec![
            Deadline {
                title: "기사A".to_string(),
                datetime: "2026-03-20T18:00:00".to_string(),
            },
            Deadline {
                title: "기사B".to_string(),
                datetime: "2026-03-21T09:00:00".to_string(),
            },
        ];
        save_deadlines_to(&deadlines, &path);

        let mut loaded = load_deadlines_from(&path);
        loaded.retain(|d| d.title != "기사A");
        save_deadlines_to(&loaded, &path);

        let final_deadlines = load_deadlines_from(&path);
        assert_eq!(final_deadlines.len(), 1);
        assert_eq!(final_deadlines[0].title, "기사B");
    }

    #[test]
    fn deadline_set_updates_existing() {
        let (_dir, path) = temp_deadlines_path();
        let mut deadlines = vec![Deadline {
            title: "기사A".to_string(),
            datetime: "2026-03-20T18:00:00".to_string(),
        }];
        save_deadlines_to(&deadlines, &path);

        // Simulate update
        if let Some(existing) = deadlines.iter_mut().find(|d| d.title == "기사A") {
            existing.datetime = "2026-03-21T09:00:00".to_string();
        }
        save_deadlines_to(&deadlines, &path);

        let loaded = load_deadlines_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].datetime, "2026-03-21T09:00:00");
    }

    #[test]
    fn embargo_load_empty_returns_empty() {
        let (_dir, path) = temp_embargoes_path();
        let embargoes = load_embargoes_from(&path);
        assert!(embargoes.is_empty());
    }

    #[test]
    fn embargo_save_and_load_roundtrip() {
        let (_dir, path) = temp_embargoes_path();
        let embargoes = vec![
            Embargo {
                title: "보건복지부 의료개혁안".to_string(),
                release_at: "2026-03-21T09:00:00".to_string(),
            },
            Embargo {
                title: "국방부 발표".to_string(),
                release_at: "2026-03-22T14:00:00".to_string(),
            },
        ];
        save_embargoes_to(&embargoes, &path);
        let loaded = load_embargoes_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "보건복지부 의료개혁안");
        assert_eq!(loaded[1].release_at, "2026-03-22T14:00:00");
    }

    #[test]
    fn embargo_clear_by_index() {
        let (_dir, path) = temp_embargoes_path();
        let embargoes = vec![
            Embargo {
                title: "기사A".to_string(),
                release_at: "2026-03-21T09:00:00".to_string(),
            },
            Embargo {
                title: "기사B".to_string(),
                release_at: "2026-03-22T14:00:00".to_string(),
            },
            Embargo {
                title: "기사C".to_string(),
                release_at: "2026-03-23T10:00:00".to_string(),
            },
        ];
        save_embargoes_to(&embargoes, &path);

        // Remove index 2 (기사B)
        let mut loaded = load_embargoes_from(&path);
        loaded.remove(1); // 0-indexed
        save_embargoes_to(&loaded, &path);

        let final_embargoes = load_embargoes_from(&path);
        assert_eq!(final_embargoes.len(), 2);
        assert_eq!(final_embargoes[0].title, "기사A");
        assert_eq!(final_embargoes[1].title, "기사C");
    }

    #[test]
    fn embargo_set_updates_existing() {
        let (_dir, path) = temp_embargoes_path();
        let mut embargoes = vec![Embargo {
            title: "보건복지부 의료개혁안".to_string(),
            release_at: "2026-03-21T09:00:00".to_string(),
        }];
        save_embargoes_to(&embargoes, &path);

        // Update release time
        if let Some(existing) = embargoes
            .iter_mut()
            .find(|e| e.title == "보건복지부 의료개혁안")
        {
            existing.release_at = "2026-03-22T10:00:00".to_string();
        }
        save_embargoes_to(&embargoes, &path);

        let loaded = load_embargoes_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].release_at, "2026-03-22T10:00:00");
    }

    #[test]
    fn embargo_parse_args_quoted_title() {
        let (title, time) =
            parse_embargo_args("\"보건복지부 의료개혁안\" 2026-03-21 09:00");
        assert_eq!(title, "보건복지부 의료개혁안");
        assert_eq!(time, "2026-03-21 09:00");
    }

    #[test]
    fn embargo_parse_args_unquoted_title() {
        let (title, time) = parse_embargo_args("국방부발표 2026-03-22 14:00");
        assert_eq!(title, "국방부발표");
        assert_eq!(time, "2026-03-22 14:00");
    }

    #[test]
    fn embargo_parse_args_time_only() {
        let (title, time) = parse_embargo_args("긴급속보 09:00");
        assert_eq!(title, "긴급속보");
        assert_eq!(time, "09:00");
    }

    #[test]
    fn embargo_color_logic() {
        // Future (>1h) → 🔴 active
        let (secs, _) = remaining_time("2099-12-31T23:59:00");
        assert!(secs > 3600);

        // Past → 🟢 released
        let (secs, _) = remaining_time("2020-01-01T00:00:00");
        assert!(secs <= 0);
    }

    #[test]
    fn data_parse_csv_basic() {
        let csv = "이름, 나이, 점수\n김철수, 30, 85\n이영희, 25, 92\n";
        let (headers, rows) = parse_csv(csv);
        assert_eq!(headers, vec!["이름", "나이", "점수"]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["김철수", "30", "85"]);
    }

    #[test]
    fn data_parse_csv_empty() {
        let (headers, rows) = parse_csv("");
        assert!(headers.is_empty());
        assert!(rows.is_empty());
    }

    #[test]
    fn data_compute_stats_basic() {
        let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let (count, min, max, mean) = compute_column_stats(&values);
        assert_eq!(count, 5);
        assert!((min - 10.0).abs() < f64::EPSILON);
        assert!((max - 50.0).abs() < f64::EPSILON);
        assert!((mean - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn data_compute_stats_empty() {
        let (count, _, _, _) = compute_column_stats(&[]);
        assert_eq!(count, 0);
    }

    #[test]
    fn data_build_csv_summary_has_stats() {
        let csv = "지역, 인구, 면적\n서울, 9700000, 605\n부산, 3400000, 770\n";
        let summary = build_csv_summary(csv);
        assert!(summary.contains("행 수: 2"));
        assert!(summary.contains("열 수: 3"));
        assert!(summary.contains("인구"));
        assert!(summary.contains("면적"));
    }

    #[test]
    fn data_build_csv_summary_missing_values() {
        let csv = "항목, 값\nA, 100\nB, NA\nC, 200\n";
        let summary = build_csv_summary(csv);
        assert!(summary.contains("결측치"));
    }

    #[test]
    fn data_analyze_prompt_contains_angles() {
        let prompt = build_data_analyze_prompt("test.csv", "a,b\n1,2\n");
        assert!(prompt.contains("기사 앵글"));
        assert!(prompt.contains("이상치"));
        assert!(prompt.contains("추세"));
    }

    #[test]
    fn data_compare_prompt_contains_both_files() {
        let prompt = build_data_compare_prompt("a.csv", "x,y\n1,2\n", "b.csv", "x,y\n3,4\n");
        assert!(prompt.contains("a.csv"));
        assert!(prompt.contains("b.csv"));
        assert!(prompt.contains("구조 비교"));
    }

    #[test]
    fn desk_roundtrip_save_load() {
        let (_dir, path) = temp_desk_path();

        let items = vec![
            DeskAssignment {
                reporter: "김기자".to_string(),
                content: "국회 예산안 취재".to_string(),
                deadline: Some("15:00".to_string()),
                status: DeskStatus::Pending,
                feedback: Vec::new(),
                is_pitch: false,
                created_at: "2026-03-20T14:00:00".to_string(),
            },
            DeskAssignment {
                reporter: "이기자".to_string(),
                content: "인사 청문회 정리".to_string(),
                deadline: None,
                status: DeskStatus::Done,
                feedback: vec!["좋은 기사".to_string()],
                is_pitch: false,
                created_at: "2026-03-20T10:00:00".to_string(),
            },
        ];

        save_desk_to(&items, &path);
        let loaded = load_desk_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].reporter, "김기자");
        assert_eq!(loaded[0].content, "국회 예산안 취재");
        assert_eq!(loaded[0].deadline, Some("15:00".to_string()));
        assert_eq!(loaded[0].status, DeskStatus::Pending);
        assert!(loaded[0].feedback.is_empty());
        assert_eq!(loaded[1].status, DeskStatus::Done);
        assert_eq!(loaded[1].feedback.len(), 1);
    }

    #[test]
    fn desk_load_missing_file() {
        let path = std::path::PathBuf::from("/tmp/nonexistent_desk_test_xyz.json");
        let loaded = load_desk_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn desk_assign_and_done() {
        let (_dir, path) = temp_desk_path();

        // Assign a task
        let assignment = DeskAssignment {
            reporter: "박기자".to_string(),
            content: "반도체 실적 취재".to_string(),
            deadline: Some("17:00".to_string()),
            status: DeskStatus::Pending,
            feedback: Vec::new(),
            is_pitch: false,
            created_at: "2026-03-20T14:00:00".to_string(),
        };
        save_desk_to(&[assignment], &path);

        // Mark as done
        let mut loaded = load_desk_from(&path);
        assert_eq!(loaded[0].status, DeskStatus::Pending);
        loaded[0].status = DeskStatus::Done;
        save_desk_to(&loaded, &path);

        let reloaded = load_desk_from(&path);
        assert_eq!(reloaded[0].status, DeskStatus::Done);
    }

    #[test]
    fn desk_feedback_appends() {
        let (_dir, path) = temp_desk_path();

        let assignment = DeskAssignment {
            reporter: "최기자".to_string(),
            content: "환율 동향 분석".to_string(),
            deadline: None,
            status: DeskStatus::Pending,
            feedback: Vec::new(),
            is_pitch: false,
            created_at: "2026-03-20T10:00:00".to_string(),
        };
        save_desk_to(&[assignment], &path);

        let mut loaded = load_desk_from(&path);
        loaded[0].feedback.push("수치 확인 필요".to_string());
        loaded[0].feedback.push("그래프 추가".to_string());
        save_desk_to(&loaded, &path);

        let reloaded = load_desk_from(&path);
        assert_eq!(reloaded[0].feedback.len(), 2);
        assert_eq!(reloaded[0].feedback[0], "수치 확인 필요");
        assert_eq!(reloaded[0].feedback[1], "그래프 추가");
    }

    #[test]
    fn desk_pitch_flag() {
        let (_dir, path) = temp_desk_path();

        let pitch = DeskAssignment {
            reporter: "제안".to_string(),
            content: "[AI 규제] 미국 AI 규제 법안 분석".to_string(),
            deadline: None,
            status: DeskStatus::Pending,
            feedback: Vec::new(),
            is_pitch: true,
            created_at: "2026-03-20T11:00:00".to_string(),
        };
        save_desk_to(&[pitch], &path);

        let loaded = load_desk_from(&path);
        assert!(loaded[0].is_pitch);
        assert_eq!(loaded[0].reporter, "제안");
    }

    #[test]
    fn desk_parse_assign_args_basic() {
        let result = parse_desk_assign_args("김기자 국회 취재");
        assert!(result.is_some());
        let (reporter, content, deadline) = result.unwrap();
        assert_eq!(reporter, "김기자");
        assert_eq!(content, "국회 취재");
        assert!(deadline.is_none());
    }

    #[test]
    fn desk_parse_assign_args_with_deadline() {
        let result = parse_desk_assign_args("이기자 반도체 취재 --deadline 15:30");
        assert!(result.is_some());
        let (reporter, content, deadline) = result.unwrap();
        assert_eq!(reporter, "이기자");
        assert_eq!(content, "반도체 취재");
        assert_eq!(deadline, Some("15:30".to_string()));
    }

    #[test]
    fn desk_parse_assign_args_missing_content() {
        let result = parse_desk_assign_args("김기자");
        assert!(result.is_none());
    }

    #[test]
    fn is_valid_time_checks() {
        assert!(is_valid_time("00:00"));
        assert!(is_valid_time("23:59"));
        assert!(is_valid_time("15:30"));
        assert!(!is_valid_time("24:00"));
        assert!(!is_valid_time("12:60"));
        assert!(!is_valid_time("1:30"));
        assert!(!is_valid_time("abc"));
        assert!(!is_valid_time("12345"));
    }

    #[test]
    fn collab_start_creates_project() {
        let dir = temp_collab_dir();
        let path = collab_project_path_in(dir.path(), "반도체취재");

        let project = CollabProject {
            name: "반도체취재".to_string(),
            reporters: vec!["김기자".to_string(), "이기자".to_string()],
            notes: Vec::new(),
            status: CollabStatus::Active,
            created_at: "2026-03-20T14:00:00".to_string(),
        };
        save_collab_project_to(&project, &path);

        let loaded = load_collab_project_from(&path).unwrap();
        assert_eq!(loaded.name, "반도체취재");
        assert_eq!(loaded.reporters.len(), 2);
        assert_eq!(loaded.status, CollabStatus::Active);
        assert!(loaded.notes.is_empty());
    }

    #[test]
    fn collab_note_adds_entry() {
        let dir = temp_collab_dir();
        let path = collab_project_path_in(dir.path(), "국회취재");

        let mut project = CollabProject {
            name: "국회취재".to_string(),
            reporters: vec!["박기자".to_string()],
            notes: Vec::new(),
            status: CollabStatus::Active,
            created_at: "2026-03-20T10:00:00".to_string(),
        };
        save_collab_project_to(&project, &path);

        // Add a note
        let note = CollabNote {
            reporter: "박기자".to_string(),
            content: "법안 소위 통과 확인".to_string(),
            timestamp: "2026-03-20T11:00:00".to_string(),
        };
        project.notes.push(note);
        save_collab_project_to(&project, &path);

        let loaded = load_collab_project_from(&path).unwrap();
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0].reporter, "박기자");
        assert_eq!(loaded.notes[0].content, "법안 소위 통과 확인");
    }

    #[test]
    fn collab_close_marks_closed() {
        let dir = temp_collab_dir();
        let path = collab_project_path_in(dir.path(), "경제분석");

        let mut project = CollabProject {
            name: "경제분석".to_string(),
            reporters: Vec::new(),
            notes: Vec::new(),
            status: CollabStatus::Active,
            created_at: "2026-03-20T09:00:00".to_string(),
        };
        save_collab_project_to(&project, &path);

        project.status = CollabStatus::Closed;
        save_collab_project_to(&project, &path);

        let loaded = load_collab_project_from(&path).unwrap();
        assert_eq!(loaded.status, CollabStatus::Closed);
    }

    #[test]
    fn collab_list_shows_active_only() {
        let dir = temp_collab_dir();

        let active = CollabProject {
            name: "활성프로젝트".to_string(),
            reporters: Vec::new(),
            notes: Vec::new(),
            status: CollabStatus::Active,
            created_at: "2026-03-20T08:00:00".to_string(),
        };
        save_collab_project_to(&active, &collab_project_path_in(dir.path(), "활성프로젝트"));

        let closed = CollabProject {
            name: "종료프로젝트".to_string(),
            reporters: Vec::new(),
            notes: Vec::new(),
            status: CollabStatus::Closed,
            created_at: "2026-03-20T07:00:00".to_string(),
        };
        save_collab_project_to(&closed, &collab_project_path_in(dir.path(), "종료프로젝트"));

        let all = list_collab_projects_in(dir.path());
        assert_eq!(all.len(), 2);
        let active_count = all.iter().filter(|p| p.status == CollabStatus::Active).count();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn collab_parse_start_args() {
        let (name, reporters) = parse_collab_start_args("반도체 --reporters 김기자,이기자");
        assert_eq!(name, "반도체");
        assert_eq!(reporters, vec!["김기자", "이기자"]);
    }

    #[test]
    fn collab_parse_start_args_no_reporters() {
        let (name, reporters) = parse_collab_start_args("국회취재");
        assert_eq!(name, "국회취재");
        assert!(reporters.is_empty());
    }

    #[test]
    fn collab_parse_note_args() {
        let result = parse_collab_note_args("반도체 삼성 공장 가동률 확인 --reporter 김기자");
        assert!(result.is_some());
        let (project, content, reporter) = result.unwrap();
        assert_eq!(project, "반도체");
        assert_eq!(content, "삼성 공장 가동률 확인");
        assert_eq!(reporter, "김기자");
    }

    #[test]
    fn collab_parse_note_args_no_reporter() {
        let result = parse_collab_note_args("반도체 취재 메모 내용");
        assert!(result.is_some());
        let (project, content, reporter) = result.unwrap();
        assert_eq!(project, "반도체");
        assert_eq!(content, "취재 메모 내용");
        assert!(reporter.is_empty());
    }

    #[test]
    fn collab_parse_note_args_missing_content() {
        let result = parse_collab_note_args("반도체");
        assert!(result.is_none());
    }

    #[test]
    fn collab_multiple_notes_preserve_order() {
        let dir = temp_collab_dir();
        let path = collab_project_path_in(dir.path(), "순서테스트");

        let mut project = CollabProject {
            name: "순서테스트".to_string(),
            reporters: vec!["A기자".to_string(), "B기자".to_string()],
            notes: Vec::new(),
            status: CollabStatus::Active,
            created_at: "2026-03-20T08:00:00".to_string(),
        };

        for i in 1..=3 {
            project.notes.push(CollabNote {
                reporter: format!("기자{i}"),
                content: format!("메모 {i}"),
                timestamp: format!("2026-03-20T{:02}:00:00", 8 + i),
            });
        }
        save_collab_project_to(&project, &path);

        let loaded = load_collab_project_from(&path).unwrap();
        assert_eq!(loaded.notes.len(), 3);
        assert_eq!(loaded.notes[0].content, "메모 1");
        assert_eq!(loaded.notes[2].content, "메모 3");
    }

    #[test]
    fn coverage_claim_and_load() {
        let (_dir, path) = temp_coverage_path();
        let claims = load_coverage_from(&path);
        assert!(claims.is_empty());

        let mut claims = Vec::new();
        claims.push(CoverageClaim {
            topic: "국회 본회의".to_string(),
            reporter: "김기자".to_string(),
            until: Some("18:00".to_string()),
            active: true,
            created_at: "2026-03-20T14:00:00".to_string(),
        });
        save_coverage_to(&claims, &path);

        let loaded = load_coverage_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].topic, "국회 본회의");
        assert_eq!(loaded[0].reporter, "김기자");
        assert!(loaded[0].active);
        assert_eq!(loaded[0].until, Some("18:00".to_string()));
    }

    #[test]
    fn coverage_release_deactivates() {
        let (_dir, path) = temp_coverage_path();

        let mut claims = vec![
            CoverageClaim {
                topic: "반도체 실적".to_string(),
                reporter: "이기자".to_string(),
                until: None,
                active: true,
                created_at: "2026-03-20T10:00:00".to_string(),
            },
            CoverageClaim {
                topic: "환율 동향".to_string(),
                reporter: "박기자".to_string(),
                until: Some("17:00".to_string()),
                active: true,
                created_at: "2026-03-20T11:00:00".to_string(),
            },
        ];
        save_coverage_to(&claims, &path);

        // Release first claim
        claims[0].active = false;
        save_coverage_to(&claims, &path);

        let loaded = load_coverage_from(&path);
        assert!(!loaded[0].active);
        assert!(loaded[1].active);
    }

    #[test]
    fn coverage_expire_claims() {
        let mut claims = vec![
            CoverageClaim {
                topic: "속보1".to_string(),
                reporter: "A".to_string(),
                until: Some("14:00".to_string()),
                active: true,
                created_at: "2026-03-20T13:00:00".to_string(),
            },
            CoverageClaim {
                topic: "속보2".to_string(),
                reporter: "B".to_string(),
                until: Some("20:00".to_string()),
                active: true,
                created_at: "2026-03-20T13:00:00".to_string(),
            },
            CoverageClaim {
                topic: "속보3".to_string(),
                reporter: "C".to_string(),
                until: None,
                active: true,
                created_at: "2026-03-20T13:00:00".to_string(),
            },
        ];

        let expired = expire_claims(&mut claims, "15:00");
        assert_eq!(expired, 1); // Only 속보1 (14:00) expired
        assert!(!claims[0].active);
        assert!(claims[1].active);
        assert!(claims[2].active); // No until → never expires
    }

    #[test]
    fn coverage_check_keyword_match() {
        let (_dir, path) = temp_coverage_path();

        let claims = vec![
            CoverageClaim {
                topic: "국회 본회의 표결".to_string(),
                reporter: "김기자".to_string(),
                until: None,
                active: true,
                created_at: "2026-03-20T14:00:00".to_string(),
            },
            CoverageClaim {
                topic: "반도체 실적 발표".to_string(),
                reporter: "이기자".to_string(),
                until: None,
                active: true,
                created_at: "2026-03-20T14:00:00".to_string(),
            },
            CoverageClaim {
                topic: "환율 동향".to_string(),
                reporter: "박기자".to_string(),
                until: None,
                active: false, // inactive
                created_at: "2026-03-20T14:00:00".to_string(),
            },
        ];
        save_coverage_to(&claims, &path);

        let loaded = load_coverage_from(&path);
        let keyword = "국회";
        let keyword_lower = keyword.to_lowercase();
        let matches: Vec<&CoverageClaim> = loaded
            .iter()
            .filter(|c| c.active && c.topic.to_lowercase().contains(&keyword_lower))
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].reporter, "김기자");

        // Inactive claims should not match
        let keyword2 = "환율";
        let keyword2_lower = keyword2.to_lowercase();
        let matches2: Vec<&CoverageClaim> = loaded
            .iter()
            .filter(|c| c.active && c.topic.to_lowercase().contains(&keyword2_lower))
            .collect();
        assert!(matches2.is_empty());
    }

    #[test]
    fn coverage_parse_claim_args_full() {
        let (topic, reporter, until) =
            parse_coverage_claim_args("국회 본회의 --reporter 김기자 --until 18:00");
        assert_eq!(topic, "국회 본회의");
        assert_eq!(reporter, "김기자");
        assert_eq!(until, Some("18:00".to_string()));
    }

    #[test]
    fn coverage_parse_claim_args_topic_only() {
        let (topic, reporter, until) = parse_coverage_claim_args("반도체 실적");
        assert_eq!(topic, "반도체 실적");
        assert!(reporter.is_empty());
        assert!(until.is_none());
    }

    #[test]
    fn coverage_parse_claim_args_with_reporter_only() {
        let (topic, reporter, until) =
            parse_coverage_claim_args("환율 --reporter 박기자");
        assert_eq!(topic, "환율");
        assert_eq!(reporter, "박기자");
        assert!(until.is_none());
    }

    #[test]
    fn coverage_time_diff_minutes() {
        assert_eq!(time_diff_minutes("18:00", "14:00"), Some(240));
        assert_eq!(time_diff_minutes("14:30", "14:00"), Some(30));
        assert_eq!(time_diff_minutes("13:00", "14:00"), Some(-60));
    }

    #[test]
    fn coverage_is_claim_expired_checks() {
        let claim_with_until = CoverageClaim {
            topic: "test".to_string(),
            reporter: "r".to_string(),
            until: Some("15:00".to_string()),
            active: true,
            created_at: "".to_string(),
        };
        assert!(is_claim_expired(&claim_with_until, "15:00"));
        assert!(is_claim_expired(&claim_with_until, "16:00"));
        assert!(!is_claim_expired(&claim_with_until, "14:59"));

        let claim_no_until = CoverageClaim {
            topic: "test".to_string(),
            reporter: "r".to_string(),
            until: None,
            active: true,
            created_at: "".to_string(),
        };
        assert!(!is_claim_expired(&claim_no_until, "23:59"));
    }

    #[test]
    fn dashboard_empty_state_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let deadlines = tmp.path().join("deadlines.json");
        let embargoes = tmp.path().join("embargoes.json");
        let desk = tmp.path().join("desk.json");
        let followups = tmp.path().join("followups.json");
        let collab_dir = tmp.path().join("collab");
        let coverage = tmp.path().join("coverage.json");
        // Should not panic with no files
        handle_dashboard_impl(
            &deadlines,
            &embargoes,
            &desk,
            &followups,
            &collab_dir,
            &coverage,
        );
    }

    #[test]
    fn dashboard_with_data() {
        let tmp = tempfile::tempdir().unwrap();
        // Write deadline
        let deadlines_path = tmp.path().join("deadlines.json");
        let dls = vec![Deadline {
            title: "석간 마감".to_string(),
            datetime: "2026-03-21T15:00:00".to_string(),
        }];
        save_deadlines_to(&dls, &deadlines_path);

        // Write embargo
        let embargoes_path = tmp.path().join("embargoes.json");
        let ems = vec![Embargo {
            title: "정부 발표".to_string(),
            release_at: "2026-03-22T09:00:00".to_string(),
        }];
        save_embargoes_to(&ems, &embargoes_path);

        // Write desk assignment
        let desk_path = tmp.path().join("desk.json");
        let assigns = vec![DeskAssignment {
            reporter: "김기자".to_string(),
            content: "취재 나가세요".to_string(),
            deadline: Some("18:00".to_string()),
            status: DeskStatus::Pending,
            feedback: vec![],
            is_pitch: false,
            created_at: "2026-03-21T09:00:00".to_string(),
        }];
        save_desk_to(&assigns, &desk_path);

        // Write followup
        let followups_path = tmp.path().join("followups.json");
        let fups = vec![Followup {
            topic: "후속 기사".to_string(),
            due: Some("2026-03-22".to_string()),
            done: false,
            created_at: "2026-03-21T09:00:00".to_string(),
        }];
        save_followups_to(&fups, &followups_path);

        // Write coverage
        let coverage_path = tmp.path().join("coverage.json");
        let claims = vec![CoverageClaim {
            topic: "환율".to_string(),
            reporter: "박기자".to_string(),
            until: Some("18:00".to_string()),
            active: true,
            created_at: "2026-03-21T09:00:00".to_string(),
        }];
        save_coverage_to(&claims, &coverage_path);

        let collab_dir = tmp.path().join("collab");
        std::fs::create_dir_all(&collab_dir).unwrap();
        let proj = CollabProject {
            name: "공동취재1".to_string(),
            reporters: vec!["기자A".to_string(), "기자B".to_string()],
            notes: vec![],
            status: CollabStatus::Active,
            created_at: "2026-03-21T09:00:00".to_string(),
        };
        save_collab_project_to(&proj, &collab_dir.join("공동취재1.json"));

        // Should not panic with populated data
        handle_dashboard_impl(
            &deadlines_path,
            &embargoes_path,
            &desk_path,
            &followups_path,
            &collab_dir,
            &coverage_path,
        );
    }

    #[test]
    fn calendar_load_empty_returns_empty() {
        let (_dir, path) = temp_calendar_path();
        let events = load_calendar_from(&path);
        assert!(events.is_empty());
    }

    #[test]
    fn calendar_save_and_load_roundtrip() {
        let (_dir, path) = temp_calendar_path();
        let events = vec![
            CalendarEvent {
                id: 1,
                date: "2026-03-25".to_string(),
                time: "14:00".to_string(),
                description: "기자간담회".to_string(),
                done: false,
            },
            CalendarEvent {
                id: 2,
                date: "2026-03-26".to_string(),
                time: "10:00".to_string(),
                description: "국회 일정".to_string(),
                done: false,
            },
        ];
        save_calendar_to(&events, &path);
        let loaded = load_calendar_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].description, "기자간담회");
        assert_eq!(loaded[1].date, "2026-03-26");
    }

    #[test]
    fn calendar_next_id() {
        let events = vec![
            CalendarEvent {
                id: 1,
                date: "2026-03-25".to_string(),
                time: "14:00".to_string(),
                description: "A".to_string(),
                done: false,
            },
            CalendarEvent {
                id: 5,
                date: "2026-03-26".to_string(),
                time: "10:00".to_string(),
                description: "B".to_string(),
                done: false,
            },
        ];
        assert_eq!(next_calendar_id(&events), 6);
        assert_eq!(next_calendar_id(&[]), 1);
    }

    #[test]
    fn calendar_parse_date_valid() {
        assert_eq!(
            parse_calendar_date("2026-03-25"),
            Some("2026-03-25".to_string())
        );
    }

    #[test]
    fn calendar_parse_date_invalid() {
        assert!(parse_calendar_date("2026-13-01").is_none());
        assert!(parse_calendar_date("2026-00-01").is_none());
        assert!(parse_calendar_date("2026-01-00").is_none());
        assert!(parse_calendar_date("invalid").is_none());
        assert!(parse_calendar_date("").is_none());
    }

    #[test]
    fn calendar_parse_time_valid() {
        assert_eq!(parse_calendar_time("14:00"), Some("14:00".to_string()));
        assert_eq!(parse_calendar_time("09:30"), Some("09:30".to_string()));
        assert_eq!(parse_calendar_time("0:00"), Some("00:00".to_string()));
    }

    #[test]
    fn calendar_parse_time_invalid() {
        assert!(parse_calendar_time("25:00").is_none());
        assert!(parse_calendar_time("12:60").is_none());
        assert!(parse_calendar_time("invalid").is_none());
        assert!(parse_calendar_time("").is_none());
    }

    #[test]
    fn calendar_done_marks_event() {
        let (_dir, path) = temp_calendar_path();
        let events = vec![CalendarEvent {
            id: 1,
            date: "2026-03-25".to_string(),
            time: "14:00".to_string(),
            description: "테스트".to_string(),
            done: false,
        }];
        save_calendar_to(&events, &path);

        let mut loaded = load_calendar_from(&path);
        if let Some(e) = loaded.iter_mut().find(|e| e.id == 1) {
            e.done = true;
        }
        save_calendar_to(&loaded, &path);

        let final_events = load_calendar_from(&path);
        assert!(final_events[0].done);
    }

    #[test]
    fn calendar_remove_deletes_event() {
        let (_dir, path) = temp_calendar_path();
        let events = vec![
            CalendarEvent {
                id: 1,
                date: "2026-03-25".to_string(),
                time: "14:00".to_string(),
                description: "A".to_string(),
                done: false,
            },
            CalendarEvent {
                id: 2,
                date: "2026-03-26".to_string(),
                time: "10:00".to_string(),
                description: "B".to_string(),
                done: false,
            },
        ];
        save_calendar_to(&events, &path);

        let mut loaded = load_calendar_from(&path);
        loaded.retain(|e| e.id != 1);
        save_calendar_to(&loaded, &path);

        let final_events = load_calendar_from(&path);
        assert_eq!(final_events.len(), 1);
        assert_eq!(final_events[0].id, 2);
    }

    #[test]
    fn calendar_sort_by_date_then_time() {
        let mut events = vec![
            CalendarEvent {
                id: 1,
                date: "2026-03-26".to_string(),
                time: "10:00".to_string(),
                description: "B".to_string(),
                done: false,
            },
            CalendarEvent {
                id: 2,
                date: "2026-03-25".to_string(),
                time: "14:00".to_string(),
                description: "A".to_string(),
                done: false,
            },
            CalendarEvent {
                id: 3,
                date: "2026-03-25".to_string(),
                time: "09:00".to_string(),
                description: "C".to_string(),
                done: false,
            },
        ];
        events.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));
        assert_eq!(events[0].description, "C");
        assert_eq!(events[1].description, "A");
        assert_eq!(events[2].description, "B");
    }

    #[test]
    fn calendar_date_color_today_is_red() {
        assert_eq!(date_color_index("2026-03-21", "2026-03-21"), 0);
    }

    #[test]
    fn calendar_date_color_tomorrow_is_yellow() {
        assert_eq!(date_color_index("2026-03-22", "2026-03-21"), 1);
    }

    #[test]
    fn calendar_date_color_past_is_dim() {
        assert_eq!(date_color_index("2026-03-20", "2026-03-21"), 2);
    }

    #[test]
    fn calendar_date_color_future_is_none() {
        assert_eq!(date_color_index("2026-03-25", "2026-03-21"), 3);
    }

    #[test]
    fn calendar_next_day_basic() {
        assert_eq!(next_day("2026-03-21"), Some("2026-03-22".to_string()));
        assert_eq!(next_day("2026-03-31"), Some("2026-04-01".to_string()));
        assert_eq!(next_day("2026-12-31"), Some("2027-01-01".to_string()));
        assert_eq!(next_day("2024-02-28"), Some("2024-02-29".to_string())); // leap
        assert_eq!(next_day("2024-02-29"), Some("2024-03-01".to_string()));
        assert_eq!(next_day("2026-02-28"), Some("2026-03-01".to_string())); // non-leap
    }

    #[test]
    fn calendar_week_start_and_end() {
        // 2026-03-21 is a Saturday
        let mon = week_start("2026-03-21");
        let sun = week_end("2026-03-21");
        assert_eq!(mon, Some("2026-03-16".to_string()));
        assert_eq!(sun, Some("2026-03-22".to_string()));
    }

    #[test]
    fn calendar_day_of_week() {
        // 2026-03-21 is a Saturday = 5 (0=Mon)
        assert_eq!(day_of_week("2026-03-21"), Some(5));
        // 2026-03-16 is a Monday = 0
        assert_eq!(day_of_week("2026-03-16"), Some(0));
    }

    #[test]
    fn performance_add_creates_entry() {
        let (_dir, path) = temp_performance_path();
        performance_add("테스트기사 --views 100 --comments 5 --shares 10", &path);
        let data = load_performance_from(&path);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["title"], "테스트기사");
        assert_eq!(data[0]["views"], 100);
        assert_eq!(data[0]["comments"], 5);
        assert_eq!(data[0]["shares"], 10);
        assert_eq!(data[0]["id"], 1);
    }

    #[test]
    fn performance_add_multi_word_title() {
        let (_dir, path) = temp_performance_path();
        performance_add("반도체 수출 동향 --views 200", &path);
        let data = load_performance_from(&path);
        assert_eq!(data[0]["title"], "반도체 수출 동향");
        assert_eq!(data[0]["views"], 200);
        assert_eq!(data[0]["comments"], 0);
        assert_eq!(data[0]["shares"], 0);
    }

    #[test]
    fn performance_update_modifies_entry() {
        let (_dir, path) = temp_performance_path();
        performance_add("기사A --views 10", &path);
        performance_update("1 --views 500 --comments 20", &path);
        let data = load_performance_from(&path);
        assert_eq!(data[0]["views"], 500);
        assert_eq!(data[0]["comments"], 20);
        assert_eq!(data[0]["shares"], 0); // unchanged
    }

    #[test]
    fn performance_update_out_of_range() {
        let (_dir, path) = temp_performance_path();
        performance_add("기사A --views 10", &path);
        // Should not crash for out-of-range
        performance_update("99 --views 500", &path);
        let data = load_performance_from(&path);
        assert_eq!(data[0]["views"], 10); // unchanged
    }

    #[test]
    fn performance_list_sorted_by_engagement() {
        let (_dir, path) = temp_performance_path();
        performance_add("기사낮음 --views 10", &path);
        performance_add("기사높음 --views 1000 --comments 50 --shares 200", &path);
        performance_add("기사중간 --views 100 --comments 10", &path);
        let data = load_performance_from(&path);
        assert_eq!(data.len(), 3);

        // Verify sorting logic
        let mut sorted: Vec<(usize, &serde_json::Value)> = data.iter().enumerate().collect();
        sorted.sort_by(|a, b| {
            let total_a = a.1["views"].as_u64().unwrap_or(0)
                + a.1["comments"].as_u64().unwrap_or(0)
                + a.1["shares"].as_u64().unwrap_or(0);
            let total_b = b.1["views"].as_u64().unwrap_or(0)
                + b.1["comments"].as_u64().unwrap_or(0)
                + b.1["shares"].as_u64().unwrap_or(0);
            total_b.cmp(&total_a)
        });
        assert_eq!(sorted[0].1["title"], "기사높음");
        assert_eq!(sorted[1].1["title"], "기사중간");
        assert_eq!(sorted[2].1["title"], "기사낮음");
    }

    #[test]
    fn performance_top_finds_best() {
        let (_dir, path) = temp_performance_path();
        performance_add("기사A --views 10", &path);
        performance_add("기사B --views 1000 --shares 500", &path);
        performance_add("기사C --views 100", &path);
        let data = load_performance_from(&path);

        let best = data
            .iter()
            .enumerate()
            .max_by_key(|(_, e)| {
                e["views"].as_u64().unwrap_or(0)
                    + e["comments"].as_u64().unwrap_or(0)
                    + e["shares"].as_u64().unwrap_or(0)
            })
            .unwrap();
        assert_eq!(best.1["title"], "기사B");
    }

    #[test]
    fn performance_json_roundtrip() {
        let (_dir, path) = temp_performance_path();
        performance_add("기사1 --views 100 --comments 10 --shares 5", &path);
        performance_add("기사2 --views 200", &path);
        let data = load_performance_from(&path);
        assert_eq!(data.len(), 2);

        // Re-save and re-load
        save_performance_to(&data, &path);
        let reloaded = load_performance_from(&path);
        assert_eq!(reloaded.len(), 2);
        assert_eq!(reloaded[0]["title"], "기사1");
        assert_eq!(reloaded[1]["title"], "기사2");
    }

    #[test]
    fn performance_report_prompt_contains_data() {
        let data = vec![
            serde_json::json!({"id": 1, "title": "테스트", "date": "2026-03-21", "views": 100, "comments": 5, "shares": 10}),
        ];
        let prompt = performance_report_prompt(&data);
        assert!(prompt.contains("테스트"));
        assert!(prompt.contains("100"));
        assert!(prompt.contains("리포트"));
    }

    #[test]
    fn parse_performance_args_basic() {
        let (title, views, comments, shares) =
            parse_performance_args("기사제목 --views 100 --comments 5 --shares 10");
        assert_eq!(title, "기사제목");
        assert_eq!(views, Some(100));
        assert_eq!(comments, Some(5));
        assert_eq!(shares, Some(10));
    }

    #[test]
    fn parse_performance_args_partial() {
        let (title, views, comments, shares) = parse_performance_args("제목만 있는 경우");
        assert_eq!(title, "제목만 있는 경우");
        assert_eq!(views, None);
        assert_eq!(comments, None);
        assert_eq!(shares, None);
    }

    #[test]
    fn parse_autopitch_args_empty() {
        let (beat, rest) = parse_autopitch_args("");
        assert!(beat.is_none());
        assert!(rest.is_empty());
    }

    #[test]
    fn parse_autopitch_args_beat_only() {
        let (beat, rest) = parse_autopitch_args("--beat 경제");
        assert_eq!(beat, Some("경제".to_string()));
        assert!(rest.is_empty());
    }

    #[test]
    fn parse_autopitch_args_beat_with_extra() {
        let (beat, rest) = parse_autopitch_args("--beat 정치 추가텍스트");
        assert_eq!(beat, Some("정치".to_string()));
        assert_eq!(rest, "추가텍스트");
    }

    #[test]
    fn parse_autopitch_args_no_beat() {
        let (beat, rest) = parse_autopitch_args("그냥 텍스트");
        assert!(beat.is_none());
        assert_eq!(rest, "그냥 텍스트");
    }

    #[test]
    fn build_autopitch_prompt_no_beat() {
        let prompt = build_autopitch_prompt(None, "테스트 데이터");
        assert!(prompt.contains("테스트 데이터"));
        assert!(prompt.contains("미발굴 각도"));
        assert!(prompt.contains("후속 보도"));
        assert!(prompt.contains("시의성"));
        assert!(!prompt.contains("출입처/분야:"));
    }

    #[test]
    fn build_autopitch_prompt_with_beat() {
        let prompt = build_autopitch_prompt(Some("경제"), "데이터");
        assert!(prompt.contains("출입처/분야: 경제"));
        assert!(prompt.contains("데이터"));
    }

    #[test]
    fn collect_journalist_data_empty_dir() {
        // When no .journalist/ subdirectories exist, should return placeholder
        let data = collect_journalist_data();
        // Just verify it returns a non-empty string (may have data or placeholder)
        assert!(!data.is_empty());
    }

    #[test]
    fn pitches_dir_constant() {
        assert_eq!(PITCHES_DIR, ".journalist/pitches");
    }

    #[test]
    fn morning_dir_constant() {
        assert_eq!(MORNING_DIR, ".journalist/morning");
    }

    #[test]
    fn collect_morning_data_graceful_when_empty() {
        // When no .journalist/ data files exist, should still return structured sections
        let data = collect_morning_data();
        assert!(data.contains("오늘 일정"));
        assert!(data.contains("마감 임박"));
        assert!(data.contains("후속보도 리마인드"));
        assert!(data.contains("데스크 지시 대기 건"));
    }

    #[test]
    fn build_morning_prompt_contains_sections() {
        let prompt = build_morning_prompt("테스트 데이터");
        assert!(prompt.contains("테스트 데이터"));
        assert!(prompt.contains("오늘 일정 요약"));
        assert!(prompt.contains("마감 임박 경고"));
        assert!(prompt.contains("후속보도 리마인드"));
        assert!(prompt.contains("데스크 지시 대기 건"));
        assert!(prompt.contains("오늘의 주요 이슈"));
        assert!(prompt.contains("오늘의 추천 액션"));
    }

    #[test]
    fn breaking_dir_constant() {
        assert_eq!(BREAKING_DIR, ".journalist/breaking");
    }

    #[test]
    fn parse_breaking_help() {
        assert!(matches!(parse_breaking_input("/breaking"), BreakingAction::Help));
        assert!(matches!(parse_breaking_input("/breaking help"), BreakingAction::Help));
        assert!(matches!(parse_breaking_input("/breaking --help"), BreakingAction::Help));
    }

    #[test]
    fn parse_breaking_list() {
        assert!(matches!(parse_breaking_input("/breaking list"), BreakingAction::List));
    }

    #[test]
    fn parse_breaking_new() {
        match parse_breaking_input("/breaking 국회 긴급 본회의 소집") {
            BreakingAction::New(topic) => assert_eq!(topic, "국회 긴급 본회의 소집"),
            _ => panic!("expected New"),
        }
    }

    #[test]
    fn parse_breaking_update() {
        match parse_breaking_input("/breaking update 사상자 3명 추가 확인") {
            BreakingAction::Update(info) => assert_eq!(info, "사상자 3명 추가 확인"),
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn parse_breaking_update_empty() {
        match parse_breaking_input("/breaking update") {
            BreakingAction::Update(info) => assert!(info.is_empty()),
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn breaking_file_path_has_slug() {
        let path = breaking_file_path_with_ts("지진 발생", "2026-03-22_103000");
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("2026-03-22_103000_"));
        assert!(name.ends_with(".md"));
        assert!(name.contains("지진"));
    }

    #[test]
    fn breaking_file_path_empty_topic() {
        let path = breaking_file_path_with_ts("", "2026-03-22_103000");
        let name = path.file_name().unwrap().to_string_lossy();
        assert_eq!(name, "2026-03-22_103000_breaking.md");
    }

    #[test]
    fn build_breaking_prompt_contains_topic() {
        let prompt = build_breaking_prompt("반도체 공장 화재");
        assert!(prompt.contains("반도체 공장 화재"));
        assert!(prompt.contains("5W1H"));
        assert!(prompt.contains("역피라미드"));
        assert!(prompt.contains("체크리스트"));
    }

    #[test]
    fn build_breaking_update_prompt_contains_info() {
        let prompt = build_breaking_update_prompt("기존 기사 내용", "사상자 추가");
        assert!(prompt.contains("기존 기사 내용"));
        assert!(prompt.contains("사상자 추가"));
        assert!(prompt.contains("업데이트"));
    }

    #[test]
    fn save_breaking_creates_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        let nested = dir.path().join("sub").join("dir").join("test.md");
        save_breaking(&nested, "test content").unwrap();
        assert_eq!(std::fs::read_to_string(&nested).unwrap(), "test content");
    }

    #[test]
    fn epoch_days_to_ymd_known_date() {
        // 2026-03-22 is 20534 days since epoch (1970-01-01)
        let (y, m, d) = epoch_days_to_ymd(20534);
        assert_eq!(y, 2026);
        assert_eq!(m, 3);
        assert_eq!(d, 22);
    }

    #[test]
    fn list_breaking_files_empty_dir() {
        // Non-existent dir should return empty
        let files = list_breaking_files();
        // This may or may not be empty depending on the test environment,
        // but at minimum it should not panic.
        let _ = files;
    }

    #[test]
    fn recap_dir_constant() {
        assert_eq!(RECAP_DIR, ".journalist/recap");
    }

    #[test]
    fn collect_recap_data_graceful_when_empty() {
        // When no .journalist/ data files exist, should still return structured sections
        let data = collect_recap_data();
        assert!(data.contains("오늘 작성한 메모"));
        assert!(data.contains("오늘 접촉한 취재원"));
        assert!(data.contains("오늘 일정"));
        assert!(data.contains("오늘 작업한 초고"));
        assert!(data.contains("마감 상태"));
        assert!(data.contains("데스크 지시 현황"));
    }

    #[test]
    fn build_recap_prompt_contains_sections() {
        let prompt = build_recap_prompt("테스트 데이터");
        assert!(prompt.contains("테스트 데이터"));
        assert!(prompt.contains("오늘 한 일 요약"));
        assert!(prompt.contains("미완료 사항"));
        assert!(prompt.contains("내일 이월"));
        assert!(prompt.contains("취재 성과"));
        assert!(prompt.contains("내일 우선순위 제안"));
    }

    #[test]
    fn diary_dir_constant() {
        assert_eq!(DIARY_DIR, ".journalist/diary");
    }

    #[test]
    fn collect_diary_data_graceful_when_empty() {
        let data = collect_diary_data();
        assert!(data.contains("취재 메모"));
        assert!(data.contains("취재원 접촉 기록"));
        assert!(data.contains("일정"));
    }

    #[test]
    fn build_diary_prompt_official_format() {
        let prompt = build_diary_prompt("테스트 데이터", "official");
        assert!(prompt.contains("테스트 데이터"));
        assert!(prompt.contains("공식 취재 일지"));
        assert!(prompt.contains("취재처"));
        assert!(prompt.contains("취재 내용"));
        assert!(prompt.contains("취재원"));
        assert!(prompt.contains("비고"));
    }

    #[test]
    fn build_diary_prompt_brief_format() {
        let prompt = build_diary_prompt("테스트 데이터", "brief");
        assert!(prompt.contains("테스트 데이터"));
        assert!(prompt.contains("간략 취재 일지"));
        assert!(prompt.contains("접촉 취재원"));
    }

    // ── /rival tests ────────────────────────────────────────────────────

    #[test]
    fn rival_parse_help() {
        assert!(matches!(parse_rival_input("/rival"), RivalAction::Help));
        assert!(matches!(parse_rival_input("/rival help"), RivalAction::Help));
        assert!(matches!(
            parse_rival_input("/rival --help"),
            RivalAction::Help
        ));
    }

    #[test]
    fn rival_parse_search() {
        match parse_rival_input("/rival search 삼성전자 반도체") {
            RivalAction::Search(kw) => assert_eq!(kw, "삼성전자 반도체"),
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn rival_parse_search_empty() {
        assert!(matches!(
            parse_rival_input("/rival search"),
            RivalAction::Help
        ));
        assert!(matches!(
            parse_rival_input("/rival search   "),
            RivalAction::Help
        ));
    }

    #[test]
    fn rival_parse_compare_two_files() {
        match parse_rival_input("/rival my.md rival.md") {
            RivalAction::Compare { my_file, rival } => {
                assert_eq!(my_file, "my.md");
                assert_eq!(rival, "rival.md");
            }
            _ => panic!("expected Compare"),
        }
    }

    #[test]
    fn rival_parse_compare_with_url() {
        match parse_rival_input("/rival my.md https://news.example.com/article") {
            RivalAction::Compare { my_file, rival } => {
                assert_eq!(my_file, "my.md");
                assert_eq!(rival, "https://news.example.com/article");
            }
            _ => panic!("expected Compare"),
        }
    }

    #[test]
    fn rival_parse_single_arg_is_help() {
        assert!(matches!(
            parse_rival_input("/rival onlyonefile.md"),
            RivalAction::Help
        ));
    }

    #[test]
    fn rival_build_prompt_contains_sections() {
        let prompt = build_rival_prompt("내 기사 내용", "my.md", "경쟁사 내용", "rival.md");
        assert!(prompt.contains("경쟁 분석 관점"));
        assert!(prompt.contains("기사 각도(프레임) 차이"));
        assert!(prompt.contains("취재원 비교"));
        assert!(prompt.contains("빠진 정보"));
        assert!(prompt.contains("강점"));
        assert!(prompt.contains("구조·분량 비교"));
        assert!(prompt.contains("내 기사 내용"));
        assert!(prompt.contains("경쟁사 내용"));
        assert!(prompt.contains("my.md"));
        assert!(prompt.contains("rival.md"));
    }

    #[test]
    fn rival_build_search_prompt() {
        let prompt = build_rival_search_prompt("삼성전자");
        assert!(prompt.contains("삼성전자"));
        assert!(prompt.contains("경쟁사 기사를 검색"));
    }

    #[test]
    fn rival_file_path_with_topic() {
        let path = rival_file_path_with_date("삼성전자 반도체", "2026-03-22");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/rival/2026-03-22_삼성전자-반도체.md"
        );
    }

    #[test]
    fn rival_file_path_empty_topic() {
        let path = rival_file_path_with_date("", "2026-03-22");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/rival/2026-03-22_rival.md"
        );
    }

    #[test]
    fn rival_save_result() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("rival").join("test.md");
        save_rival_result(&path, "분석 결과").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "분석 결과");
    }

    // ── pipeline tests ──────────────────────────────────────────────────

    #[test]
    fn pipeline_parse_steps_quoted() {
        let steps =
            parse_pipeline_steps("\"research 반도체 수출\" \"factcheck\" \"article --type analysis\"");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], "research 반도체 수출");
        assert_eq!(steps[1], "factcheck");
        assert_eq!(steps[2], "article --type analysis");
    }

    #[test]
    fn pipeline_parse_steps_unquoted() {
        let steps = parse_pipeline_steps("factcheck research article");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], "factcheck");
        assert_eq!(steps[1], "research");
        assert_eq!(steps[2], "article");
    }

    #[test]
    fn pipeline_parse_steps_mixed() {
        let steps = parse_pipeline_steps("\"research 반도체\" factcheck \"article --type analysis\"");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], "research 반도체");
        assert_eq!(steps[1], "factcheck");
        assert_eq!(steps[2], "article --type analysis");
    }

    #[test]
    fn pipeline_parse_steps_empty() {
        let steps = parse_pipeline_steps("");
        assert!(steps.is_empty());
    }

    #[test]
    fn pipeline_save_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let steps = vec![
            "research 반도체".to_string(),
            "factcheck".to_string(),
            "article".to_string(),
        ];
        save_pipeline_to_dir(dir.path(), "test_pipe", &steps).unwrap();
        let loaded = load_pipeline_from_dir(dir.path(), "test_pipe").unwrap();
        assert_eq!(loaded.name, "test_pipe");
        assert_eq!(loaded.steps, steps);
    }

    #[test]
    fn pipeline_list_in_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        save_pipeline_to_dir(dir.path(), "alpha", &["research".to_string()]).unwrap();
        save_pipeline_to_dir(dir.path(), "beta", &["factcheck".to_string()]).unwrap();
        let names = list_pipelines_in(dir.path());
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn pipeline_list_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let names = list_pipelines_in(dir.path());
        assert!(names.is_empty());
    }

    #[test]
    fn pipeline_load_nonexistent() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(load_pipeline_from_dir(dir.path(), "nope").is_none());
    }

    #[test]
    fn pipeline_build_run_prompt() {
        let def = PipelineDef {
            name: "반도체속보".to_string(),
            steps: vec![
                "research 반도체".to_string(),
                "factcheck".to_string(),
                "article --type analysis".to_string(),
            ],
            created: "2026-03-22T16:00:00".to_string(),
        };
        let prompt = build_pipeline_run_prompt(&def);
        assert!(prompt.contains("반도체속보"));
        assert!(prompt.contains("/research 반도체"));
        assert!(prompt.contains("/factcheck"));
        assert!(prompt.contains("/article --type analysis"));
        assert!(prompt.contains("단계 1"));
        assert!(prompt.contains("단계 3"));
    }
}
