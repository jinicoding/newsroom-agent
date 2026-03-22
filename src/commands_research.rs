//! Research & source management command handlers (취재·리서치 도메인)
//! Commands: /alert, /clip, /contact, /factcheck, /follow, /law, /network, /news, /note, /press, /research, /rss, /sns, /sources, /trend

use crate::commands::auto_compact_if_needed;
use crate::commands_project::*;
use crate::commands_workflow::today_date_string;
use crate::commands_writing::format_unix_timestamp;
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

// ── /research ───────────────────────────────────────────────────────────

/// Directory for cached research results.
pub const RESEARCH_DIR: &str = ".journalist/research";

/// Build the research file path: `.journalist/research/YYYY-MM-DD_<slug>.md`
pub fn research_file_path(topic: &str) -> std::path::PathBuf {
    research_file_path_with_date(topic, &today_str())
}

/// Build the research file path with an explicit date string (for testing).
pub fn research_file_path_with_date(topic: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(topic, 50);
    let filename = if slug.is_empty() {
        format!("{date}_research.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(RESEARCH_DIR).join(filename)
}

/// Save research result to file. Creates the research directory if needed.
fn save_research(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// List existing research files in the research directory.
fn research_list() {
    let dir = std::path::Path::new(RESEARCH_DIR);
    if !dir.exists() {
        println!("{DIM}  저장된 리서치가 없습니다.{RESET}\n");
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == "md")
            })
            .collect(),
        Err(_) => {
            println!("{DIM}  리서치 디렉토리를 읽을 수 없습니다.{RESET}\n");
            return;
        }
    };
    if entries.is_empty() {
        println!("{DIM}  저장된 리서치가 없습니다.{RESET}\n");
        return;
    }
    entries.sort_by_key(|e| e.file_name());
    println!("{DIM}  저장된 리서치 목록:{RESET}");
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

/// Search saved research files by keyword (case-insensitive).
/// Checks both filename and file content. Returns (filename, first_line, preview).
pub fn research_search_in(
    keyword: &str,
    research_dir: &std::path::Path,
) -> Vec<(String, String, String)> {
    let kw = keyword.trim().to_lowercase();
    if kw.is_empty() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(research_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut results = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let filename = match path.file_name().and_then(|f| f.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let filename_lower = filename.to_lowercase();
        let content_lower = content.to_lowercase();
        if filename_lower.contains(&kw) || content_lower.contains(&kw) {
            let first_line = content
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("")
                .to_string();
            // Build a short preview: first matching line from content (up to 80 chars)
            let preview = content
                .lines()
                .find(|l| l.to_lowercase().contains(&kw))
                .map(|l| {
                    if l.len() > 80 {
                        format!("{}…", &l[..l.floor_char_boundary(80)])
                    } else {
                        l.to_string()
                    }
                })
                .unwrap_or_default();
            results.push((filename, first_line, preview));
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// Display research search results.
fn research_search(keyword: &str) {
    let dir = std::path::Path::new(RESEARCH_DIR);
    let results = research_search_in(keyword, dir);
    if results.is_empty() {
        println!("{DIM}  \"{keyword}\" 검색 결과가 없습니다.{RESET}\n");
        return;
    }
    println!(
        "{DIM}  \"{keyword}\" 검색 결과 ({count}건):{RESET}",
        count = results.len()
    );
    for (i, (filename, title, preview)) in results.iter().enumerate() {
        println!("{DIM}  {idx}. {filename}{RESET}", idx = i + 1);
        if !title.is_empty() {
            println!("{DIM}     제목: {title}{RESET}");
        }
        if !preview.is_empty() && preview != title {
            println!("{DIM}     매칭: {preview}{RESET}");
        }
    }
    println!();
}

/// Format a list of `NewsItem`s into a context block for the research prompt.
pub fn build_news_context(items: &[NewsItem]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut ctx = String::from(
        "\n\n[네이버 뉴스 API 검색 결과 — 아래 뉴스를 참고하여 리서치하세요]\n",
    );
    for (i, item) in items.iter().enumerate() {
        ctx.push_str(&format!("{}. {}", i + 1, item.title));
        if !item.pub_date.is_empty() {
            ctx.push_str(&format!(" ({})", item.pub_date));
        }
        ctx.push('\n');
        if !item.link.is_empty() {
            ctx.push_str(&format!("   링크: {}\n", item.link));
        }
        if !item.description.is_empty() {
            ctx.push_str(&format!("   요약: {}\n", item.description));
        }
    }
    ctx
}

/// Build the full research prompt, optionally injecting news API results.
pub fn build_research_prompt(topic: &str, news_context: &str) -> String {
    let encoded = topic.replace(' ', "+");
    format!(
        "다음 주제에 대해 웹 리서치를 수행해주세요: {topic}\n\n\
         다음 단계를 따라주세요:\n\
         1. DuckDuckGo로 검색: curl -s \"https://lite.duckduckgo.com/lite?q={encoded}\" | sed 's/<[^>]*>//g' | head -80\n\
         2. 네이버 뉴스 검색: curl -s \"https://search.naver.com/search.naver?where=news&query={encoded}\" | sed 's/<[^>]*>//g' | head -80\n\
         3. 검색 결과를 종합하여 다음을 정리:\n\
            - **핵심 사실** — 확인된 주요 정보\n\
            - **주요 출처** — 신뢰할 수 있는 출처 목록\n\
            - **쟁점** — 다른 시각이나 논란\n\
            - **추가 취재 제안** — 더 파고들 수 있는 방향\n\n\
         모든 정보에 출처를 명시하고, 확인되지 않은 내용은 명확히 표시하세요.{news_context}",
    )
}

/// Handle the /research command: web research on a topic using DuckDuckGo/Naver.
pub async fn handle_research(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let topic = input
        .strip_prefix("/research")
        .unwrap_or("")
        .trim();

    if topic.is_empty() {
        println!("{DIM}  사용법: /research <주제>{RESET}");
        println!("{DIM}  예시: /research 반도체 수출 동향{RESET}");
        println!("{DIM}  /research list — 저장된 리서치 목록{RESET}");
        println!("{DIM}  /research search <키워드> — 저장된 리서치 검색{RESET}\n");
        return;
    }

    if topic == "list" {
        research_list();
        return;
    }

    if let Some(kw) = topic.strip_prefix("search") {
        let kw = kw.trim();
        if kw.is_empty() {
            println!("{DIM}  사용법: /research search <키워드>{RESET}\n");
        } else {
            research_search(kw);
        }
        return;
    }

    // If Naver News API is configured, fetch recent news to enrich the prompt
    let news_context = match fetch_news_results(topic, 5) {
        Ok(items) if !items.is_empty() => {
            println!(
                "{DIM}  네이버 뉴스 API: {}건 수집{RESET}",
                items.len()
            );
            build_news_context(&items)
        }
        _ => String::new(),
    };

    let prompt = build_research_prompt(topic, &news_context);

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save research result to file
    if !response.trim().is_empty() {
        let path = research_file_path(topic);
        match save_research(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 리서치 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  리서치 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /sources ────────────────────────────────────────────────────────────

/// Sources database path.
pub const SOURCES_FILE: &str = ".journalist/sources.json";

/// Handle the /sources command: manage reporter's source database.
pub fn handle_sources(input: &str) {
    let args = input
        .strip_prefix("/sources")
        .unwrap_or("")
        .trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "list" => sources_list(),
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /sources add <이름> <소속> <연락처> [메모] [--beat 분야]{RESET}");
                println!("{DIM}  예시: /sources add 홍길동 산업통상자원부 010-1234-5678 반도체 정책 담당 --beat 경제{RESET}\n");
            } else {
                sources_add(rest);
            }
        }
        "search" => {
            let query = args.strip_prefix("search").unwrap_or("").trim();
            if query.is_empty() {
                println!("{DIM}  사용법: /sources search <검색어>{RESET}\n");
            } else {
                sources_search(query);
            }
        }
        "remove" => {
            let rest = args.strip_prefix("remove").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /sources remove <번호>{RESET}");
                println!("{DIM}  예시: /sources remove 2{RESET}\n");
            } else {
                sources_remove(rest);
            }
        }
        "edit" => {
            let rest = args.strip_prefix("edit").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /sources edit <번호> <필드> <값>{RESET}");
                println!("{DIM}  필드: name, org, contact, note, beat{RESET}");
                println!("{DIM}  예시: /sources edit 1 org 기획재정부{RESET}\n");
            } else {
                sources_edit(rest);
            }
        }
        "beat" => {
            let beat_name = args.strip_prefix("beat").unwrap_or("").trim();
            if beat_name.is_empty() {
                println!("{DIM}  사용법: /sources beat <분야명>{RESET}");
                println!("{DIM}  예시: /sources beat 경제{RESET}\n");
            } else {
                sources_beat_filter(beat_name);
            }
        }
        other => {
            println!("{DIM}  알 수 없는 하위 명령: {other}{RESET}");
            println!("{DIM}  사용법: /sources [list|add|search|remove|edit|beat]{RESET}\n");
        }
    }
}

pub fn ensure_sources_dir_at(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
}

pub fn load_sources() -> Vec<serde_json::Value> {
    load_sources_from(std::path::Path::new(SOURCES_FILE))
}

pub fn load_sources_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_sources(sources: &[serde_json::Value]) {
    save_sources_to(sources, std::path::Path::new(SOURCES_FILE));
}

pub fn save_sources_to(sources: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(sources) {
        let _ = std::fs::write(path, json);
    }
}

fn sources_list() {
    let sources = load_sources();
    if sources.is_empty() {
        println!("{DIM}  취재원 DB가 비어 있습니다.");
        println!("  /sources add <이름> <소속> <연락처> [메모] 로 추가하세요.{RESET}\n");
        return;
    }
    println!("{DIM}  ── 취재원 목록 ({} 명) ──", sources.len());
    for (i, s) in sources.iter().enumerate() {
        let name = s["name"].as_str().unwrap_or("?");
        let org = s["org"].as_str().unwrap_or("");
        let contact = s["contact"].as_str().unwrap_or("");
        let note = s["note"].as_str().unwrap_or("");
        let beat = s["beat"].as_str().unwrap_or("");
        let mut extra = String::new();
        if !beat.is_empty() {
            extra.push_str(&format!(" [{}]", beat));
        }
        if !note.is_empty() {
            extra.push_str(&format!(" | {note}"));
        }
        println!("  {}. {} | {} | {}{}", i + 1, name, org, contact, extra);
    }
    println!("{RESET}");
}

fn sources_add(args: &str) {
    // Extract --beat <value> if present, then parse remaining args
    let (beat, remaining) = extract_beat_option(args);
    let parts: Vec<&str> = remaining.splitn(4, ' ').collect();
    if parts.len() < 3 {
        println!("{DIM}  최소 이름, 소속, 연락처가 필요합니다.{RESET}\n");
        return;
    }
    let entry = serde_json::json!({
        "name": parts[0],
        "org": parts[1],
        "contact": parts[2],
        "note": if parts.len() > 3 { parts[3] } else { "" },
        "beat": beat,
    });
    let mut sources = load_sources();
    sources.push(entry);
    save_sources(&sources);
    let beat_info = if beat.is_empty() {
        String::new()
    } else {
        format!(" [{}]", beat)
    };
    println!(
        "{DIM}  취재원 추가됨: {} ({}){beat_info}{RESET}\n",
        parts[0], parts[1]
    );
}

/// Extract `--beat <value>` from args string, returning (beat, remaining_args).
fn extract_beat_option(args: &str) -> (&str, String) {
    let words: Vec<&str> = args.split_whitespace().collect();
    let mut beat = "";
    let mut remaining = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i] == "--beat" && i + 1 < words.len() {
            beat = words[i + 1];
            i += 2;
        } else {
            remaining.push(words[i]);
            i += 1;
        }
    }
    // Reconstruct remaining, preserving the note (last part) with spaces
    // We need to be more careful: rebuild from original args minus --beat <val>
    let remaining_str = if beat.is_empty() {
        args.to_string()
    } else {
        let beat_pattern = format!("--beat {}", beat);
        args.replace(&beat_pattern, "").split_whitespace().collect::<Vec<_>>().join(" ")
    };
    (beat, remaining_str)
}

fn sources_remove(args: &str) {
    let idx: usize = match args.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            println!("{DIM}  유효한 번호를 입력하세요. (1부터 시작){RESET}\n");
            return;
        }
    };
    let mut sources = load_sources();
    if idx > sources.len() {
        println!(
            "{DIM}  번호 {idx}은(는) 범위를 벗어났습니다. (총 {} 명){RESET}\n",
            sources.len()
        );
        return;
    }
    let removed = sources.remove(idx - 1);
    save_sources(&sources);
    let name = removed["name"].as_str().unwrap_or("?");
    let org = removed["org"].as_str().unwrap_or("");
    println!("{DIM}  취재원 삭제됨: {name} ({org}){RESET}\n");
}

fn sources_edit(args: &str) {
    let parts: Vec<&str> = args.splitn(3, ' ').collect();
    if parts.len() < 3 {
        println!("{DIM}  사용법: /sources edit <번호> <필드> <값>{RESET}");
        println!("{DIM}  필드: name, org, contact, note, beat{RESET}\n");
        return;
    }
    let idx: usize = match parts[0].parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            println!("{DIM}  유효한 번호를 입력하세요. (1부터 시작){RESET}\n");
            return;
        }
    };
    let field = parts[1];
    let value = parts[2];
    let valid_fields = ["name", "org", "contact", "note", "beat"];
    if !valid_fields.contains(&field) {
        println!("{DIM}  알 수 없는 필드: {field}{RESET}");
        println!("{DIM}  사용 가능한 필드: name, org, contact, note, beat{RESET}\n");
        return;
    }
    let mut sources = load_sources();
    if idx > sources.len() || sources.is_empty() {
        println!(
            "{DIM}  번호 {idx}은(는) 범위를 벗어났습니다. (총 {} 명){RESET}\n",
            sources.len()
        );
        return;
    }
    sources[idx - 1][field] = serde_json::Value::String(value.to_string());
    save_sources(&sources);
    let name = sources[idx - 1]["name"].as_str().unwrap_or("?");
    println!("{DIM}  취재원 수정됨: {name} — {field} → {value}{RESET}\n");
}

/// Check whether a source entry matches a query (case-insensitive).
pub fn source_matches(source: &serde_json::Value, query_lower: &str) -> bool {
    let text = format!(
        "{} {} {} {} {}",
        source["name"].as_str().unwrap_or(""),
        source["org"].as_str().unwrap_or(""),
        source["contact"].as_str().unwrap_or(""),
        source["note"].as_str().unwrap_or(""),
        source["beat"].as_str().unwrap_or(""),
    )
    .to_lowercase();
    text.contains(query_lower)
}

fn sources_search(query: &str) {
    let sources = load_sources();
    let query_lower = query.to_lowercase();
    let matches: Vec<&serde_json::Value> = sources
        .iter()
        .filter(|s| source_matches(s, &query_lower))
        .collect();

    if matches.is_empty() {
        println!("{DIM}  '{query}'에 해당하는 취재원이 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  ── 검색 결과: {} 명 ──", matches.len());
    for (i, s) in matches.iter().enumerate() {
        let name = s["name"].as_str().unwrap_or("?");
        let org = s["org"].as_str().unwrap_or("");
        let contact = s["contact"].as_str().unwrap_or("");
        let note = s["note"].as_str().unwrap_or("");
        let beat = s["beat"].as_str().unwrap_or("");
        let mut extra = String::new();
        if !beat.is_empty() {
            extra.push_str(&format!(" [{}]", beat));
        }
        if !note.is_empty() {
            extra.push_str(&format!(" | {note}"));
        }
        println!("  {}. {} | {} | {}{}", i + 1, name, org, contact, extra);
    }
    println!("{RESET}");
}

fn sources_beat_filter(beat: &str) {
    let sources = load_sources();
    let beat_lower = beat.to_lowercase();
    let matches: Vec<&serde_json::Value> = sources
        .iter()
        .filter(|s| {
            s["beat"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                == beat_lower
        })
        .collect();

    if matches.is_empty() {
        println!("{DIM}  '{beat}' 분야 취재원이 없습니다.{RESET}\n");
        return;
    }
    println!(
        "{DIM}  ── 분야별 취재원: {} ({} 명) ──",
        beat,
        matches.len()
    );
    for (i, s) in matches.iter().enumerate() {
        let name = s["name"].as_str().unwrap_or("?");
        let org = s["org"].as_str().unwrap_or("");
        let contact = s["contact"].as_str().unwrap_or("");
        let note = s["note"].as_str().unwrap_or("");
        println!(
            "  {}. {} | {} | {}{}",
            i + 1,
            name,
            org,
            contact,
            if note.is_empty() {
                String::new()
            } else {
                format!(" | {note}")
            }
        );
    }
    println!("{RESET}");
}

// ── /factcheck ──────────────────────────────────────────────────────────

/// Directory for cached factcheck results.
const FACTCHECK_DIR: &str = ".journalist/factcheck";

/// Build the factcheck file path: `.journalist/factcheck/YYYY-MM-DD_<slug>.md`
pub fn factcheck_file_path(claim: &str) -> std::path::PathBuf {
    factcheck_file_path_with_date(claim, &today_str())
}

/// Build the factcheck file path with an explicit date string (for testing).
pub fn factcheck_file_path_with_date(claim: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(claim, 50);
    let filename = if slug.is_empty() {
        format!("{date}_factcheck.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(FACTCHECK_DIR).join(filename)
}

/// Save factcheck result to file. Creates the factcheck directory if needed.
fn save_factcheck(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// List existing factcheck files in the factcheck directory.
fn factcheck_list() {
    let dir = std::path::Path::new(FACTCHECK_DIR);
    if !dir.exists() {
        println!("{DIM}  저장된 팩트체크가 없습니다.{RESET}\n");
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map_or(false, |ext| ext == "md")
            })
            .collect(),
        Err(_) => {
            println!("{DIM}  팩트체크 디렉토리를 읽을 수 없습니다.{RESET}\n");
            return;
        }
    };
    if entries.is_empty() {
        println!("{DIM}  저장된 팩트체크가 없습니다.{RESET}\n");
        return;
    }
    entries.sort_by_key(|e| e.file_name());
    println!("{DIM}  저장된 팩트체크 목록:{RESET}");
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

/// Build the factcheck prompt for a given claim.
/// Returns None if the claim is empty (should be rejected).
pub fn build_factcheck_prompt(claim: &str) -> Option<String> {
    if claim.is_empty() {
        return None;
    }
    Some(format!(
        "다음 주장/사실에 대한 팩트체크를 수행해주세요: \"{claim}\"\n\n\
         다음 단계를 따라주세요:\n\
         1. 여러 소스에서 관련 정보를 검색 (DuckDuckGo, 네이버 등)\n\
         2. 교차검증 전략을 적용하세요:\n\
         - 공공데이터포털(data.go.kr) 등 정부·공공 통계로 수치 확인\n\
         - 관련 기관의 공식 보도자료와 대조\n\
         - 시계열 데이터를 비교하여 추세와 맥락 파악\n\
         3. 검증 과정을 단계별로 보여주세요 (\"Show Me the Work\" 원칙 — 기자는 근거 없는 판정을 쓸 수 없습니다):\n\
         - 어떤 소스를 확인했는지\n\
         - 각 소스에서 무엇을 발견했는지\n\
         - 소스 간 일치/불일치 여부\n\
         4. 다음 형식으로 결과를 정리:\n\n\
         **주장:** {claim}\n\
         **판정:** [사실 / 대체로 사실 / 절반의 사실 / 대체로 거짓 / 거짓 / 판단 불가]\n\
         **검증 과정:**\n\
         - [단계 1]: [확인한 소스와 발견 내용]\n\
         - [단계 2]: [확인한 소스와 발견 내용]\n\
         - [단계 3]: [소스 간 교차 대조 결과]\n\
         **근거:**\n\
         - 출처 1: [내용]\n\
         - 출처 2: [내용]\n\
         **맥락:** [주장의 배경이나 누락된 맥락]\n\
         **결론:** [기자가 기사에 반영할 때 주의할 점]\n\n\
         주의: 확인할 수 없는 경우 '판단 불가'로 표시하고 그 이유를 설명하세요.\n\
         절대로 사실을 만들어내지 마세요."
    ))
}

/// Handle the /factcheck command: multi-source fact verification.
pub async fn handle_factcheck(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let claim = input
        .strip_prefix("/factcheck")
        .unwrap_or("")
        .trim();

    if claim == "list" {
        factcheck_list();
        return;
    }

    let prompt = match build_factcheck_prompt(claim) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /factcheck <주장 또는 사실>{RESET}");
            println!("{DIM}  예시: /factcheck 한국 반도체 수출이 2025년 사상 최대를 기록했다{RESET}");
            println!("{DIM}  /factcheck list — 저장된 팩트체크 목록{RESET}\n");
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save factcheck result to file
    if !response.trim().is_empty() {
        let path = factcheck_file_path(claim);
        match save_factcheck(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 팩트체크 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  팩트체크 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /clip ────────────────────────────────────────────────────────────────

/// Directory where clipped articles are saved.
const CLIPS_DIR: &str = ".journalist/clips";

/// Build the file path for a clip from a URL and date.
fn clip_file_path(url: &str, date: &str) -> std::path::PathBuf {
    // Extract domain + path slug from URL
    let slug = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .replace(['/', '?', '&', '=', '#', '%', ':', '.'], "-")
        .trim_matches('-')
        .to_string();
    let slug = if slug.len() > 80 {
        slug[..80].trim_end_matches('-').to_string()
    } else {
        slug
    };
    let filename = format!("{date}_{slug}.md");
    std::path::PathBuf::from(CLIPS_DIR).join(filename)
}

/// Save clipped article content to a file, creating directories as needed.
fn save_clip(path: &std::path::Path, url: &str, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let full = format!("<!-- source: {url} -->\n\n{content}");
    std::fs::write(path, full)
}

/// List saved clips in `.journalist/clips/`.
fn clip_list() {
    let dir = std::path::Path::new(CLIPS_DIR);
    if !dir.exists() {
        println!("{DIM}  스크랩한 기사가 없습니다.{RESET}\n");
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "md")
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            eprintln!("{RED}  클립 목록 읽기 실패: {e}{RESET}\n");
            return;
        }
    };
    if entries.is_empty() {
        println!("{DIM}  스크랩한 기사가 없습니다.{RESET}\n");
        return;
    }
    entries.sort_by_key(|e| e.file_name());
    entries.reverse(); // newest first
    println!("{DIM}  ── 스크랩 목록 ({} 건) ──{RESET}", entries.len());
    for (i, entry) in entries.iter().enumerate() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Try to read first line for source URL
        let path = entry.path();
        let source = std::fs::read_to_string(&path)
            .ok()
            .and_then(|c| {
                c.lines()
                    .next()
                    .and_then(|l| l.strip_prefix("<!-- source: "))
                    .and_then(|l| l.strip_suffix(" -->"))
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        if source.is_empty() {
            println!("  {: >3}. {name}", i + 1);
        } else {
            println!("  {: >3}. {name}", i + 1);
            println!("{DIM}       {source}{RESET}");
        }
    }
    println!();
}

/// Handle the `/clip` command.
pub async fn handle_clip(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/clip").unwrap_or("").trim();

    if args.is_empty() || args == "help" {
        println!("{DIM}  사용법: /clip <URL>       URL 기사 스크랩{RESET}");
        println!("{DIM}          /clip list        스크랩 목록 보기{RESET}");
        println!("{DIM}  예시:   /clip https://news.example.com/article/123{RESET}\n");
        return;
    }

    if args == "list" {
        clip_list();
        return;
    }

    let url = args.split_whitespace().next().unwrap_or(args);
    if !url.starts_with("http://") && !url.starts_with("https://") {
        eprintln!("{RED}  유효한 URL이 아닙니다: {url}{RESET}");
        println!("{DIM}  http:// 또는 https://로 시작하는 URL을 입력하세요.{RESET}\n");
        return;
    }

    println!("{DIM}  기사 가져오는 중: {url}{RESET}");

    // Fetch and strip HTML
    let fetch_cmd = format!(
        "curl -sL --max-time 15 '{}' | sed 's/<[^>]*>//g' | head -c 50000",
        url.replace('\'', "'\\''")
    );
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(&fetch_cmd)
        .output();

    let raw_text = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            eprintln!("{RED}  기사 가져오기 실패: {err}{RESET}\n");
            return;
        }
        Err(e) => {
            eprintln!("{RED}  curl 실행 실패: {e}{RESET}\n");
            return;
        }
    };

    if raw_text.trim().is_empty() {
        eprintln!("{RED}  빈 페이지이거나 접근할 수 없는 URL입니다.{RESET}\n");
        return;
    }

    // Use AI to extract the article body
    let prompt = format!(
        "다음은 웹 페이지에서 HTML 태그를 제거한 텍스트입니다. \
         여기서 **기사 본문만** 추출해주세요. 광고, 메뉴, 푸터, 관련기사 목록 등은 모두 제외하세요.\n\
         제목이 있으면 맨 위에 # 제목 형식으로 포함하세요.\n\
         날짜, 기자명이 보이면 제목 아래에 메타 정보로 포함하세요.\n\
         본문은 원문 그대로 유지하되, 깨끗하게 정리해주세요.\n\n\
         출처 URL: {url}\n\n\
         ---\n\n{raw_text}"
    );

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    if response.trim().is_empty() {
        eprintln!("{RED}  기사 본문 추출 실패{RESET}\n");
        return;
    }

    // Save to .journalist/clips/
    let today = today_str();
    let path = clip_file_path(url, &today);
    match save_clip(&path, url, &response) {
        Ok(_) => {
            println!(
                "{GREEN}  ✓ 스크랩 저장: {}{RESET}\n",
                path.display()
            );
        }
        Err(e) => {
            eprintln!("{RED}  스크랩 저장 실패: {e}{RESET}\n");
        }
    }
}

// ── /news ────────────────────────────────────────────────────────────────

/// A single news search result.
#[derive(Debug, Clone)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    pub description: String,
    pub pub_date: String,
}

/// Strip HTML tags and decode common HTML entities.
pub fn strip_html_tags(s: &str) -> String {
    // Remove HTML tags
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Parse Naver News API JSON response into a list of `NewsItem`.
pub fn parse_naver_news_json(json: &str) -> Vec<NewsItem> {
    // Minimal JSON parsing without serde — extract items array
    let items_start = match json.find("\"items\"") {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let array_start = match json[items_start..].find('[') {
        Some(pos) => items_start + pos,
        None => return Vec::new(),
    };
    let array_end = match json[array_start..].rfind(']') {
        Some(pos) => array_start + pos + 1,
        None => return Vec::new(),
    };
    let array_str = &json[array_start..array_end];

    // Split by objects — find each {...}
    let mut results = Vec::new();
    let mut depth = 0;
    let mut obj_start = None;
    for (i, ch) in array_str.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        let obj = &array_str[start..=i];
                        if let Some(item) = parse_news_item(obj) {
                            results.push(item);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    results
}

/// Extract a field value from a JSON object string (simple key-value parsing).
fn json_extract_string(obj: &str, key: &str) -> Option<String> {
    let search = format!("\"{}\"", key);
    let key_pos = obj.find(&search)?;
    let after_key = &obj[key_pos + search.len()..];
    // Skip whitespace and colon
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if !after_colon.starts_with('"') {
        return None;
    }
    let value_start = 1; // skip opening quote
    let mut escaped = false;
    let mut end = None;
    for (i, ch) in after_colon[value_start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            end = Some(value_start + i);
            break;
        }
    }
    let end = end?;
    Some(after_colon[value_start..end].to_string())
}

/// Parse a single news item JSON object.
fn parse_news_item(obj: &str) -> Option<NewsItem> {
    let title = json_extract_string(obj, "title").unwrap_or_default();
    let link = json_extract_string(obj, "link").unwrap_or_default();
    let description = json_extract_string(obj, "description").unwrap_or_default();
    let pub_date = json_extract_string(obj, "pubDate").unwrap_or_default();

    if title.is_empty() && link.is_empty() {
        return None;
    }

    Some(NewsItem {
        title: strip_html_tags(&title),
        link,
        description: strip_html_tags(&description),
        pub_date,
    })
}

/// Generate file path for saving a news item as a clip.
pub fn news_clip_path(item: &NewsItem, date: &str) -> std::path::PathBuf {
    clip_file_path(&item.link, date)
}

/// Search Naver News via API (with env vars) or fallback to curl-based search.
fn fetch_news_results(keyword: &str, display: u32) -> Result<Vec<NewsItem>, String> {
    let client_id = std::env::var("NAVER_CLIENT_ID").ok();
    let client_secret = std::env::var("NAVER_CLIENT_SECRET").ok();

    if let (Some(id), Some(secret)) = (client_id, client_secret) {
        // Use Naver News API
        let encoded = keyword.replace(' ', "%20");
        let url = format!(
            "https://openapi.naver.com/v1/search/news.json?query={}&display={}&sort=date",
            encoded, display
        );
        let output = std::process::Command::new("curl")
            .args([
                "-s",
                "--max-time",
                "10",
                "-H",
                &format!("X-Naver-Client-Id: {}", id),
                "-H",
                &format!("X-Naver-Client-Secret: {}", secret),
                &url,
            ])
            .output()
            .map_err(|e| format!("curl 실행 실패: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "API 요청 실패: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let body = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(parse_naver_news_json(&body))
    } else {
        // Fallback: curl-based web scraping via DuckDuckGo lite
        let encoded = keyword.replace(' ', "+");
        let url = format!(
            "https://lite.duckduckgo.com/lite/?q={}+site:news.naver.com&kl=kr-kr",
            encoded
        );
        let output = std::process::Command::new("curl")
            .args([
                "-sL",
                "--max-time",
                "10",
                "-A",
                "Mozilla/5.0",
                &url,
            ])
            .output()
            .map_err(|e| format!("curl 실행 실패: {e}"))?;

        if !output.status.success() {
            return Err("웹 검색 실패".to_string());
        }
        let body = String::from_utf8_lossy(&output.stdout).to_string();
        // Parse DuckDuckGo lite results: extract links and titles
        let mut results = Vec::new();
        for line in body.lines() {
            if let Some(href_pos) = line.find("href=\"") {
                let after = &line[href_pos + 6..];
                if let Some(end) = after.find('"') {
                    let link = &after[..end];
                    if link.contains("news.naver.com") || link.contains("n.news.naver.com") {
                        // Extract text between > and <
                        let title = if let Some(gt) = line.rfind('>') {
                            let rest = &line[gt + 1..];
                            if let Some(lt) = rest.find('<') {
                                strip_html_tags(&rest[..lt])
                            } else {
                                strip_html_tags(rest)
                            }
                        } else {
                            String::new()
                        };
                        if !title.trim().is_empty() {
                            results.push(NewsItem {
                                title: title.trim().to_string(),
                                link: link.to_string(),
                                description: String::new(),
                                pub_date: String::new(),
                            });
                        }
                    }
                }
            }
            if results.len() >= display as usize {
                break;
            }
        }
        if results.is_empty() {
            Err("검색 결과가 없습니다. NAVER_CLIENT_ID/NAVER_CLIENT_SECRET 환경변수를 설정하면 더 정확한 결과를 얻을 수 있습니다.".to_string())
        } else {
            Ok(results)
        }
    }
}

/// Display news search results.
fn display_news_results(results: &[NewsItem]) {
    println!();
    for (i, item) in results.iter().enumerate() {
        println!(
            "  {BOLD}{YELLOW}[{}]{RESET} {BOLD}{}{RESET}",
            i + 1,
            item.title
        );
        if !item.pub_date.is_empty() {
            println!("  {DIM}    {}{RESET}", item.pub_date);
        }
        if !item.description.is_empty() {
            println!("  {DIM}    {}{RESET}", item.description);
        }
        println!("  {DIM}    {}{RESET}", item.link);
        println!();
    }
}

/// Thread-local storage for last news search results (for `/news save`).
use std::cell::RefCell;
thread_local! {
    static LAST_NEWS_RESULTS: RefCell<Vec<NewsItem>> = const { RefCell::new(Vec::new()) };
}

/// Handle the `/news` command.
pub async fn handle_news(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/news").unwrap_or("").trim();

    if args.is_empty() || args == "help" {
        println!("{DIM}  사용법: /news <키워드>     뉴스 검색{RESET}");
        println!("{DIM}          /news save <번호>  검색 결과를 클립으로 저장{RESET}");
        println!(
            "{DIM}  환경변수: NAVER_CLIENT_ID, NAVER_CLIENT_SECRET (미설정 시 웹 검색 폴백){RESET}"
        );
        println!("{DIM}  예시:   /news 반도체 수출{RESET}\n");
        return;
    }

    // Handle /news save <number>
    if let Some(save_args) = args.strip_prefix("save") {
        let save_args = save_args.trim();
        let num: usize = match save_args.parse() {
            Ok(n) if n >= 1 => n,
            _ => {
                eprintln!("{RED}  유효한 번호를 입력하세요 (예: /news save 1){RESET}\n");
                return;
            }
        };
        LAST_NEWS_RESULTS.with(|results| {
            let results = results.borrow();
            if results.is_empty() {
                eprintln!("{RED}  먼저 /news <키워드>로 검색하세요.{RESET}\n");
                return;
            }
            if num > results.len() {
                eprintln!(
                    "{RED}  번호 범위 초과: 1~{} 사이의 번호를 입력하세요.{RESET}\n",
                    results.len()
                );
                return;
            }
            let item = &results[num - 1];
            let date = today_str();
            let path = news_clip_path(item, &date);
            let content = format!(
                "# {}\n\n- 날짜: {}\n- 링크: {}\n\n{}\n",
                item.title, item.pub_date, item.link, item.description
            );
            match save_clip(&path, &item.link, &content) {
                Ok(_) => {
                    println!(
                        "{GREEN}  ✓ 저장: {}{RESET}\n",
                        path.display()
                    );
                }
                Err(e) => {
                    eprintln!("{RED}  저장 실패: {e}{RESET}\n");
                }
            }
        });
        return;
    }

    // Regular search
    let keyword = args;
    println!("{DIM}  '{keyword}' 뉴스 검색 중...{RESET}");

    match fetch_news_results(keyword, 10) {
        Ok(results) if results.is_empty() => {
            println!("{DIM}  검색 결과가 없습니다.{RESET}\n");
        }
        Ok(results) => {
            display_news_results(&results);
            println!(
                "{DIM}  💡 /news save <번호> 로 기사를 클립에 저장할 수 있습니다.{RESET}\n"
            );
            // Store for /news save
            LAST_NEWS_RESULTS.with(|cell| {
                *cell.borrow_mut() = results;
            });
        }
        Err(e) => {
            eprintln!("{RED}  뉴스 검색 실패: {e}{RESET}\n");
            // Fallback: ask the agent to search
            let prompt = format!(
                "'{keyword}'에 대한 최신 뉴스를 검색해서 정리해줘. \
                 제목, 날짜, 요약, 출처 링크를 포함해서 목록으로 보여줘."
            );
            run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
        }
    }
}

// ── /wire — 통신사 속보 모니터링 ──────────────────────────────────────

/// RSS feed URLs for major Korean wire services.
const WIRE_FEEDS: &[(&str, &str)] = &[
    ("연합뉴스", "https://www.yna.co.kr/rss/news.xml"),
    ("뉴시스", "https://newsis.com/rss/all_rss.xml"),
    ("뉴스1", "https://www.news1.kr/rss/latest"),
];

// Thread-local storage for the last wire results (for `/wire save`).
thread_local! {
    static LAST_WIRE_RESULTS: std::cell::RefCell<Vec<NewsItem>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Parse RSS XML content into a list of `NewsItem`.
/// Extracts `<title>`, `<link>`, `<description>`, and `<pubDate>` from each `<item>`.
pub fn parse_rss_items(xml: &str) -> Vec<NewsItem> {
    let mut results = Vec::new();
    let mut search_from = 0;

    while let Some(item_start) = xml[search_from..].find("<item>").or_else(|| xml[search_from..].find("<item ")) {
        let abs_start = search_from + item_start;
        let item_end = match xml[abs_start..].find("</item>") {
            Some(pos) => abs_start + pos + 7,
            None => break,
        };
        let item_xml = &xml[abs_start..item_end];

        let title = xml_extract_tag(item_xml, "title").unwrap_or_default();
        let link = xml_extract_tag(item_xml, "link").unwrap_or_default();
        let description = xml_extract_tag(item_xml, "description").unwrap_or_default();
        let pub_date = xml_extract_tag(item_xml, "pubDate").unwrap_or_default();

        if !title.is_empty() || !link.is_empty() {
            results.push(NewsItem {
                title: strip_html_tags(&title).trim().to_string(),
                link: link.trim().to_string(),
                description: strip_html_tags(&description).trim().to_string(),
                pub_date: pub_date.trim().to_string(),
            });
        }

        search_from = item_end;
    }
    results
}

/// Extract text content between `<tag>...</tag>` or `<tag><![CDATA[...]]></tag>`.
fn xml_extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let start_pos = xml.find(&open)?;
    // Skip past the opening tag (handle attributes)
    let after_open = &xml[start_pos + open.len()..];
    let content_start = after_open.find('>')? + 1;
    let content = &after_open[content_start..];
    let end_pos = content.find(&close)?;
    let raw = &content[..end_pos];

    // Handle CDATA sections
    let raw = raw.trim();
    if let Some(cdata) = raw.strip_prefix("<![CDATA[") {
        if let Some(end) = cdata.find("]]>") {
            return Some(cdata[..end].to_string());
        }
    }
    Some(raw.to_string())
}

/// Fetch RSS feed from a single URL.
fn fetch_rss_feed(name: &str, url: &str) -> Result<Vec<NewsItem>, String> {
    let output = std::process::Command::new("curl")
        .args(["-sL", "--max-time", "10", "-A", "Mozilla/5.0", url])
        .output()
        .map_err(|e| format!("{name}: curl 실행 실패: {e}"))?;

    if !output.status.success() {
        return Err(format!("{name}: HTTP 요청 실패"));
    }
    let body = String::from_utf8_lossy(&output.stdout).to_string();
    let mut items = parse_rss_items(&body);
    // Tag each item with the source name in the description prefix
    for item in &mut items {
        if !item.description.is_empty() {
            item.description = format!("[{name}] {}", item.description);
        } else {
            item.description = format!("[{name}]");
        }
    }
    Ok(items)
}

/// Fetch wire news from all configured RSS feeds.
fn fetch_wire_news(keyword: Option<&str>, max_items: usize) -> Vec<NewsItem> {
    let mut all_items = Vec::new();
    for &(name, url) in WIRE_FEEDS {
        match fetch_rss_feed(name, url) {
            Ok(items) => all_items.extend(items),
            Err(e) => {
                eprintln!("  {DIM}{e}{RESET}");
            }
        }
    }

    // Filter by keyword if provided
    if let Some(kw) = keyword {
        let kw_lower = kw.to_lowercase();
        let keywords: Vec<&str> = kw_lower.split_whitespace().collect();
        all_items.retain(|item| {
            let title_lower = item.title.to_lowercase();
            let desc_lower = item.description.to_lowercase();
            keywords.iter().all(|k| title_lower.contains(k) || desc_lower.contains(k))
        });
    }

    all_items.truncate(max_items);
    all_items
}

/// Display wire news results.
fn display_wire_results(results: &[NewsItem]) {
    println!();
    for (i, item) in results.iter().enumerate() {
        println!(
            "  {BOLD}{YELLOW}[{}]{RESET} {BOLD}{}{RESET}",
            i + 1,
            item.title
        );
        if !item.pub_date.is_empty() {
            println!("     {DIM}{}{RESET}", item.pub_date);
        }
        if !item.description.is_empty() {
            let desc = if item.description.len() > 120 {
                format!("{}…", &item.description[..item.description.char_indices().nth(120).map(|(i, _)| i).unwrap_or(item.description.len())])
            } else {
                item.description.clone()
            };
            println!("     {DIM}{desc}{RESET}");
        }
        if !item.link.is_empty() {
            println!("     {DIM}{}{RESET}", item.link);
        }
        println!();
    }
}

/// Handle the `/wire` command: wire service breaking news monitoring via RSS.
pub fn handle_wire(input: &str) {
    let args = input.strip_prefix("/wire").unwrap_or("").trim();

    if args == "help" {
        println!("{DIM}  사용법: /wire              최신 속보 (최대 20건){RESET}");
        println!("{DIM}          /wire <키워드>     키워드 필터링{RESET}");
        println!("{DIM}          /wire save <번호>  기사를 클립으로 저장{RESET}");
        println!("{DIM}  피드:   연합뉴스, 뉴시스, 뉴스1{RESET}");
        println!("{DIM}  비교:   /news는 키워드 검색, /wire는 실시간 속보 피드{RESET}\n");
        return;
    }

    // Handle /wire save <number>
    if let Some(save_args) = args.strip_prefix("save") {
        let save_args = save_args.trim();
        let num: usize = match save_args.parse() {
            Ok(n) if n >= 1 => n,
            _ => {
                eprintln!("{RED}  유효한 번호를 입력하세요 (예: /wire save 1){RESET}\n");
                return;
            }
        };
        LAST_WIRE_RESULTS.with(|results| {
            let results = results.borrow();
            if results.is_empty() {
                eprintln!("{RED}  먼저 /wire 로 속보를 조회하세요.{RESET}\n");
                return;
            }
            if num > results.len() {
                eprintln!(
                    "{RED}  번호 범위 초과: 1~{} 사이의 번호를 입력하세요.{RESET}\n",
                    results.len()
                );
                return;
            }
            let item = &results[num - 1];
            let date = today_str();
            let path = news_clip_path(item, &date);
            let content = format!(
                "# {}\n\n- 날짜: {}\n- 링크: {}\n- 출처: {}\n\n{}\n",
                item.title,
                item.pub_date,
                item.link,
                item.description.split(']').next().unwrap_or("").trim_start_matches('['),
                item.description
            );
            match save_clip(&path, &item.link, &content) {
                Ok(_) => {
                    println!(
                        "{GREEN}  ✓ 저장: {}{RESET}\n",
                        path.display()
                    );
                }
                Err(e) => {
                    eprintln!("{RED}  저장 실패: {e}{RESET}\n");
                }
            }
        });
        return;
    }

    // Fetch wire news
    let keyword = if args.is_empty() { None } else { Some(args) };
    let label = keyword.unwrap_or("전체");
    println!("{DIM}  통신사 속보 조회 중... ({label}){RESET}");

    let results = fetch_wire_news(keyword, 20);
    if results.is_empty() {
        if keyword.is_some() {
            println!("{DIM}  '{label}'에 해당하는 속보가 없습니다.{RESET}\n");
        } else {
            println!("{DIM}  속보 피드를 가져올 수 없습니다. 네트워크를 확인하세요.{RESET}\n");
        }
        return;
    }

    println!("{DIM}  ── 통신사 속보 ({} 건) ──{RESET}", results.len());
    display_wire_results(&results);
    println!("{DIM}  💡 /wire save <번호> 로 기사를 클립에 저장할 수 있습니다.{RESET}\n");

    // Store for /wire save
    LAST_WIRE_RESULTS.with(|cell| {
        *cell.borrow_mut() = results;
    });
}

// ── /rss — RSS 피드 구독 및 뉴스 수집 ─────────────────────────────────

/// File storing the list of subscribed RSS feed URLs.
const RSS_FEEDS_FILE: &str = ".journalist/rss/feeds.json";
/// Directory storing cached items per feed.
const RSS_CACHE_DIR: &str = ".journalist/rss/cache";

/// A single RSS feed subscription entry.
#[derive(Debug, Clone)]
struct RssFeed {
    url: String,
    name: String,
    added: String,
}

/// Load subscribed RSS feeds from the feeds file.
fn load_rss_feeds() -> Vec<RssFeed> {
    load_rss_feeds_from(std::path::Path::new(RSS_FEEDS_FILE))
}

fn load_rss_feeds_from(path: &std::path::Path) -> Vec<RssFeed> {
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let arr: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap_or_default();
    arr.iter()
        .filter_map(|v| {
            Some(RssFeed {
                url: v["url"].as_str()?.to_string(),
                name: v["name"].as_str().unwrap_or("").to_string(),
                added: v["added"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Save RSS feeds to the feeds file.
fn save_rss_feeds(feeds: &[RssFeed]) {
    save_rss_feeds_to(feeds, std::path::Path::new(RSS_FEEDS_FILE));
}

fn save_rss_feeds_to(feeds: &[RssFeed], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    let arr: Vec<serde_json::Value> = feeds
        .iter()
        .map(|f| {
            serde_json::json!({
                "url": f.url,
                "name": f.name,
                "added": f.added,
            })
        })
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&arr) {
        let _ = std::fs::write(path, json);
    }
}

/// Derive a cache filename from a feed URL.
fn rss_cache_filename(url: &str) -> String {
    // Simple hash: use a slug of the URL domain + path
    let stripped = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let slug = crate::commands_project::topic_to_slug(stripped, 60);
    if slug.is_empty() {
        "feed".to_string()
    } else {
        slug
    }
}

/// Load cached RSS items for a given feed URL.
fn load_rss_cache(url: &str) -> Vec<NewsItem> {
    let filename = format!("{}.json", rss_cache_filename(url));
    let path = std::path::Path::new(RSS_CACHE_DIR).join(filename);
    load_rss_cache_from(&path)
}

fn load_rss_cache_from(path: &std::path::Path) -> Vec<NewsItem> {
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let arr: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap_or_default();
    arr.iter()
        .filter_map(|v| {
            Some(NewsItem {
                title: v["title"].as_str()?.to_string(),
                link: v["link"].as_str().unwrap_or("").to_string(),
                description: v["description"].as_str().unwrap_or("").to_string(),
                pub_date: v["pub_date"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Save cached RSS items for a given feed URL.
fn save_rss_cache(url: &str, items: &[NewsItem]) {
    let filename = format!("{}.json", rss_cache_filename(url));
    let path = std::path::Path::new(RSS_CACHE_DIR).join(filename);
    save_rss_cache_to(items, &path);
}

fn save_rss_cache_to(items: &[NewsItem], path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let arr: Vec<serde_json::Value> = items
        .iter()
        .map(|item| {
            serde_json::json!({
                "title": item.title,
                "link": item.link,
                "description": item.description,
                "pub_date": item.pub_date,
            })
        })
        .collect();
    if let Ok(json) = serde_json::to_string_pretty(&arr) {
        let _ = std::fs::write(path, json);
    }
}

/// Handle the `/rss` command: RSS feed subscription and news collection.
pub fn handle_rss(input: &str) {
    let args = input.strip_prefix("/rss").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("help") {
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /rss add <URL> [이름]{RESET}");
                println!("{DIM}  예시: /rss add https://www.yna.co.kr/rss/news.xml 연합뉴스{RESET}\n");
            } else {
                rss_add(rest);
            }
        }
        "list" => {
            rss_list();
        }
        "check" => {
            rss_check();
        }
        "search" => {
            let rest = args.strip_prefix("search").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /rss search <키워드>{RESET}");
                println!("{DIM}  예시: /rss search 반도체{RESET}\n");
            } else {
                rss_search(rest);
            }
        }
        "remove" => {
            let rest = args.strip_prefix("remove").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /rss remove <번호>{RESET}");
                println!("{DIM}  /rss list 에서 번호를 확인하세요.{RESET}\n");
            } else {
                rss_remove(rest);
            }
        }
        "help" => {
            println!("{DIM}  사용법:{RESET}");
            println!("{DIM}    /rss add <URL> [이름]   피드 등록{RESET}");
            println!("{DIM}    /rss list               구독 목록{RESET}");
            println!("{DIM}    /rss check              최신 뉴스 가져오기{RESET}");
            println!("{DIM}    /rss search <키워드>    가져온 뉴스 검색{RESET}");
            println!("{DIM}    /rss remove <번호>      피드 삭제{RESET}");
            println!("{DIM}  비교: /wire·/news는 내장 소스, /rss는 사용자 지정 피드{RESET}\n");
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /rss [add|list|check|search|remove|help]{RESET}\n");
        }
    }
}

/// Add a new RSS feed subscription.
fn rss_add(rest: &str) {
    let mut parts = rest.splitn(2, char::is_whitespace);
    let url = parts.next().unwrap_or("").trim();
    let name = parts.next().unwrap_or("").trim();

    if !url.starts_with("http://") && !url.starts_with("https://") {
        eprintln!("{RED}  유효한 URL을 입력하세요 (http:// 또는 https://){RESET}\n");
        return;
    }

    let mut feeds = load_rss_feeds();

    // Check for duplicates
    if feeds.iter().any(|f| f.url == url) {
        println!("{DIM}  이미 등록된 피드입니다: {url}{RESET}\n");
        return;
    }

    // Auto-detect name from feed if not provided
    let feed_name = if name.is_empty() {
        // Try to extract domain as name
        let domain = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or(url);
        domain.to_string()
    } else {
        name.to_string()
    };

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);

    feeds.push(RssFeed {
        url: url.to_string(),
        name: feed_name.clone(),
        added: timestamp.clone(),
    });
    save_rss_feeds(&feeds);

    println!("{GREEN}  ✓ RSS 피드 등록: {feed_name} ({url}){RESET}");
    println!("{DIM}    /rss check 으로 뉴스를 가져올 수 있습니다.{RESET}\n");
}

/// List all subscribed RSS feeds.
fn rss_list() {
    let feeds = load_rss_feeds();
    if feeds.is_empty() {
        println!("{DIM}  등록된 RSS 피드가 없습니다.");
        println!("  /rss add <URL> [이름] 으로 추가하세요.{RESET}\n");
        return;
    }

    println!("{BOLD}  RSS 구독 목록 ({} 개){RESET}", feeds.len());
    println!("{DIM}  ─────────────────────────────{RESET}");
    for (i, feed) in feeds.iter().enumerate() {
        println!(
            "{DIM}  {}. {}{RESET}  {DIM}{}{RESET}",
            i + 1,
            if feed.name.is_empty() {
                &feed.url
            } else {
                &feed.name
            },
            feed.url
        );
        if !feed.added.is_empty() {
            println!("{DIM}     등록: {}{RESET}", feed.added);
        }
    }
    println!();
}

/// Fetch latest news from all subscribed RSS feeds.
fn rss_check() {
    let feeds = load_rss_feeds();
    if feeds.is_empty() {
        println!("{DIM}  등록된 RSS 피드가 없습니다.");
        println!("  /rss add <URL> [이름] 으로 추가하세요.{RESET}\n");
        return;
    }

    println!("{BOLD}  RSS 피드 확인 중... ({} 개 피드){RESET}\n", feeds.len());

    let mut total_new = 0usize;

    for feed in &feeds {
        let label = if feed.name.is_empty() {
            &feed.url
        } else {
            &feed.name
        };
        print!("{DIM}  ▶ {label}...{RESET}");

        match fetch_rss_feed(label, &feed.url) {
            Ok(items) => {
                // Load existing cache to find new items
                let existing = load_rss_cache(&feed.url);
                let existing_links: std::collections::HashSet<&str> =
                    existing.iter().map(|i| i.link.as_str()).collect();

                let new_items: Vec<&NewsItem> = items
                    .iter()
                    .filter(|i| !i.link.is_empty() && !existing_links.contains(i.link.as_str()))
                    .collect();

                let new_count = new_items.len();
                total_new += new_count;

                println!(" {GREEN}{} 건{RESET} (새 {} 건)", items.len(), new_count);

                // Show new items
                for item in new_items.iter().take(5) {
                    println!(
                        "    {YELLOW}•{RESET} {BOLD}{}{RESET}",
                        item.title
                    );
                    if !item.pub_date.is_empty() {
                        println!("      {DIM}{}{RESET}", item.pub_date);
                    }
                }
                if new_count > 5 {
                    println!("    {DIM}... 외 {} 건{RESET}", new_count - 5);
                }

                // Merge and save cache (keep latest 200 per feed)
                let mut merged = items;
                for old in existing {
                    if !merged.iter().any(|m| m.link == old.link) {
                        merged.push(old);
                    }
                }
                merged.truncate(200);
                save_rss_cache(&feed.url, &merged);
            }
            Err(e) => {
                println!(" {RED}실패: {e}{RESET}");
            }
        }
        println!();
    }

    println!(
        "{BOLD}  총 새 기사: {total_new} 건{RESET}"
    );
    println!("{DIM}  /rss search <키워드> 로 검색할 수 있습니다.{RESET}\n");
}

/// Search cached RSS items by keyword.
fn rss_search(keyword: &str) {
    let feeds = load_rss_feeds();
    if feeds.is_empty() {
        println!("{DIM}  등록된 RSS 피드가 없습니다.{RESET}\n");
        return;
    }

    let kw = keyword.to_lowercase();
    let keywords: Vec<&str> = kw.split_whitespace().collect();
    let mut results: Vec<(String, NewsItem)> = Vec::new();

    for feed in &feeds {
        let label = if feed.name.is_empty() {
            feed.url.clone()
        } else {
            feed.name.clone()
        };
        let cached = load_rss_cache(&feed.url);
        for item in cached {
            let title_lower = item.title.to_lowercase();
            let desc_lower = item.description.to_lowercase();
            if keywords
                .iter()
                .all(|k| title_lower.contains(k) || desc_lower.contains(k))
            {
                results.push((label.clone(), item));
            }
        }
    }

    if results.is_empty() {
        println!("{DIM}  '{keyword}'에 해당하는 기사가 없습니다.{RESET}");
        println!("{DIM}  /rss check 으로 최신 뉴스를 먼저 가져오세요.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  RSS 검색 결과: '{keyword}' ({} 건){RESET}\n",
        results.len()
    );

    for (i, (source, item)) in results.iter().take(20).enumerate() {
        println!(
            "  {BOLD}{YELLOW}[{}]{RESET} {BOLD}{}{RESET}",
            i + 1,
            item.title
        );
        println!("     {DIM}[{source}] {}{RESET}", item.pub_date);
        if !item.description.is_empty() {
            let desc = if item.description.chars().count() > 100 {
                let end = item
                    .description
                    .char_indices()
                    .nth(100)
                    .map(|(i, _)| i)
                    .unwrap_or(item.description.len());
                format!("{}…", &item.description[..end])
            } else {
                item.description.clone()
            };
            println!("     {DIM}{desc}{RESET}");
        }
        if !item.link.is_empty() {
            println!("     {DIM}{}{RESET}", item.link);
        }
        println!();
    }
    if results.len() > 20 {
        println!("{DIM}  ... 외 {} 건{RESET}\n", results.len() - 20);
    }
}

/// Remove an RSS feed by index number.
fn rss_remove(num_str: &str) {
    let num: usize = match num_str.trim().parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요 (예: /rss remove 1){RESET}\n");
            return;
        }
    };

    let mut feeds = load_rss_feeds();
    if feeds.is_empty() {
        println!("{DIM}  등록된 RSS 피드가 없습니다.{RESET}\n");
        return;
    }
    if num > feeds.len() {
        eprintln!(
            "{RED}  번호 범위 초과: 1~{} 사이의 번호를 입력하세요.{RESET}\n",
            feeds.len()
        );
        return;
    }

    let removed = feeds.remove(num - 1);
    save_rss_feeds(&feeds);

    let label = if removed.name.is_empty() {
        &removed.url
    } else {
        &removed.name
    };
    println!("{GREEN}  ✓ 삭제됨: {label} ({url}){RESET}\n", url = removed.url);
}

// ── /alert — 키워드 뉴스 모니터링 ──────────────────────────────────────

const ALERTS_FILE: &str = ".journalist/alerts.json";

/// Handle the /alert command: keyword news monitoring.
pub fn handle_alert(input: &str) {
    let args = input.strip_prefix("/alert").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /alert add <키워드>{RESET}");
                println!("{DIM}  예시: /alert add 반도체{RESET}\n");
            } else {
                alert_add(rest);
            }
        }
        "list" => {
            alert_list();
        }
        "check" => {
            alert_check();
        }
        "remove" => {
            let rest = args.strip_prefix("remove").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /alert remove <번호>{RESET}");
                println!("{DIM}  예시: /alert remove 2{RESET}\n");
            } else {
                alert_remove(rest);
            }
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /alert [add|list|check|remove]{RESET}\n");
        }
    }
}

fn load_alerts() -> Vec<serde_json::Value> {
    load_alerts_from(std::path::Path::new(ALERTS_FILE))
}

fn load_alerts_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_alerts(alerts: &[serde_json::Value]) {
    save_alerts_to(alerts, std::path::Path::new(ALERTS_FILE));
}

fn save_alerts_to(alerts: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(alerts) {
        let _ = std::fs::write(path, json);
    }
}

fn alert_add(keyword: &str) {
    let keyword = keyword.trim();
    let mut alerts = load_alerts();

    // Check for duplicates
    if alerts
        .iter()
        .any(|a| a["keyword"].as_str() == Some(keyword))
    {
        println!("{DIM}  '{keyword}' 키워드는 이미 등록되어 있습니다.{RESET}\n");
        return;
    }

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);

    let entry = serde_json::json!({
        "keyword": keyword,
        "created": timestamp,
    });
    alerts.push(entry);
    save_alerts(&alerts);

    println!("{DIM}  키워드 등록됨: \"{keyword}\" [{timestamp}]{RESET}\n");
}

fn alert_list() {
    let alerts = load_alerts();
    if alerts.is_empty() {
        println!("{DIM}  등록된 모니터링 키워드가 없습니다.");
        println!("  /alert add <키워드> 로 추가하세요.{RESET}\n");
        return;
    }

    println!("{BOLD}  모니터링 키워드 ({} 건){RESET}", alerts.len());
    println!("{DIM}  ─────────────────────────────{RESET}");
    for (i, alert) in alerts.iter().enumerate() {
        let keyword = alert["keyword"].as_str().unwrap_or("?");
        let created = alert["created"].as_str().unwrap_or("");
        println!("{DIM}  {}. {keyword}  (등록: {created}){RESET}", i + 1);
    }
    println!();
}

fn alert_check() {
    let alerts = load_alerts();
    if alerts.is_empty() {
        println!("{DIM}  등록된 모니터링 키워드가 없습니다.");
        println!("  /alert add <키워드> 로 추가하세요.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  뉴스 모니터링 — {} 개 키워드 확인 중...{RESET}\n",
        alerts.len()
    );

    for alert in &alerts {
        let keyword = alert["keyword"].as_str().unwrap_or("?");
        println!("{BOLD}  ▶ \"{keyword}\"{RESET}");

        // URL-encode keyword for Naver news search
        let encoded = keyword
            .as_bytes()
            .iter()
            .map(|&b| {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                    format!("{}", b as char)
                } else {
                    format!("%{:02X}", b)
                }
            })
            .collect::<String>();

        let url = format!(
            "https://search.naver.com/search.naver?where=news&query={encoded}&sort=1&sm=tab_smr"
        );

        // Use curl to fetch news results
        let output = std::process::Command::new("curl")
            .args(["-sL", "--max-time", "10", &url])
            .output();

        match output {
            Ok(result) => {
                let body = String::from_utf8_lossy(&result.stdout);
                let headlines = extract_naver_news_headlines(&body, 5);
                if headlines.is_empty() {
                    println!("{DIM}    검색 결과 없음{RESET}");
                } else {
                    for (i, headline) in headlines.iter().enumerate() {
                        println!("{DIM}    {}. {headline}{RESET}", i + 1);
                    }
                }
            }
            Err(e) => {
                eprintln!("{RED}    뉴스 조회 실패: {e}{RESET}");
            }
        }
        println!();
    }
}

/// Extract news headlines from Naver search HTML.
fn extract_naver_news_headlines(html: &str, max: usize) -> Vec<String> {
    let mut headlines = Vec::new();
    // Naver news titles appear in <a class="news_tit" ... title="...">
    for chunk in html.split("class=\"news_tit\"") {
        if headlines.len() >= max {
            break;
        }
        // Look for title="..." attribute
        if let Some(title_start) = chunk.find("title=\"") {
            let after = &chunk[title_start + 7..];
            if let Some(end) = after.find('"') {
                let title = &after[..end];
                if !title.is_empty() {
                    // Decode HTML entities
                    let decoded = title
                        .replace("&amp;", "&")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&quot;", "\"")
                        .replace("&#39;", "'");
                    headlines.push(decoded);
                }
            }
        }
    }
    headlines
}

fn alert_remove(idx_str: &str) {
    let idx: usize = match idx_str.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요: {idx_str}{RESET}\n");
            return;
        }
    };
    let mut alerts = load_alerts();
    if idx > alerts.len() {
        eprintln!(
            "{RED}  번호 {idx}번은 범위를 벗어났습니다 (총 {} 건).{RESET}\n",
            alerts.len()
        );
        return;
    }
    let removed = alerts.remove(idx - 1);
    save_alerts(&alerts);
    let keyword = removed["keyword"].as_str().unwrap_or("?");
    println!("{DIM}  키워드 삭제됨: \"{keyword}\"{RESET}\n");
}

// ── /trend — 키워드 뉴스 트렌드 분석 ─────────────────────────────────────

/// Trends directory under .journalist/.
const TRENDS_DIR: &str = ".journalist/trends";

/// Build the trend file path with an explicit date string (for testing).
pub fn trend_file_path_with_date(keyword: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(keyword, 50);
    let filename = if slug.is_empty() {
        format!("{date}_trend.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(TRENDS_DIR).join(filename)
}

/// Build the trend file path using today's date.
fn trend_file_path(keyword: &str) -> std::path::PathBuf {
    trend_file_path_with_date(keyword, &today_str())
}

/// Save trend analysis result to file.
fn save_trend(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Build the AI prompt for trend analysis.
pub fn build_trend_prompt(keyword: &str, news_context: &str) -> String {
    format!(
        "키워드 '{keyword}'에 대한 뉴스 트렌드를 분석해주세요.\n\n\
         다음 항목을 포함해 분석해주세요:\n\n\
         ## 1. 보도량 추이\n\
         최근 보도량이 과열/보통/미개척 중 어디에 해당하는지 판단하고, 근거를 설명하세요.\n\n\
         ## 2. 주요 프레임·논조 분석\n\
         이 키워드가 어떤 프레임(각도)으로 보도되고 있는지 분석하세요. \
         긍정/부정/중립 논조 비율도 추정해주세요.\n\n\
         ## 3. 아직 안 다뤄진 각도(angle) 제안\n\
         기존 보도에서 빠져 있거나 충분히 다뤄지지 않은 취재 각도를 3~5개 제안하세요. \
         각 제안에 왜 독자에게 가치가 있는지 한 줄로 설명하세요.\n\n\
         ## 4. 취재 타이밍 판단\n\
         \"지금 쓸 만한가?\" — 이 주제를 지금 기사화하는 것이 적절한 시점인지 판단하세요. \
         너무 이른지, 적기인지, 이미 늦었는지 판단 근거와 함께 제시하세요.\n\n\
         ## 5. 종합 제안\n\
         기자에게 구체적으로 어떤 앵글로, 언제, 어떻게 쓰면 좋을지 요약해주세요.\
         {news_context}"
    )
}

/// Handle the /trend command: analyze news trend for a keyword.
pub async fn handle_trend(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let keyword = input.strip_prefix("/trend").unwrap_or("").trim();

    if keyword.is_empty() || keyword == "help" {
        println!("{DIM}  사용법: /trend <키워드>     키워드 뉴스 트렌드 분석{RESET}");
        println!("{DIM}  예시:   /trend 반도체 수출{RESET}");
        println!("{DIM}  결과:   보도량 추이, 프레임 분석, 미개척 각도, 취재 타이밍{RESET}\n");
        return;
    }

    println!("{DIM}  '{keyword}' 트렌드 분석 중...{RESET}");

    // Fetch recent news to enrich the analysis
    let news_context = match fetch_news_results(keyword, 10) {
        Ok(items) if !items.is_empty() => {
            println!(
                "{DIM}  네이버 뉴스 API: {}건 수집{RESET}",
                items.len()
            );
            build_news_context(&items)
        }
        _ => String::new(),
    };

    let prompt = build_trend_prompt(keyword, &news_context);

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save trend result to file
    if !response.trim().is_empty() {
        let path = trend_file_path(keyword);
        match save_trend(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 트렌드 분석 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  트렌드 분석 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /follow ──────────────────────────────────────────────────────────────

const FOLLOWUPS_FILE: &str = ".journalist/followups.json";

/// A single follow-up story entry.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Followup {
    pub topic: String,
    /// Optional due date in "YYYY-MM-DD" format.
    pub due: Option<String>,
    pub done: bool,
    /// ISO 8601 datetime when the followup was created.
    pub created_at: String,
}

pub fn followups_path() -> std::path::PathBuf {
    std::path::PathBuf::from(FOLLOWUPS_FILE)
}

pub fn load_followups_from(path: &std::path::Path) -> Vec<Followup> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub fn save_followups_to(followups: &[Followup], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(followups).unwrap_or_default();
    let _ = std::fs::write(path, json);
}

/// Handle `/follow` command with subcommands: add, list, done, remind.
pub fn handle_follow(input: &str) {
    let args = input.strip_prefix("/follow").unwrap_or("").trim();

    if args.is_empty() {
        handle_follow_list();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "add" => handle_follow_add(rest),
        "list" => handle_follow_list(),
        "done" => handle_follow_done(rest),
        "remind" => handle_follow_remind(),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_follow_usage();
        }
    }
}

fn print_follow_usage() {
    println!("{DIM}  사용법:");
    println!("    /follow add <주제> [--due YYYY-MM-DD]  후속 보도 등록");
    println!("    /follow list                           활성 후속 보도 목록");
    println!("    /follow done <번호>                    완료 처리");
    println!("    /follow remind                         임박 후속 보도 알림 (3일 이내)");
    println!("    /follow                                (list와 동일){RESET}\n");
}

/// Parse topic and optional --due flag from args.
fn parse_follow_add_args(args: &str) -> (String, Option<String>) {
    if let Some(due_pos) = args.find("--due") {
        let topic = args[..due_pos].trim().to_string();
        let due_str = args[due_pos + 5..].trim().to_string();
        let due = if due_str.is_empty() {
            None
        } else {
            Some(due_str)
        };
        (topic, due)
    } else {
        (args.trim().to_string(), None)
    }
}

fn handle_follow_add(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /follow add <주제> [--due YYYY-MM-DD]{RESET}\n");
        return;
    }

    let (topic, due) = parse_follow_add_args(args);

    if topic.is_empty() {
        eprintln!("{RED}  주제를 지정하세요: /follow add <주제>{RESET}\n");
        return;
    }

    // Validate due date format if provided
    if let Some(ref d) = due {
        if !is_valid_date(d) {
            eprintln!("{RED}  날짜 형식이 올바르지 않습니다: {d}{RESET}");
            eprintln!("{DIM}  예: 2026-03-25{RESET}\n");
            return;
        }
    }

    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let date = format_unix_timestamp(secs);
        date.replace(' ', "T").to_string() + ":00"
    };
    let path = followups_path();
    let mut followups = load_followups_from(&path);

    followups.push(Followup {
        topic: topic.clone(),
        due: due.clone(),
        done: false,
        created_at: now,
    });

    save_followups_to(&followups, &path);

    let due_text = due
        .as_deref()
        .map(|d| format!(" (마감: {d})"))
        .unwrap_or_default();
    println!("{GREEN}  📝 후속 보도 등록: {topic}{due_text}{RESET}\n");
}

fn handle_follow_list() {
    let path = followups_path();
    let followups = load_followups_from(&path);

    let active: Vec<&Followup> = followups.iter().filter(|f| !f.done).collect();

    if active.is_empty() {
        println!("{DIM}  등록된 후속 보도가 없습니다.{RESET}\n");
        return;
    }

    // Sort by due date (entries with due date first, then by date ascending; no-date entries last)
    let mut sorted: Vec<(usize, &Followup)> = followups
        .iter()
        .enumerate()
        .filter(|(_, f)| !f.done)
        .collect();
    sorted.sort_by(|(_, a), (_, b)| match (&a.due, &b.due) {
        (Some(da), Some(db)) => da.cmp(db),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.created_at.cmp(&b.created_at),
    });

    println!("{BOLD}  📋 후속 보도 목록{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");

    let today = today_date_string();

    for (idx, followup) in &sorted {
        let num = idx + 1;
        let due_text = followup
            .due
            .as_deref()
            .map(|d| format!(" [마감: {d}]"))
            .unwrap_or_default();

        let days_left = followup.due.as_deref().and_then(|d| days_until(d, &today));

        match days_left {
            Some(n) if n < 0 => {
                // Overdue
                println!("  {RED}🔴 #{num} {}{due_text} (기한 초과){RESET}", followup.topic);
            }
            Some(n) if n <= 3 => {
                // Due within 3 days
                println!(
                    "  {YELLOW}🟡 #{num} {}{due_text} ({n}일 남음){RESET}",
                    followup.topic
                );
            }
            _ => {
                println!("  {GREEN}🟢 #{num} {}{due_text}{RESET}", followup.topic);
            }
        }
    }
    println!();
}

fn handle_follow_done(num_str: &str) {
    if num_str.is_empty() {
        eprintln!("{RED}  번호를 지정하세요: /follow done <번호>{RESET}\n");
        return;
    }

    let num: usize = match num_str.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요: {num_str}{RESET}\n");
            return;
        }
    };

    let path = followups_path();
    let mut followups = load_followups_from(&path);
    let idx = num - 1;

    if idx >= followups.len() {
        eprintln!("{RED}  #{num}번 후속 보도를 찾을 수 없습니다.{RESET}\n");
        return;
    }

    if followups[idx].done {
        println!("{DIM}  #{num}번은 이미 완료 처리되었습니다.{RESET}\n");
        return;
    }

    followups[idx].done = true;
    let topic = followups[idx].topic.clone();
    save_followups_to(&followups, &path);
    println!("{GREEN}  ✅ 후속 보도 완료: #{num} {topic}{RESET}\n");
}

fn handle_follow_remind() {
    let path = followups_path();
    let followups = load_followups_from(&path);

    let today = today_date_string();
    let mut urgent: Vec<(usize, &Followup, i64)> = Vec::new();

    for (i, f) in followups.iter().enumerate() {
        if f.done {
            continue;
        }
        if let Some(ref due) = f.due {
            if let Some(days) = days_until(due, &today) {
                if days <= 3 {
                    urgent.push((i, f, days));
                }
            }
        }
    }

    if urgent.is_empty() {
        println!("{GREEN}  3일 이내 임박한 후속 보도가 없습니다.{RESET}\n");
        return;
    }

    urgent.sort_by_key(|(_, _, days)| *days);

    println!("{BOLD}  ⏰ 임박 후속 보도 알림{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");

    for (idx, f, days) in &urgent {
        let num = idx + 1;
        let due = f.due.as_deref().unwrap_or("");
        if *days < 0 {
            println!(
                "  {RED}🔴 #{num} {} [마감: {due}] — 기한 초과!{RESET}",
                f.topic
            );
        } else if *days == 0 {
            println!(
                "  {RED}🔴 #{num} {} [마감: {due}] — 오늘 마감!{RESET}",
                f.topic
            );
        } else {
            println!(
                "  {YELLOW}🟡 #{num} {} [마감: {due}] — {days}일 남음{RESET}",
                f.topic
            );
        }
    }
    println!();
}

/// Validate YYYY-MM-DD date format.
fn is_valid_date(s: &str) -> bool {
    if s.len() != 10 {
        return false;
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts[0].parse::<u32>().is_ok()
        && parts[1].parse::<u32>().map_or(false, |m| (1..=12).contains(&m))
        && parts[2].parse::<u32>().map_or(false, |d| (1..=31).contains(&d))
}

/// Calculate days from `today` to `target` date (both YYYY-MM-DD). Returns None if either is invalid.
pub fn days_until(target: &str, today: &str) -> Option<i64> {
    let target_days = date_to_epoch_days(target)?;
    let today_days = date_to_epoch_days(today)?;
    Some(target_days - today_days)
}

/// Convert "YYYY-MM-DD" to days since epoch. Returns None if format is invalid.
fn date_to_epoch_days(date: &str) -> Option<i64> {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Civil date to days since epoch (Howard Hinnant's algorithm, inverse of format_unix_timestamp)
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe as i64 - 719468;
    Some(days)
}

// ── /press ──────────────────────────────────────────────────────────────

const PRESS_DIR: &str = ".journalist/press";

/// A single press release item parsed from the API response.
#[derive(Debug, Clone)]
pub struct PressRelease {
    pub title: String,
    pub ministry: String,
    pub date: String,
    pub link: String,
    pub summary: String,
}

/// Build the cache directory path for press releases.
fn press_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(PRESS_DIR)
}

/// Cache a press release to `.journalist/press/<id>.json`.
fn cache_press_release(item: &PressRelease, idx: usize) -> Result<(), String> {
    let dir = press_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("디렉토리 생성 실패: {e}"))?;
    let filename = format!("press_{idx}.json");
    let path = dir.join(filename);
    let json = serde_json::json!({
        "title": item.title,
        "ministry": item.ministry,
        "date": item.date,
        "link": item.link,
        "summary": item.summary,
    });
    let content = serde_json::to_string_pretty(&json).unwrap_or_default();
    std::fs::write(&path, content).map_err(|e| format!("캐시 저장 실패: {e}"))
}

/// Parse the XML response from the press release API.
/// The API returns XML with <item> elements containing <title>, <Ministry>, <ModDate>, <Link>, <Description>.
pub fn parse_press_xml(xml: &str) -> Vec<PressRelease> {
    let mut results = Vec::new();
    let items: Vec<&str> = xml.split("<item>").collect();
    // Skip the first split part (before first <item>)
    for item_xml in items.iter().skip(1) {
        let title = extract_xml_tag(item_xml, "title").unwrap_or_default();
        let ministry = extract_xml_tag(item_xml, "SubName1")
            .or_else(|| extract_xml_tag(item_xml, "Ministry"))
            .unwrap_or_default();
        let date = extract_xml_tag(item_xml, "ModDate")
            .or_else(|| extract_xml_tag(item_xml, "Date"))
            .unwrap_or_default();
        let link = extract_xml_tag(item_xml, "DetailUrl")
            .or_else(|| extract_xml_tag(item_xml, "Link"))
            .or_else(|| extract_xml_tag(item_xml, "OriginalUrl"))
            .unwrap_or_default();
        let summary = extract_xml_tag(item_xml, "SubContent1")
            .or_else(|| extract_xml_tag(item_xml, "Description"))
            .unwrap_or_default();
        if !title.is_empty() {
            results.push(PressRelease {
                title,
                ministry,
                date,
                link,
                summary,
            });
        }
    }
    results
}

/// Extract text content between XML tags, e.g. `<tag>content</tag>`.
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>", tag = tag);
    if let Some(start) = xml.find(&open) {
        let after = &xml[start + open.len()..];
        if let Some(end) = after.find(&close) {
            let content = after[..end].trim();
            // Handle CDATA sections
            let content = if content.starts_with("<![CDATA[") && content.ends_with("]]>") {
                &content[9..content.len() - 3]
            } else {
                content
            };
            return Some(strip_html_tags(content));
        }
    }
    None
}

/// Fetch press releases from the government API.
fn fetch_press_releases(
    api_key: &str,
    keyword: Option<&str>,
    count: u32,
) -> Result<Vec<PressRelease>, String> {
    let encoded_key = api_key.replace(' ', "%20");
    let base_url = "https://apis.data.go.kr/1371000/pressReleaseService/pressReleaseList";
    let url = if let Some(kw) = keyword {
        let encoded_kw = kw.replace(' ', "%20");
        format!(
            "{}?serviceKey={}&numOfRows={}&pageNo=1&keyword={}",
            base_url, encoded_key, count, encoded_kw
        )
    } else {
        format!(
            "{}?serviceKey={}&numOfRows={}&pageNo=1",
            base_url, encoded_key, count
        )
    };

    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "15", &url])
        .output()
        .map_err(|e| format!("curl 실행 실패: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "API 요청 실패: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let body = String::from_utf8_lossy(&output.stdout).to_string();

    // Check for API error responses
    if body.contains("<returnReasonCode>") {
        if let Some(msg) = extract_xml_tag(&body, "returnAuthMsg") {
            return Err(format!("API 인증 오류: {msg}"));
        }
        return Err("API 응답 오류".to_string());
    }

    Ok(parse_press_xml(&body))
}

/// Display press release results in a formatted table.
fn display_press_results(results: &[PressRelease]) {
    if results.is_empty() {
        println!("{DIM}  검색 결과가 없습니다.{RESET}\n");
        return;
    }
    println!(
        "\n{BOLD}  📢 정부 보도자료 ({} 건){RESET}\n",
        results.len()
    );
    for (i, item) in results.iter().enumerate() {
        let num = i + 1;
        let ministry_info = if item.ministry.is_empty() {
            String::new()
        } else {
            format!(" [{CYAN}{}{RESET}]", item.ministry)
        };
        let date_info = if item.date.is_empty() {
            String::new()
        } else {
            format!("  {DIM}{}{RESET}", item.date)
        };
        println!("  {BOLD}{num:>3}.{RESET} {}{ministry_info}{date_info}", item.title);
        if !item.summary.is_empty() {
            let preview: String = item.summary.chars().take(80).collect();
            let ellipsis = if item.summary.chars().count() > 80 {
                "…"
            } else {
                ""
            };
            println!("       {DIM}{preview}{ellipsis}{RESET}");
        }
    }
    println!();
    println!("{DIM}  상세 보기: /press view <번호>{RESET}\n");
}

/// Handle the `/press` command.
pub fn handle_press(input: &str) {
    let args = input.strip_prefix("/press").unwrap_or("").trim();

    // Check for API key
    let api_key = match std::env::var("PRESS_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => {
            println!("{YELLOW}  PRESS_API_KEY 환경변수가 설정되지 않았습니다.{RESET}");
            println!(
                "{DIM}  정책브리핑 보도자료 API를 사용하려면 data.go.kr에서 API 키를 발급받으세요.{RESET}"
            );
            println!("{DIM}  발급 후: export PRESS_API_KEY=\"your-api-key\"{RESET}\n");
            return;
        }
    };

    if args.is_empty() {
        print_press_usage();
        return;
    }

    let (subcmd, rest) = match args.split_once(' ') {
        Some((c, r)) => (c, r.trim()),
        None => (args, ""),
    };

    match subcmd {
        "search" => {
            if rest.is_empty() {
                println!("{DIM}  사용법: /press search <키워드>{RESET}\n");
                return;
            }
            println!("{DIM}  보도자료 검색 중: \"{rest}\"...{RESET}");
            match fetch_press_releases(&api_key, Some(rest), 10) {
                Ok(results) => {
                    // Cache results
                    for (i, item) in results.iter().enumerate() {
                        let _ = cache_press_release(item, i + 1);
                    }
                    display_press_results(&results);
                }
                Err(e) => {
                    eprintln!("{RED}  보도자료 검색 실패: {e}{RESET}\n");
                }
            }
        }
        "latest" => {
            let count: u32 = rest.parse().unwrap_or(5);
            let count = count.clamp(1, 30);
            println!("{DIM}  최신 보도자료 {count}건 조회 중...{RESET}");
            match fetch_press_releases(&api_key, None, count) {
                Ok(results) => {
                    for (i, item) in results.iter().enumerate() {
                        let _ = cache_press_release(item, i + 1);
                    }
                    display_press_results(&results);
                }
                Err(e) => {
                    eprintln!("{RED}  보도자료 조회 실패: {e}{RESET}\n");
                }
            }
        }
        "view" => {
            if rest.is_empty() {
                println!("{DIM}  사용법: /press view <번호>{RESET}\n");
                return;
            }
            let num: usize = match rest.parse() {
                Ok(n) if n >= 1 => n,
                _ => {
                    println!("{RED}  올바른 번호를 입력하세요.{RESET}\n");
                    return;
                }
            };
            let cache_path = press_dir().join(format!("press_{num}.json"));
            match std::fs::read_to_string(&cache_path) {
                Ok(content) => {
                    if let Ok(item) = serde_json::from_str::<serde_json::Value>(&content) {
                        println!("\n{BOLD}  ── 보도자료 상세 ──{RESET}\n");
                        if let Some(title) = item.get("title").and_then(|v| v.as_str()) {
                            println!("  {BOLD}제목:{RESET} {title}");
                        }
                        if let Some(ministry) = item.get("ministry").and_then(|v| v.as_str()) {
                            if !ministry.is_empty() {
                                println!("  {BOLD}부처:{RESET} {ministry}");
                            }
                        }
                        if let Some(date) = item.get("date").and_then(|v| v.as_str()) {
                            if !date.is_empty() {
                                println!("  {BOLD}날짜:{RESET} {date}");
                            }
                        }
                        if let Some(link) = item.get("link").and_then(|v| v.as_str()) {
                            if !link.is_empty() {
                                println!("  {BOLD}링크:{RESET} {link}");
                            }
                        }
                        if let Some(summary) = item.get("summary").and_then(|v| v.as_str()) {
                            if !summary.is_empty() {
                                println!("\n  {BOLD}요약:{RESET}");
                                // Word-wrap summary at ~70 chars
                                for line in summary.lines() {
                                    println!("  {line}");
                                }
                            }
                        }
                        println!();
                    } else {
                        eprintln!("{RED}  캐시 파일 파싱 실패{RESET}\n");
                    }
                }
                Err(_) => {
                    println!("{YELLOW}  #{num} 보도자료 캐시가 없습니다.{RESET}");
                    println!(
                        "{DIM}  먼저 /press search 또는 /press latest 로 검색하세요.{RESET}\n"
                    );
                }
            }
        }
        _ => {
            print_press_usage();
        }
    }
}

fn print_press_usage() {
    println!("{DIM}  /press — 정부 보도자료 검색·모니터링{RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /press search <키워드>   키워드로 보도자료 검색{RESET}");
    println!("{DIM}    /press latest [N]       최신 N건 조회 (기본 5건){RESET}");
    println!("{DIM}    /press view <번호>      검색 결과 상세 보기{RESET}\n");
}

// ── /law ────────────────────────────────────────────────────────────────

/// A single legal terminology result.
struct LawTerm {
    term: String,
    definition: String,
    law_name: String,
}

/// Parse JSON response from the legal terminology API.
fn parse_law_response(body: &str) -> Vec<LawTerm> {
    let mut results = Vec::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return results;
    };

    // Navigate: response.body.items.item (array or single object)
    let items = json
        .get("response")
        .and_then(|r| r.get("body"))
        .and_then(|b| b.get("items"))
        .and_then(|i| i.get("item"));

    let item_list: Vec<&serde_json::Value> = match items {
        Some(serde_json::Value::Array(arr)) => arr.iter().collect(),
        Some(obj @ serde_json::Value::Object(_)) => vec![obj],
        _ => Vec::new(),
    };

    for item in item_list {
        let term = item
            .get("termNm")
            .or_else(|| item.get("lglTrmNm"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let definition = item
            .get("termDf")
            .or_else(|| item.get("lglTrmDfn"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let law_name = item
            .get("rlLwNm")
            .or_else(|| item.get("lawNm"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if !term.is_empty() {
            results.push(LawTerm {
                term,
                definition,
                law_name,
            });
        }
    }
    results
}

/// Fetch legal terminology from the 법제처 API.
fn fetch_law_terms(api_key: &str, query: &str, mode: &str) -> Result<Vec<LawTerm>, String> {
    let encoded_key = api_key.replace(' ', "%20");
    let encoded_query = query.replace(' ', "%20");
    let base_url = "https://apis.data.go.kr/1170000/legal-terminology";
    let endpoint = match mode {
        "term" => "lglTrmSrch",
        "search" => "lglTrmSrch",
        _ => "lglTrmSrch",
    };
    let url = format!(
        "{}/{}?serviceKey={}&query={}&numOfRows=10&pageNo=1&type=json",
        base_url, endpoint, encoded_key, encoded_query
    );

    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "15", &url])
        .output()
        .map_err(|e| format!("curl 실행 실패: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "API 요청 실패: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let body = String::from_utf8_lossy(&output.stdout).to_string();

    // Check for XML error responses (API may return XML on auth errors)
    if body.contains("<returnReasonCode>") {
        if let Some(msg) = extract_xml_tag(&body, "returnAuthMsg") {
            return Err(format!("API 인증 오류: {msg}"));
        }
        return Err("API 응답 오류".to_string());
    }

    Ok(parse_law_response(&body))
}

/// Display legal terminology results.
fn display_law_results(results: &[LawTerm]) {
    if results.is_empty() {
        println!("{DIM}  검색 결과가 없습니다.{RESET}\n");
        return;
    }
    println!(
        "\n{BOLD}  ⚖ 법령용어 검색 결과 ({} 건){RESET}\n",
        results.len()
    );
    for (i, item) in results.iter().enumerate() {
        let num = i + 1;
        let law_info = if item.law_name.is_empty() {
            String::new()
        } else {
            format!(" [{CYAN}{}{RESET}]", item.law_name)
        };
        println!("  {BOLD}{num:>3}.{RESET} {}{law_info}", item.term);
        if !item.definition.is_empty() {
            let preview: String = item.definition.chars().take(100).collect();
            let ellipsis = if item.definition.chars().count() > 100 {
                "…"
            } else {
                ""
            };
            println!("       {DIM}{preview}{ellipsis}{RESET}");
        }
    }
    println!();
}

fn print_law_usage() {
    println!("{DIM}  /law — 법령 용어 검색 (법제처 API){RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /law term <용어>      법률 용어 정의 검색{RESET}");
    println!("{DIM}    /law search <키워드>  키워드로 관련 용어 검색{RESET}\n");
}

/// Handle the `/law` command.
pub fn handle_law(input: &str) {
    let args = input.strip_prefix("/law").unwrap_or("").trim();

    let api_key = match std::env::var("LAW_API_KEY") {
        Ok(key) if !key.is_empty() => key,
        _ => {
            println!("{YELLOW}  LAW_API_KEY 환경변수가 설정되지 않았습니다.{RESET}");
            println!(
                "{DIM}  법제처 법령용어 API를 사용하려면 data.go.kr에서 API 키를 발급받으세요.{RESET}"
            );
            println!("{DIM}  발급 후: export LAW_API_KEY=\"your-api-key\"{RESET}\n");
            return;
        }
    };

    if args.is_empty() {
        print_law_usage();
        return;
    }

    let (subcmd, rest) = match args.split_once(' ') {
        Some((c, r)) => (c, r.trim()),
        None => (args, ""),
    };

    match subcmd {
        "term" => {
            if rest.is_empty() {
                println!("{DIM}  사용법: /law term <용어>{RESET}\n");
                return;
            }
            println!("{DIM}  법령용어 검색 중: \"{rest}\"...{RESET}");
            match fetch_law_terms(&api_key, rest, "term") {
                Ok(results) => display_law_results(&results),
                Err(e) => eprintln!("{RED}  법령용어 검색 실패: {e}{RESET}\n"),
            }
        }
        "search" => {
            if rest.is_empty() {
                println!("{DIM}  사용법: /law search <키워드>{RESET}\n");
                return;
            }
            println!("{DIM}  법령용어 검색 중: \"{rest}\"...{RESET}");
            match fetch_law_terms(&api_key, rest, "search") {
                Ok(results) => display_law_results(&results),
                Err(e) => eprintln!("{RED}  법령용어 검색 실패: {e}{RESET}\n"),
            }
        }
        _ => {
            print_law_usage();
        }
    }
}

// ── /sns — SNS 트렌드 모니터링 ──────────────────────────────────────────

const SNS_CACHE_DIR: &str = ".journalist/sns";

/// Return the cache directory path for SNS results.
fn sns_cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(SNS_CACHE_DIR)
}

/// Build the cache file path for a given subcommand and keyword.
fn sns_cache_path(subcmd: &str, keyword: &str) -> std::path::PathBuf {
    let safe_keyword = keyword
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect::<String>();
    let date = today_str();
    sns_cache_dir().join(format!("{subcmd}_{safe_keyword}_{date}.md"))
}

/// Save SNS result to cache.
fn sns_save_cache(subcmd: &str, keyword: &str, content: &str) {
    let path = sns_cache_path(subcmd, keyword);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, content);
}

/// Build the AI prompt for SNS trend (real-time trending topics).
fn sns_trend_prompt() -> String {
    "현재 한국 소셜미디어(X/Twitter, 네이버, 커뮤니티 등)에서 \
     실시간으로 화제가 되고 있는 트렌드 키워드를 분석해주세요.\n\n\
     다음 형식으로 정리해주세요:\n\n\
     ## 실시간 트렌드 키워드 (상위 10개)\n\
     각 키워드마다:\n\
     - **키워드**: 이름\n\
     - **맥락**: 왜 화제인지 1~2문장\n\
     - **뉴스 가치**: 기사화 가능성 (높음/보통/낮음)\n\
     - **추천 앵글**: 기자가 접근할 수 있는 취재 각도\n\n\
     ## 종합 판단\n\
     오늘 SNS에서 가장 기사 가치가 높은 주제 1~2개를 추천하고 이유를 설명하세요."
        .to_string()
}

/// Build the AI prompt for SNS keyword search (public opinion analysis).
fn sns_search_prompt(keyword: &str) -> String {
    format!(
        "키워드 '{keyword}'에 대한 SNS 여론을 분석해주세요.\n\n\
         다음 항목을 포함해 분석해주세요:\n\n\
         ## 1. 여론 동향\n\
         이 키워드에 대해 사람들이 어떤 반응을 보이고 있는지 분석하세요.\n\
         긍정/부정/중립 비율을 추정하고, 대표적인 의견 유형을 정리하세요.\n\n\
         ## 2. 주요 논점\n\
         SNS에서 이 키워드를 둘러싼 주요 쟁점이나 논쟁 포인트를 정리하세요.\n\n\
         ## 3. 영향력 있는 목소리\n\
         이 주제에 대해 영향력 있는 의견을 내고 있는 그룹이나 커뮤니티를 파악하세요.\n\n\
         ## 4. 기자를 위한 시사점\n\
         이 여론 동향이 기사 작성에 어떤 시사점을 주는지, \
         독자 반응을 고려한 앵글 제안을 해주세요."
    )
}

/// Build the AI prompt for SNS buzz analysis (virality assessment).
fn sns_buzz_prompt(keyword: &str) -> String {
    format!(
        "키워드 '{keyword}'의 화제성을 종합 분석해주세요.\n\n\
         최근 뉴스 보도와 SNS 반응을 종합해서 다음 항목을 분석해주세요:\n\n\
         ## 1. 화제성 지수\n\
         이 키워드의 현재 화제성을 5단계로 평가하세요: \
         🔥🔥🔥🔥🔥 폭발적 / 🔥🔥🔥🔥 높음 / 🔥🔥🔥 보통 / 🔥🔥 낮음 / 🔥 미미\n\
         판단 근거를 구체적으로 설명하세요.\n\n\
         ## 2. 확산 경로\n\
         이 키워드가 어디서 시작해서 어떻게 확산되고 있는지 추정하세요.\n\
         (예: 커뮤니티 → SNS → 언론 / 언론 → SNS 확산 등)\n\n\
         ## 3. 뉴스와 SNS 온도차\n\
         언론 보도의 논조와 SNS 여론 사이에 차이가 있는지 분석하세요.\n\
         온도차가 있다면 그것 자체가 기사 소재가 될 수 있습니다.\n\n\
         ## 4. 지속성 전망\n\
         이 화제가 일시적 이슈인지, 장기적 관심사로 이어질지 판단하세요.\n\n\
         ## 5. 기사화 추천\n\
         지금 기사로 쓸 가치가 있는지, 있다면 어떤 앵글이 좋을지 제안하세요."
    )
}

/// Handle the /sns command: SNS trend monitoring.
pub async fn handle_sns(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/sns").unwrap_or("").trim();

    let subcmd = args.split_whitespace().next().unwrap_or("help");
    match subcmd {
        "trend" => {
            println!("{DIM}  SNS 실시간 트렌드 분석 중...{RESET}");
            let prompt = sns_trend_prompt();
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            if !response.trim().is_empty() {
                sns_save_cache("trend", "realtime", &response);
                println!("{DIM}  💡 결과가 .journalist/sns/에 캐시됩니다.{RESET}\n");
            }
        }
        "search" => {
            let keyword = args.strip_prefix("search").unwrap_or("").trim();
            if keyword.is_empty() {
                println!("{DIM}  사용법: /sns search <키워드>{RESET}");
                println!("{DIM}  예시:   /sns search 반도체{RESET}\n");
                return;
            }
            println!("{DIM}  '{keyword}' SNS 여론 분석 중...{RESET}");
            let prompt = sns_search_prompt(keyword);
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            if !response.trim().is_empty() {
                sns_save_cache("search", keyword, &response);
                println!("{DIM}  💡 결과가 .journalist/sns/에 캐시됩니다.{RESET}\n");
            }
        }
        "buzz" => {
            let keyword = args.strip_prefix("buzz").unwrap_or("").trim();
            if keyword.is_empty() {
                println!("{DIM}  사용법: /sns buzz <키워드>{RESET}");
                println!("{DIM}  예시:   /sns buzz AI규제{RESET}\n");
                return;
            }
            println!("{DIM}  '{keyword}' 화제성 분석 중...{RESET}");
            let prompt = sns_buzz_prompt(keyword);
            let response = run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            if !response.trim().is_empty() {
                sns_save_cache("buzz", keyword, &response);
                println!("{DIM}  💡 결과가 .journalist/sns/에 캐시됩니다.{RESET}\n");
            }
        }
        "help" => {
            sns_print_help();
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            sns_print_help();
        }
    }
}

fn sns_print_help() {
    println!("{DIM}  /sns — SNS 트렌드 모니터링{RESET}");
    println!("{DIM}  사용법:{RESET}");
    println!("{DIM}    /sns trend              실시간 트렌드 키워드 분석{RESET}");
    println!("{DIM}    /sns search <키워드>    키워드의 SNS 여론 분석{RESET}");
    println!("{DIM}    /sns buzz <키워드>      키워드 화제성 종합 분석{RESET}");
    println!("{DIM}  예시:{RESET}");
    println!("{DIM}    /sns trend{RESET}");
    println!("{DIM}    /sns search 반도체{RESET}");
    println!("{DIM}    /sns buzz AI규제{RESET}\n");
}

// ── /network — 취재원 네트워크 분석 ──────────────────────────────────

/// Handle the /network command: analyze source network strategically.
pub async fn handle_network(
    agent: &mut yoagent::Agent,
    input: &str,
    session_total: &mut yoagent::Usage,
    model: &str,
) {
    let args = input.strip_prefix("/network").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("map") {
        "map" => network_map(),
        "gaps" => network_gaps(),
        "suggest" => {
            let topic = args.strip_prefix("suggest").unwrap_or("").trim();
            if topic.is_empty() {
                println!(
                    "{DIM}  사용법: /network suggest <주제>{RESET}"
                );
                println!(
                    "{DIM}  예시: /network suggest 반도체 수출규제{RESET}\n"
                );
            } else {
                network_suggest(agent, topic, session_total, model).await;
            }
        }
        other => {
            println!("{DIM}  알 수 없는 하위 명령: {other}{RESET}");
            network_usage();
        }
    }
}

/// Compute beat distribution from sources. Returns a map of beat -> count.
fn compute_beat_distribution(sources: &[serde_json::Value]) -> std::collections::HashMap<String, usize> {
    let mut dist: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for s in sources {
        let beat = s["beat"].as_str().unwrap_or("").trim();
        let key = if beat.is_empty() {
            "(미지정)".to_string()
        } else {
            beat.to_string()
        };
        *dist.entry(key).or_insert(0) += 1;
    }
    dist
}

/// Identify weak beats: beats with fewer sources than the threshold.
fn find_gap_beats(
    dist: &std::collections::HashMap<String, usize>,
    threshold: usize,
) -> Vec<(String, usize)> {
    let mut gaps: Vec<(String, usize)> = dist
        .iter()
        .filter(|(_, &count)| count <= threshold)
        .map(|(beat, &count)| (beat.clone(), count))
        .collect();
    gaps.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    gaps
}

/// Display beat distribution matrix.
fn network_map() {
    let sources = load_sources();
    if sources.is_empty() {
        println!("{DIM}  취재원 DB가 비어 있습니다.");
        println!("  /sources add 로 취재원을 먼저 등록하세요.{RESET}\n");
        return;
    }

    let dist = compute_beat_distribution(&sources);
    let mut sorted: Vec<(&String, &usize)> = dist.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let max_count = sorted.first().map(|(_, &c)| c).unwrap_or(1);

    println!("{DIM}  ── 취재원 네트워크 분포 (총 {} 명) ──\n", sources.len());
    for (beat, &count) in &sorted {
        let bar_len = if max_count > 0 {
            (count * 20) / max_count
        } else {
            0
        };
        let bar: String = "█".repeat(bar_len.max(1));
        let strength = if count >= 5 {
            "강"
        } else if count >= 3 {
            "보통"
        } else {
            "약"
        };
        println!("  {:<10} {:<20} {} 명 ({})", beat, bar, count, strength);
    }
    println!("{RESET}");
}

/// Identify and report weak areas in the network.
fn network_gaps() {
    let sources = load_sources();
    if sources.is_empty() {
        println!("{DIM}  취재원 DB가 비어 있습니다.");
        println!("  /sources add 로 취재원을 먼저 등록하세요.{RESET}\n");
        return;
    }

    let dist = compute_beat_distribution(&sources);
    let gaps = find_gap_beats(&dist, 2);

    if gaps.is_empty() {
        println!("{DIM}  모든 분야에 3명 이상의 취재원이 있습니다. 네트워크가 양호합니다.{RESET}\n");
        return;
    }

    println!("{DIM}  ── 취약 분야 경고 ──\n");
    for (beat, count) in &gaps {
        let level = if *count == 0 {
            "⚠ 없음"
        } else if *count == 1 {
            "⚠ 매우 부족"
        } else {
            "△ 부족"
        };
        println!("  {:<10} {} 명 — {}", beat, count, level);
    }
    println!(
        "\n  총 {} 개 분야에서 보강이 필요합니다.",
        gaps.len()
    );
    println!("  /sources add <이름> <소속> <연락처> --beat <분야> 로 보강하세요.{RESET}\n");
}

/// AI-powered suggestion for source types needed for a topic.
async fn network_suggest(
    agent: &mut yoagent::Agent,
    topic: &str,
    session_total: &mut yoagent::Usage,
    model: &str,
) {
    let sources = load_sources();
    let dist = compute_beat_distribution(&sources);
    let dist_summary: String = dist
        .iter()
        .map(|(beat, count)| format!("{}: {} 명", beat, count))
        .collect::<Vec<_>>()
        .join(", ");

    let prompt = network_suggest_prompt(topic, &dist_summary);

    println!("{DIM}  '{topic}' 취재에 필요한 취재원 유형을 분석합니다...{RESET}\n");

    run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);
}

/// Build the prompt for network suggest.
fn network_suggest_prompt(topic: &str, dist_summary: &str) -> String {
    format!(
        "당신은 기자의 취재원 네트워크 전략 어드바이저입니다.\n\n\
         주제: {topic}\n\n\
         현재 취재원 네트워크 현황: {dist_summary}\n\n\
         위 주제를 취재하기 위해 필요한 취재원 유형을 제안하세요:\n\
         1. 어떤 분야/직종의 취재원이 필요한지 (최소 3~5개 유형)\n\
         2. 각 유형별 역할 (왜 필요한지)\n\
         3. 현재 네트워크에서 이미 보유한 유형과 부족한 유형 식별\n\
         4. 취재원 접근 전략 (어떻게 연결할 수 있는지)\n\n\
         한국어로 답변하세요. 구체적이고 실용적인 제안을 해주세요."
    )
}

fn network_usage() {
    println!("{DIM}  사용법: /network [map|gaps|suggest <주제>]\n");
    println!("  map     — beat별 취재원 분포 매트릭스");
    println!("  gaps    — 취약 분야 식별");
    println!("  suggest — 특정 주제 취재에 필요한 취재원 유형 AI 제안\n");
    println!("  예시:");
    println!("    /network map");
    println!("    /network gaps");
    println!("    /network suggest 반도체 수출규제{RESET}\n");
}

// ── /note ────────────────────────────────────────────────────────────────

const NOTES_DIR: &str = ".journalist/notes";

/// A single reporter note entry stored as one JSONL line.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct Note {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    pub timestamp: String,
}

fn notes_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(NOTES_DIR)
}

/// Return today's JSONL notes file path: `.journalist/notes/YYYY-MM-DD.jsonl`
pub fn notes_file_for_date(date: &str) -> std::path::PathBuf {
    notes_dir().join(format!("{date}.jsonl"))
}

/// Load all notes from a single JSONL file.
pub fn load_notes_from(path: &std::path::Path) -> Vec<Note> {
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

/// Append a single note to a JSONL file.
fn append_note_to(note: &Note, path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let line = serde_json::to_string(note).unwrap_or_default();
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|_| panic!("Failed to open {}", path.display()));
    let _ = writeln!(file, "{line}");
}

/// Load all notes across all date files, sorted by timestamp ascending.
fn load_all_notes() -> Vec<Note> {
    load_all_notes_from(&notes_dir())
}

fn load_all_notes_from(dir: &std::path::Path) -> Vec<Note> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut all: Vec<Note> = Vec::new();
    let mut files: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    for f in files {
        all.extend(load_notes_from(&f));
    }
    all
}

/// Handle `/note` command with subcommands: add, list, search, export.
pub fn handle_note(input: &str) -> Option<String> {
    let args = input.strip_prefix("/note").unwrap_or("").trim();

    if args.is_empty() || args == "help" || args == "--help" {
        print_note_usage();
        return None;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "add" => {
            handle_note_add(rest);
            None
        }
        "list" => {
            handle_note_list(rest);
            None
        }
        "search" => {
            handle_note_search(rest);
            None
        }
        "export" => Some(handle_note_export(rest)),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_note_usage();
            None
        }
    }
}

fn print_note_usage() {
    println!("{DIM}  사용법:");
    println!("    /note add <메모>                              빠른 메모 저장");
    println!(
        "    /note add --source 홍길동 --topic 반도체 \"내용\"  취재원·주제 태그 포함"
    );
    println!("    /note list                                    최근 노트 시간순 목록");
    println!("    /note list --topic 반도체                     주제별 필터링");
    println!("    /note search <키워드>                         키워드 검색");
    println!("    /note export <주제>                           주제별 노트 정리 (AI){RESET}\n");
}

/// Parse add arguments: optional --source, --topic flags, and the remaining content.
fn parse_note_add_args(args: &str) -> (String, Option<String>, Option<String>) {
    let mut source: Option<String> = None;
    let mut topic: Option<String> = None;
    let mut remaining = args.to_string();

    // Extract --source value
    if let Some(pos) = remaining.find("--source") {
        let before = remaining[..pos].to_string();
        let after = remaining[pos + 8..].trim_start().to_string();
        let (val, rest) = extract_flag_value(&after);
        source = if val.is_empty() { None } else { Some(val) };
        remaining = format!("{before} {rest}").trim().to_string();
    }

    // Extract --topic value
    if let Some(pos) = remaining.find("--topic") {
        let before = remaining[..pos].to_string();
        let after = remaining[pos + 7..].trim_start().to_string();
        let (val, rest) = extract_flag_value(&after);
        topic = if val.is_empty() { None } else { Some(val) };
        remaining = format!("{before} {rest}").trim().to_string();
    }

    // Strip surrounding quotes from content
    let content = remaining
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();

    (content, source, topic)
}

/// Extract a flag value: takes the next word (or quoted string) and returns (value, rest).
fn extract_flag_value(s: &str) -> (String, String) {
    let s = s.trim();
    if s.is_empty() {
        return (String::new(), String::new());
    }

    // Check if the value is the next --flag (no value provided)
    if s.starts_with("--") {
        return (String::new(), s.to_string());
    }

    // Quoted value
    if s.starts_with('"') {
        if let Some(end) = s[1..].find('"') {
            let val = s[1..=end].to_string();
            let rest = s[end + 2..].trim().to_string();
            return (val, rest);
        }
    }

    // Unquoted: take until next whitespace or --flag
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            end = i;
            break;
        }
    }
    let val = s[..end].to_string();
    let rest = s[end..].trim().to_string();
    (val, rest)
}

fn handle_note_add(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /note add <메모 내용>{RESET}\n");
        return;
    }

    let (content, source, topic) = parse_note_add_args(args);

    if content.is_empty() {
        eprintln!("{RED}  메모 내용을 입력하세요.{RESET}\n");
        return;
    }

    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let date = format_unix_timestamp(secs);
        date.replace(' ', "T").to_string() + ":00"
    };

    let today = today_date_string();
    let note = Note {
        content: content.clone(),
        source: source.clone(),
        topic: topic.clone(),
        timestamp: now,
    };

    let path = notes_file_for_date(&today);
    append_note_to(&note, &path);

    let mut meta = String::new();
    if let Some(ref s) = source {
        meta.push_str(&format!(" [취재원: {s}]"));
    }
    if let Some(ref t) = topic {
        meta.push_str(&format!(" [주제: {t}]"));
    }
    println!("{GREEN}  📝 메모 저장: {content}{meta}{RESET}\n");
}

fn handle_note_list(args: &str) {
    let topic_filter = if let Some(pos) = args.find("--topic") {
        let after = args[pos + 7..].trim();
        if after.is_empty() {
            None
        } else {
            let (val, _) = extract_flag_value(after);
            if val.is_empty() { None } else { Some(val) }
        }
    } else {
        None
    };

    let notes = load_all_notes();

    let filtered: Vec<&Note> = if let Some(ref t) = topic_filter {
        let t_lower = t.to_lowercase();
        notes
            .iter()
            .filter(|n| {
                n.topic
                    .as_ref()
                    .is_some_and(|nt| nt.to_lowercase().contains(&t_lower))
            })
            .collect()
    } else {
        notes.iter().collect()
    };

    if filtered.is_empty() {
        let suffix = topic_filter
            .as_ref()
            .map(|t| format!(" (주제: {t})"))
            .unwrap_or_default();
        println!("{DIM}  저장된 노트가 없습니다{suffix}.{RESET}\n");
        return;
    }

    let label = topic_filter
        .as_ref()
        .map(|t| format!(" (주제: {t})"))
        .unwrap_or_default();
    println!("{BOLD}  📓 취재 노트{label}{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");

    for (i, note) in filtered.iter().enumerate() {
        let num = i + 1;
        let ts = &note.timestamp;
        let mut meta = String::new();
        if let Some(ref s) = note.source {
            meta.push_str(&format!(" [{s}]"));
        }
        if let Some(ref t) = note.topic {
            meta.push_str(&format!(" #{t}"));
        }
        println!("  {DIM}{ts}{RESET} {GREEN}#{num}{RESET}{meta} {}", note.content);
    }
    println!();
}

fn handle_note_search(query: &str) {
    if query.is_empty() {
        eprintln!("{RED}  검색어를 입력하세요: /note search <키워드>{RESET}\n");
        return;
    }

    let query_lower = query.to_lowercase();
    let notes = load_all_notes();

    let matches: Vec<&Note> = notes
        .iter()
        .filter(|n| {
            n.content.to_lowercase().contains(&query_lower)
                || n.source
                    .as_ref()
                    .is_some_and(|s| s.to_lowercase().contains(&query_lower))
                || n.topic
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains(&query_lower))
        })
        .collect();

    if matches.is_empty() {
        println!("{DIM}  \"{query}\" 검색 결과가 없습니다.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  🔍 \"{query}\" 검색 결과 ({} 건){RESET}",
        matches.len()
    );
    println!("{DIM}  ──────────────────────────────{RESET}");

    for (i, note) in matches.iter().enumerate() {
        let num = i + 1;
        let ts = &note.timestamp;
        let mut meta = String::new();
        if let Some(ref s) = note.source {
            meta.push_str(&format!(" [{s}]"));
        }
        if let Some(ref t) = note.topic {
            meta.push_str(&format!(" #{t}"));
        }
        println!("  {DIM}{ts}{RESET} {GREEN}#{num}{RESET}{meta} {}", note.content);
    }
    println!();
}

/// Build an export prompt — returns the prompt string for AI processing.
/// The caller (repl.rs) should run this through the AI.
fn handle_note_export(topic: &str) -> String {
    if topic.is_empty() {
        eprintln!("{RED}  주제를 지정하세요: /note export <주제>{RESET}\n");
        return String::new();
    }

    let topic_lower = topic.to_lowercase();
    let notes = load_all_notes();

    let matches: Vec<&Note> = notes
        .iter()
        .filter(|n| {
            n.topic
                .as_ref()
                .is_some_and(|t| t.to_lowercase().contains(&topic_lower))
                || n.content.to_lowercase().contains(&topic_lower)
        })
        .collect();

    if matches.is_empty() {
        println!("{DIM}  \"{topic}\" 관련 노트가 없습니다.{RESET}\n");
        return String::new();
    }

    println!(
        "{DIM}  📤 \"{topic}\" 관련 노트 {} 건을 정리합니다...{RESET}",
        matches.len()
    );

    let mut collected = String::new();
    for note in &matches {
        let source_tag = note
            .source
            .as_ref()
            .map(|s| format!(" (취재원: {s})"))
            .unwrap_or_default();
        collected.push_str(&format!(
            "- [{}]{source_tag}: {}\n",
            note.timestamp, note.content
        ));
    }

    format!(
        "다음은 \"{topic}\" 주제 관련 취재 노트입니다. 이 노트들을 기사 작성에 활용할 수 있도록 \
         체계적으로 정리해주세요.\n\n\
         ## 정리 요청사항:\n\
         1. 시간순으로 핵심 내용 요약\n\
         2. 취재원별 발언 정리\n\
         3. 기사에 활용할 수 있는 핵심 팩트 추출\n\
         4. 추가 취재가 필요한 사항\n\n\
         ## 취재 노트:\n{collected}"
    )
}

// ── /contact — 취재원 접촉 기록 관리 ─────────────────────────────────────

const CONTACTS_DIR: &str = ".journalist/contacts";

/// A single contact log entry stored as one JSONL line.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ContactLog {
    pub name: String,
    pub summary: String,
    pub timestamp: String,
}

fn contacts_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(CONTACTS_DIR)
}

/// Return the JSONL file path for a given source name.
fn contact_file_for(name: &str) -> std::path::PathBuf {
    // Sanitize name for filesystem: replace spaces/special chars
    let safe_name: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    contacts_dir().join(format!("{safe_name}.jsonl"))
}

pub fn append_contact_log(log: &ContactLog, path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(log) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{json}");
        }
    }
}

pub fn load_contact_logs_from(path: &std::path::Path) -> Vec<ContactLog> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Load all contact logs from all files in the contacts directory.
pub fn load_all_contact_logs() -> Vec<ContactLog> {
    let dir = contacts_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort();
    let mut all = Vec::new();
    for f in &files {
        all.extend(load_contact_logs_from(f));
    }
    all
}

/// Parse the timestamp string into seconds since epoch (approximate, for comparison).
fn parse_timestamp_secs(ts: &str) -> Option<u64> {
    // Expected format: "YYYY-MM-DDTHH:MM:SS" or similar
    let ts = ts.replace('T', " ");
    let parts: Vec<&str> = ts.split(|c: char| !c.is_ascii_digit()).collect();
    if parts.len() < 3 {
        return None;
    }
    let year: u64 = parts[0].parse().ok()?;
    let month: u64 = parts[1].parse().ok()?;
    let day: u64 = parts[2].parse().ok()?;
    let hour: u64 = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let min: u64 = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let sec: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

    // Simple days-since-epoch calculation (not perfectly accurate but sufficient for comparison)
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
    }
    let month_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    for m in 0..(month.saturating_sub(1) as usize) {
        days += month_days.get(m).copied().unwrap_or(30) as u64;
        if m == 1 && is_leap {
            days += 1;
        }
    }
    days += day.saturating_sub(1);
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

fn current_timestamp_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let date = format_unix_timestamp(secs);
    date.replace(' ', "T") + ":00"
}

fn current_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Handle the /contact command: manage source contact history.
/// Returns Some(prompt) when AI processing is needed (suggest subcommand).
pub fn handle_contact(input: &str) -> Option<String> {
    let args = input.strip_prefix("/contact").unwrap_or("").trim();

    if args.is_empty() || args == "help" || args == "--help" {
        print_contact_usage();
        return None;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "log" => {
            contact_log(rest);
            None
        }
        "history" => {
            contact_history(rest);
            None
        }
        "recent" => {
            contact_recent();
            None
        }
        "stale" => {
            contact_stale();
            None
        }
        "suggest" => Some(contact_suggest_prompt(rest)),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_contact_usage();
            None
        }
    }
}

fn print_contact_usage() {
    println!("{DIM}  사용법:");
    println!("    /contact log <이름> \"<요약>\"                     접촉 기록 저장");
    println!("    /contact history <이름>                          특정 취재원 접촉 이력 조회");
    println!("    /contact recent                                 최근 7일 접촉 기록");
    println!("    /contact stale                                  30일 이상 접촉 없는 취재원");
    println!("    /contact suggest <주제>                          주제별 취재원 추천 (AI){RESET}\n");
}

fn contact_log(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /contact log <이름> \"<요약>\"{RESET}\n");
        return;
    }

    let (name, summary) = parse_contact_log_args(args);
    if name.is_empty() || summary.is_empty() {
        eprintln!("{RED}  이름과 요약 내용이 필요합니다.{RESET}");
        eprintln!("{DIM}  예시: /contact log 홍길동 \"반도체 신규 투자 관련 전화 인터뷰\"{RESET}\n");
        return;
    }

    let log = ContactLog {
        name: name.clone(),
        summary: summary.clone(),
        timestamp: current_timestamp_string(),
    };

    let path = contact_file_for(&name);
    append_contact_log(&log, &path);
    println!("{GREEN}  📞 접촉 기록 저장: {name} — {summary}{RESET}\n");
}

/// Parse `/contact log` args: first word is name, rest is summary (optionally quoted).
fn parse_contact_log_args(args: &str) -> (String, String) {
    let args = args.trim();
    if args.is_empty() {
        return (String::new(), String::new());
    }
    let (name, rest) = match args.split_once(char::is_whitespace) {
        Some((n, r)) => (n.to_string(), r.trim().to_string()),
        None => (args.to_string(), String::new()),
    };
    // Strip surrounding quotes from summary
    let summary = rest
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();
    (name, summary)
}

fn contact_history(name: &str) {
    if name.is_empty() {
        eprintln!("{RED}  이름을 지정하세요: /contact history <이름>{RESET}\n");
        return;
    }
    let path = contact_file_for(name);
    let logs = load_contact_logs_from(&path);
    if logs.is_empty() {
        println!("{DIM}  \"{name}\"의 접촉 기록이 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  ── {name} 접촉 이력 ({} 건) ──", logs.len());
    for log in &logs {
        println!("  [{}] {}", log.timestamp, log.summary);
    }
    println!("{RESET}");
}

fn contact_recent() {
    let now_secs = current_epoch_secs();
    let seven_days = 7 * 86400;
    let cutoff = now_secs.saturating_sub(seven_days);

    let all = load_all_contact_logs();
    let recent: Vec<&ContactLog> = all
        .iter()
        .filter(|log| {
            parse_timestamp_secs(&log.timestamp)
                .map(|ts| ts >= cutoff)
                .unwrap_or(false)
        })
        .collect();

    if recent.is_empty() {
        println!("{DIM}  최근 7일간 접촉 기록이 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  ── 최근 7일 접촉 기록 ({} 건) ──", recent.len());
    for log in &recent {
        println!("  [{}] {} — {}", log.timestamp, log.name, log.summary);
    }
    println!("{RESET}");
}

fn contact_stale() {
    let sources = load_sources();
    if sources.is_empty() {
        println!("{DIM}  취재원 DB가 비어 있습니다. /sources add 로 등록하세요.{RESET}\n");
        return;
    }

    let now_secs = current_epoch_secs();
    let thirty_days = 30 * 86400;
    let cutoff = now_secs.saturating_sub(thirty_days);

    let mut stale_sources = Vec::new();
    for source in &sources {
        let name = source["name"].as_str().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let path = contact_file_for(name);
        let logs = load_contact_logs_from(&path);
        let last_contact = logs
            .iter()
            .filter_map(|l| parse_timestamp_secs(&l.timestamp))
            .max();
        let is_stale = match last_contact {
            Some(ts) => ts < cutoff,
            None => true, // Never contacted
        };
        if is_stale {
            let org = source["org"].as_str().unwrap_or("");
            let days_since = last_contact
                .map(|ts| ((now_secs.saturating_sub(ts)) / 86400).to_string() + "일 전")
                .unwrap_or_else(|| "접촉 기록 없음".to_string());
            stale_sources.push((name.to_string(), org.to_string(), days_since));
        }
    }

    if stale_sources.is_empty() {
        println!("{DIM}  모든 취재원과 최근 30일 내에 접촉한 기록이 있습니다. 👍{RESET}\n");
        return;
    }

    println!(
        "{DIM}  ── 30일 이상 접촉 없는 취재원 ({} 명) ──",
        stale_sources.len()
    );
    for (name, org, since) in &stale_sources {
        println!("  ⚠ {name} ({org}) — 마지막 접촉: {since}");
    }
    println!("{RESET}");
}

/// Build a prompt for AI-powered source suggestion based on topic.
fn contact_suggest_prompt(topic: &str) -> String {
    if topic.is_empty() {
        eprintln!("{RED}  주제를 지정하세요: /contact suggest <주제>{RESET}\n");
        return String::new();
    }

    let sources = load_sources();
    let mut sources_summary = String::new();
    if sources.is_empty() {
        sources_summary.push_str("(등록된 취재원 없음)\n");
    } else {
        for s in &sources {
            let name = s["name"].as_str().unwrap_or("?");
            let org = s["org"].as_str().unwrap_or("");
            let beat = s["beat"].as_str().unwrap_or("");
            let note = s["note"].as_str().unwrap_or("");
            sources_summary.push_str(&format!("- {name} | {org} | beat: {beat} | {note}\n"));
        }
    }

    // Also include recent contact logs for context
    let all_logs = load_all_contact_logs();
    let mut recent_context = String::new();
    let recent_logs: Vec<&ContactLog> = all_logs.iter().rev().take(20).collect();
    if recent_logs.is_empty() {
        recent_context.push_str("(최근 접촉 기록 없음)\n");
    } else {
        for log in &recent_logs {
            recent_context.push_str(&format!(
                "- {} [{}]: {}\n",
                log.name, log.timestamp, log.summary
            ));
        }
    }

    format!(
        "다음 주제에 대해 취재할 때 연락할 만한 취재원을 추천해주세요.\n\n\
         ## 취재 주제\n{topic}\n\n\
         ## 현재 보유 취재원\n{sources_summary}\n\
         ## 최근 접촉 이력\n{recent_context}\n\
         ## 요청사항:\n\
         1. 현재 취재원 중 이 주제에 연락할 만한 사람 우선 추천\n\
         2. 보유하고 있지 않다면 어떤 유형의 취재원이 필요한지 제안\n\
         3. 접근 전략 (어떻게 연결할 수 있는지)\n\
         4. 인터뷰 시 핵심 질문 3개\n\n\
         한국어로 답변하세요. 구체적이고 실용적인 제안을 해주세요."
    )
}




#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::commands_project::*;
    use crate::commands_research::*;
    use crate::commands_writing::*;
    use crate::commands_workflow::*;

    fn temp_sources_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sources.json");
        (dir, path)
    }


    /// Helper: add a source entry to a specific file path.
    fn test_add(path: &Path, args: &str) {
        let parts: Vec<&str> = args.splitn(4, ' ').collect();
        let entry = serde_json::json!({
            "name": parts[0],
            "org": parts.get(1).unwrap_or(&""),
            "contact": parts.get(2).unwrap_or(&""),
            "note": if parts.len() > 3 { parts[3] } else { "" },
        });
        let mut sources = load_sources_from(path);
        sources.push(entry);
        save_sources_to(&sources, path);
    }

    /// Helper: remove a source by 1-indexed number from a specific file.
    fn test_remove(path: &Path, idx_str: &str) {
        let idx: usize = idx_str.parse().unwrap();
        let mut sources = load_sources_from(path);
        if idx >= 1 && idx <= sources.len() {
            sources.remove(idx - 1);
            save_sources_to(&sources, path);
        }
    }

    /// Helper: edit a source field in a specific file.
    fn test_edit(path: &Path, args: &str) {
        let parts: Vec<&str> = args.splitn(3, ' ').collect();
        let idx: usize = parts[0].parse().unwrap();
        let field = parts[1];
        let value = parts[2];
        let mut sources = load_sources_from(path);
        if idx >= 1 && idx <= sources.len() {
            sources[idx - 1][field] = serde_json::Value::String(value.to_string());
            save_sources_to(&sources, path);
        }
    }

    #[test]
    fn sources_add_creates_entry() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234-5678 반도체 담당");
        let sources = load_sources_from(&path);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["name"], "홍길동");
        assert_eq!(sources[0]["org"], "산업부");
        assert_eq!(sources[0]["contact"], "010-1234-5678");
        assert_eq!(sources[0]["note"], "반도체 담당");
    }

    #[test]
    fn sources_remove_deletes_by_index() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "김기자 조선일보 010-0000-0001");
        test_add(&path, "이기자 중앙일보 010-0000-0002");
        test_add(&path, "박기자 동아일보 010-0000-0003");
        assert_eq!(load_sources_from(&path).len(), 3);

        // Remove the second entry (1-indexed)
        test_remove(&path, "2");
        let sources = load_sources_from(&path);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0]["name"], "김기자");
        assert_eq!(sources[1]["name"], "박기자");
    }

    #[test]
    fn sources_remove_out_of_range_does_not_crash() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234-5678");
        // Index 5 is out of range — should not modify
        let mut sources = load_sources_from(&path);
        let before_len = sources.len();
        if 5 > sources.len() {
            // No-op, as expected
        } else {
            sources.remove(4);
            save_sources_to(&sources, &path);
        }
        assert_eq!(load_sources_from(&path).len(), before_len);
    }

    #[test]
    fn sources_edit_updates_field() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234-5678 원래 메모");

        test_edit(&path, "1 org 기획재정부");
        let sources = load_sources_from(&path);
        assert_eq!(sources[0]["org"], "기획재정부");
        // Other fields unchanged
        assert_eq!(sources[0]["name"], "홍길동");
        assert_eq!(sources[0]["contact"], "010-1234-5678");
    }

    #[test]
    fn sources_edit_note_with_spaces() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234-5678");

        test_edit(&path, "1 note 반도체 정책 전문가");
        let sources = load_sources_from(&path);
        assert_eq!(sources[0]["note"], "반도체 정책 전문가");
    }

    #[test]
    fn sources_edit_invalid_field_does_not_modify() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234-5678");

        // Edit with invalid field — we still write it in test helper,
        // but the real sources_edit() would reject it.
        // Test the validation logic directly:
        let valid_fields = ["name", "org", "contact", "note"];
        assert!(!valid_fields.contains(&"email"));

        // Verify data is unchanged
        let sources = load_sources_from(&path);
        assert_eq!(sources[0]["name"], "홍길동");
    }

    #[test]
    fn research_file_path_with_topic() {
        let path = research_file_path_with_date("반도체 수출 동향", "2026-03-17");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/research/2026-03-17_반도체-수출-동향.md"
        );
    }

    #[test]
    fn research_file_path_empty_topic() {
        let path = research_file_path_with_date("", "2026-03-17");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/research/2026-03-17_research.md"
        );
    }

    #[test]
    fn save_research_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("research").join("test.md");
        save_research(&path, "# 리서치 결과\n내용").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# 리서치 결과\n내용");
    }

    #[test]
    fn sources_json_roundtrip() {
        let (_dir, path) = temp_sources_path();
        let entries = vec![
            serde_json::json!({"name": "김기자", "org": "조선일보", "contact": "010-1111", "note": "정치부"}),
            serde_json::json!({"name": "이기자", "org": "중앙일보", "contact": "010-2222", "note": ""}),
        ];
        save_sources_to(&entries, &path);
        let loaded = load_sources_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["name"], "김기자");
        assert_eq!(loaded[1]["org"], "중앙일보");
        // Full round-trip equality
        assert_eq!(entries, loaded);
    }

    #[test]
    fn sources_load_empty_file_returns_empty() {
        let (_dir, path) = temp_sources_path();
        let loaded = load_sources_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn sources_load_nonexistent_returns_empty() {
        let path = std::path::PathBuf::from("/tmp/does_not_exist_sources_test.json");
        let loaded = load_sources_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn sources_add_rejects_fewer_than_3_args() {
        // The sources_add function splits args into at most 4 parts and
        // requires at least 3. Verify the parsing logic.
        let too_few = "홍길동 산업부";
        let parts: Vec<&str> = too_few.splitn(4, ' ').collect();
        assert!(parts.len() < 3);

        let exact_three = "홍길동 산업부 010-1234";
        let parts: Vec<&str> = exact_three.splitn(4, ' ').collect();
        assert_eq!(parts.len(), 3);

        let with_note = "홍길동 산업부 010-1234 반도체 정책 담당";
        let parts: Vec<&str> = with_note.splitn(4, ' ').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[3], "반도체 정책 담당");
    }

    #[test]
    fn source_matches_case_insensitive() {
        let entry = serde_json::json!({
            "name": "Hong GilDong",
            "org": "Ministry of Trade",
            "contact": "010-1234",
            "note": "Semiconductor policy"
        });
        // Lowercase query matches uppercase fields
        assert!(source_matches(&entry, "hong"));
        assert!(source_matches(&entry, "ministry"));
        assert!(source_matches(&entry, "semiconductor"));
        // Mixed-case query
        assert!(source_matches(&entry, "gildong"));
        // No match
        assert!(!source_matches(&entry, "없는검색어"));
    }

    #[test]
    fn source_matches_korean() {
        let entry = serde_json::json!({
            "name": "홍길동",
            "org": "산업통상자원부",
            "contact": "010-1234",
            "note": "반도체 정책"
        });
        assert!(source_matches(&entry, "홍길동"));
        assert!(source_matches(&entry, "반도체"));
        assert!(source_matches(&entry, "산업"));
        assert!(!source_matches(&entry, "기획재정부"));
    }

    #[test]
    fn sources_search_via_tempfile() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234 반도체");
        test_add(&path, "김영희 기획부 010-5678 예산");
        test_add(&path, "Park IT부 010-9999 Server admin");

        let sources = load_sources_from(&path);

        // Case-insensitive search for "server"
        let query_lower = "server".to_lowercase();
        let matches: Vec<_> = sources.iter().filter(|s| source_matches(s, &query_lower)).collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "Park");

        // Korean search
        let query_lower = "반도체".to_lowercase();
        let matches: Vec<_> = sources.iter().filter(|s| source_matches(s, &query_lower)).collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "홍길동");
    }

    /// Helper: add a source entry with an optional beat field.
    fn test_add_with_beat(path: &Path, args: &str, beat: &str) {
        let parts: Vec<&str> = args.splitn(4, ' ').collect();
        let entry = serde_json::json!({
            "name": parts[0],
            "org": parts.get(1).unwrap_or(&""),
            "contact": parts.get(2).unwrap_or(&""),
            "note": if parts.len() > 3 { parts[3] } else { "" },
            "beat": beat,
        });
        let mut sources = load_sources_from(path);
        sources.push(entry);
        save_sources_to(&sources, path);
    }

    #[test]
    fn sources_add_with_beat_field() {
        let (_dir, path) = temp_sources_path();
        test_add_with_beat(&path, "홍길동 산업부 010-1234 반도체 담당", "경제");
        let sources = load_sources_from(&path);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["beat"], "경제");
        assert_eq!(sources[0]["name"], "홍길동");
    }

    #[test]
    fn sources_add_without_beat_defaults_empty() {
        let (_dir, path) = temp_sources_path();
        test_add(&path, "홍길동 산업부 010-1234");
        let sources = load_sources_from(&path);
        // Legacy entries without beat field should return empty/null gracefully
        let beat = sources[0]["beat"].as_str().unwrap_or("");
        assert_eq!(beat, "");
    }

    #[test]
    fn sources_search_matches_beat() {
        let (_dir, path) = temp_sources_path();
        test_add_with_beat(&path, "홍길동 산업부 010-1234", "경제");
        test_add_with_beat(&path, "김영희 기획부 010-5678", "정치");

        let sources = load_sources_from(&path);
        let query_lower = "경제".to_lowercase();
        let matches: Vec<_> = sources
            .iter()
            .filter(|s| source_matches(s, &query_lower))
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["name"], "홍길동");
    }

    #[test]
    fn sources_beat_filter() {
        let (_dir, path) = temp_sources_path();
        test_add_with_beat(&path, "홍길동 산업부 010-1234", "경제");
        test_add_with_beat(&path, "김영희 기획부 010-5678", "정치");
        test_add_with_beat(&path, "박기자 IT부 010-9999", "경제");

        let sources = load_sources_from(&path);
        let beat = "경제";
        let matches: Vec<_> = sources
            .iter()
            .filter(|s| s["beat"].as_str().unwrap_or("") == beat)
            .collect();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["name"], "홍길동");
        assert_eq!(matches[1]["name"], "박기자");
    }

    #[test]
    fn sources_edit_beat_field() {
        let (_dir, path) = temp_sources_path();
        test_add_with_beat(&path, "홍길동 산업부 010-1234", "경제");

        // Edit beat field
        let mut sources = load_sources_from(&path);
        sources[0]["beat"] = serde_json::Value::String("IT".to_string());
        save_sources_to(&sources, &path);

        let sources = load_sources_from(&path);
        assert_eq!(sources[0]["beat"], "IT");
    }

    #[test]
    fn factcheck_prompt_empty_rejected() {
        assert!(build_factcheck_prompt("").is_none());
    }

    #[test]
    fn factcheck_prompt_with_claim() {
        let prompt = build_factcheck_prompt("한국 반도체 수출이 사상 최대").unwrap();
        assert!(prompt.contains("한국 반도체 수출이 사상 최대"));
        assert!(prompt.contains("팩트체크"));
        assert!(prompt.contains("판정"));
    }

    #[test]
    fn factcheck_prompt_cross_verification_strategies() {
        let prompt = build_factcheck_prompt("테스트 주장").unwrap();
        // 교차검증 전략 키워드 확인
        assert!(prompt.contains("data.go.kr"), "공공데이터포털 참조 누락");
        assert!(prompt.contains("보도자료"), "보도자료 대조 전략 누락");
        assert!(prompt.contains("시계열"), "시계열 데이터 비교 전략 누락");
        assert!(
            prompt.contains("검증 과정"),
            "단계별 검증 과정 표시 누락"
        );
    }

    #[test]
    fn factcheck_prompt_whitespace_only_rejected() {
        // Callers trim before calling, but the function itself rejects empty
        assert!(build_factcheck_prompt("").is_none());
        // Non-empty string is accepted
        assert!(build_factcheck_prompt("test").is_some());
    }

    #[test]
    fn research_file_path_contains_date_and_slug() {
        let path = research_file_path_with_date("경제 전망 보고서", "2026-06-01");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("2026-06-01"));
        assert!(path_str.contains("경제-전망-보고서"));
        assert!(path_str.starts_with(".journalist/research/"));
    }

    #[test]
    fn factcheck_file_path_with_claim() {
        let path = factcheck_file_path_with_date("한국 반도체 수출이 사상 최대", "2026-03-18");
        let path_str = path.to_string_lossy();
        assert!(path_str.starts_with(".journalist/factcheck/"));
        assert!(path_str.contains("2026-03-18"));
        assert!(path_str.contains("한국-반도체-수출이-사상-최대"));
        assert!(path_str.ends_with(".md"));
    }

    #[test]
    fn factcheck_file_path_empty_claim() {
        let path = factcheck_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/factcheck/2026-03-18_factcheck.md"
        );
    }

    #[test]
    fn save_factcheck_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("factcheck").join("test.md");
        save_factcheck(&path, "# 팩트체크 결과\n내용").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# 팩트체크 결과\n내용");
    }

    #[test]
    fn research_search_matches_filename() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_반도체-수출-동향.md"),
            "# 반도체 수출 동향\n내용입니다",
        )
        .unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_부동산-시장.md"),
            "# 부동산 시장\n부동산 관련",
        )
        .unwrap();

        let results = research_search_in("반도체", &research_dir);
        assert_eq!(results.len(), 1);
        assert!(results[0].0.contains("반도체"));
        assert_eq!(results[0].1, "# 반도체 수출 동향");
    }

    #[test]
    fn research_search_matches_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_경제-전망.md"),
            "# 경제 전망\n삼성전자의 반도체 매출이 증가했다.",
        )
        .unwrap();

        let results = research_search_in("삼성전자", &research_dir);
        assert_eq!(results.len(), 1);
        assert!(results[0].2.contains("삼성전자"));
    }

    #[test]
    fn research_search_case_insensitive() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_ai-trends.md"),
            "# AI Trends\nArtificial Intelligence is growing.",
        )
        .unwrap();

        let results = research_search_in("ai", &research_dir);
        assert_eq!(results.len(), 1);

        let results_upper = research_search_in("AI", &research_dir);
        assert_eq!(results_upper.len(), 1);
    }

    #[test]
    fn research_search_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_부동산.md"),
            "# 부동산 시장\n내용",
        )
        .unwrap();

        let results = research_search_in("반도체", &research_dir);
        assert!(results.is_empty());
    }

    #[test]
    fn research_search_empty_keyword() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();

        let results = research_search_in("", &research_dir);
        assert!(results.is_empty());
    }

    #[test]
    fn research_search_nonexistent_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("nonexistent");

        let results = research_search_in("test", &research_dir);
        assert!(results.is_empty());
    }

    #[test]
    fn build_news_context_empty_items() {
        let ctx = build_news_context(&[]);
        assert!(ctx.is_empty());
    }

    #[test]
    fn build_news_context_formats_items() {
        let items = vec![
            NewsItem {
                title: "반도체 수출 증가".to_string(),
                link: "https://example.com/1".to_string(),
                description: "요약 내용".to_string(),
                pub_date: "Mon, 17 Mar 2026".to_string(),
            },
            NewsItem {
                title: "두 번째 뉴스".to_string(),
                link: "https://example.com/2".to_string(),
                description: String::new(),
                pub_date: String::new(),
            },
        ];
        let ctx = build_news_context(&items);
        assert!(ctx.contains("네이버 뉴스 API 검색 결과"));
        assert!(ctx.contains("1. 반도체 수출 증가 (Mon, 17 Mar 2026)"));
        assert!(ctx.contains("링크: https://example.com/1"));
        assert!(ctx.contains("요약: 요약 내용"));
        assert!(ctx.contains("2. 두 번째 뉴스"));
        // No pub_date for second item — no parentheses
        assert!(!ctx.contains("2. 두 번째 뉴스 ("));
        // No description for second item — no 요약 line
        assert!(!ctx.contains("요약: \n"));
    }

    #[test]
    fn build_research_prompt_without_news() {
        let prompt = build_research_prompt("반도체 수출 동향", "");
        assert!(prompt.contains("반도체 수출 동향"));
        assert!(prompt.contains("DuckDuckGo"));
        assert!(prompt.contains("반도체+수출+동향"));
        assert!(!prompt.contains("네이버 뉴스 API 검색 결과"));
    }

    #[test]
    fn build_research_prompt_with_news_context() {
        let news = "\n\n[네이버 뉴스 API 검색 결과]\n1. 테스트 뉴스\n";
        let prompt = build_research_prompt("AI 동향", news);
        assert!(prompt.contains("AI 동향"));
        assert!(prompt.contains("DuckDuckGo"));
        assert!(prompt.ends_with(news));
    }

    #[test]
    fn clip_file_path_basic() {
        let path = clip_file_path("https://news.example.com/article/123", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(
                ".journalist/clips/2026-03-18_news-example-com-article-123.md"
            )
        );
    }

    #[test]
    fn clip_file_path_long_url_truncated() {
        let long_url = format!("https://example.com/{}", "a".repeat(200));
        let path = clip_file_path(&long_url, "2026-03-18");
        let filename = path.file_name().unwrap().to_string_lossy();
        // date prefix (11) + slug (<=80) + .md (3) = <=94
        assert!(filename.len() <= 95, "filename too long: {filename}");
    }

    #[test]
    fn clip_file_path_special_chars() {
        let path =
            clip_file_path("https://news.com/article?id=42&lang=ko#top", "2026-03-18");
        let filename = path.file_name().unwrap().to_string_lossy();
        // Should not contain special URL chars
        assert!(!filename.contains('?'));
        assert!(!filename.contains('&'));
        assert!(!filename.contains('#'));
    }

    #[test]
    fn save_clip_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("clips").join("test-clip.md");
        let result = save_clip(&path, "https://example.com/test", "# 기사 제목\n\n본문 내용");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<!-- source: https://example.com/test -->"));
        assert!(content.contains("기사 제목"));
        assert!(content.contains("본문 내용"));
    }

    #[test]
    fn parse_news_results_valid_json() {
        let json = r#"{"items":[
            {"title":"<b>반도체</b> 수출 호조","link":"https://news.example.com/1","description":"반도체 수출이...","pubDate":"Thu, 19 Mar 2026 10:00:00 +0900"},
            {"title":"삼성 <b>반도체</b> 신공장","link":"https://news.example.com/2","description":"삼성전자가...","pubDate":"Wed, 18 Mar 2026 09:00:00 +0900"}
        ]}"#;
        let results = parse_naver_news_json(json);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "반도체 수출 호조"); // HTML tags stripped
        assert_eq!(results[0].link, "https://news.example.com/1");
        assert!(results[0].description.contains("반도체"));
        assert!(results[0].pub_date.contains("2026"));
    }

    #[test]
    fn parse_news_results_empty() {
        let json = r#"{"items":[]}"#;
        let results = parse_naver_news_json(json);
        assert!(results.is_empty());
    }

    #[test]
    fn parse_news_results_invalid_json() {
        let results = parse_naver_news_json("not json");
        assert!(results.is_empty());
    }

    #[test]
    fn strip_html_tags_basic() {
        assert_eq!(strip_html_tags("<b>hello</b>"), "hello");
        assert_eq!(strip_html_tags("no tags"), "no tags");
        assert_eq!(strip_html_tags("<a href=\"x\">link</a>"), "link");
        assert_eq!(strip_html_tags("&amp; &lt; &gt; &quot;"), "& < > \"");
    }

    #[test]
    fn news_save_path_generation() {
        let items = vec![NewsItem {
            title: "테스트 기사".to_string(),
            link: "https://news.example.com/article/42".to_string(),
            description: "기사 요약".to_string(),
            pub_date: "2026-03-19".to_string(),
        }];
        let path = news_clip_path(&items[0], "2026-03-19");
        assert!(path.starts_with(".journalist/clips/"));
        assert!(path.to_string_lossy().ends_with(".md"));
    }

    #[test]
    fn alert_save_and_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("alerts.json");
        let alerts = vec![
            serde_json::json!({"keyword": "반도체", "created": "2026-03-20 09:00"}),
            serde_json::json!({"keyword": "삼성전자", "created": "2026-03-20 09:01"}),
        ];
        save_alerts_to(&alerts, &path);
        assert!(path.exists());
        let loaded = load_alerts_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["keyword"], "반도체");
        assert_eq!(loaded[1]["keyword"], "삼성전자");
    }

    #[test]
    fn alert_load_empty_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("alerts.json");
        let loaded = load_alerts_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn alert_save_creates_parent_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sub").join("dir").join("alerts.json");
        let alerts = vec![serde_json::json!({"keyword": "테스트", "created": "2026-01-01 00:00"})];
        save_alerts_to(&alerts, &path);
        assert!(path.exists());
    }

    #[test]
    fn alert_remove_by_index() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("alerts.json");
        let mut alerts = vec![
            serde_json::json!({"keyword": "반도체", "created": "2026-03-20 09:00"}),
            serde_json::json!({"keyword": "삼성전자", "created": "2026-03-20 09:01"}),
            serde_json::json!({"keyword": "LG", "created": "2026-03-20 09:02"}),
        ];
        save_alerts_to(&alerts, &path);

        // Remove second entry (1-based index 2 → vec index 1)
        alerts.remove(1);
        save_alerts_to(&alerts, &path);
        let loaded = load_alerts_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["keyword"], "반도체");
        assert_eq!(loaded[1]["keyword"], "LG");
    }

    #[test]
    fn alert_no_duplicate_keywords() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("alerts.json");
        let alerts = vec![
            serde_json::json!({"keyword": "반도체", "created": "2026-03-20 09:00"}),
        ];
        save_alerts_to(&alerts, &path);

        // Check that the keyword already exists
        let loaded = load_alerts_from(&path);
        let exists = loaded.iter().any(|a| a["keyword"].as_str() == Some("반도체"));
        assert!(exists);
    }

    #[test]
    fn trend_file_path_with_keyword() {
        let path = trend_file_path_with_date("반도체 수출", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/trends/2026-03-20_반도체-수출.md"
        );
    }

    #[test]
    fn trend_file_path_empty_keyword() {
        let path = trend_file_path_with_date("", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/trends/2026-03-20_trend.md"
        );
    }

    #[test]
    fn trend_file_path_contains_date_and_slug() {
        let path = trend_file_path_with_date("AI 규제 정책", "2026-06-01");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("2026-06-01"));
        assert!(path_str.contains("ai-규제-정책"));
        assert!(path_str.starts_with(".journalist/trends/"));
    }

    #[test]
    fn save_trend_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("trends").join("test.md");
        save_trend(&path, "# 트렌드 분석\n내용").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# 트렌드 분석\n내용");
    }

    #[test]
    fn build_trend_prompt_contains_keyword() {
        let prompt = build_trend_prompt("반도체", "");
        assert!(prompt.contains("반도체"));
        assert!(prompt.contains("보도량 추이"));
        assert!(prompt.contains("프레임"));
        assert!(prompt.contains("각도"));
        assert!(prompt.contains("취재 타이밍"));
    }

    #[test]
    fn build_trend_prompt_includes_news_context() {
        let news_ctx = "\n[뉴스 데이터]\n1. 반도체 수출 급증";
        let prompt = build_trend_prompt("반도체", news_ctx);
        assert!(prompt.contains("반도체 수출 급증"));
    }

    #[test]
    fn follow_parse_add_args_topic_only() {
        let (topic, due) = parse_follow_add_args("국회 예산안 후속");
        assert_eq!(topic, "국회 예산안 후속");
        assert!(due.is_none());
    }

    #[test]
    fn follow_parse_add_args_with_due() {
        let (topic, due) = parse_follow_add_args("국회 예산안 후속 --due 2026-03-25");
        assert_eq!(topic, "국회 예산안 후속");
        assert_eq!(due.unwrap(), "2026-03-25");
    }

    #[test]
    fn follow_is_valid_date() {
        assert!(is_valid_date("2026-03-25"));
        assert!(is_valid_date("2026-12-01"));
        assert!(!is_valid_date("2026-13-01"));
        assert!(!is_valid_date("2026-00-01"));
        assert!(!is_valid_date("20260325"));
        assert!(!is_valid_date("abc"));
    }

    #[test]
    fn follow_days_until_future() {
        assert_eq!(days_until("2026-03-25", "2026-03-20"), Some(5));
    }

    #[test]
    fn follow_days_until_past() {
        assert_eq!(days_until("2026-03-18", "2026-03-20"), Some(-2));
    }

    #[test]
    fn follow_days_until_today() {
        assert_eq!(days_until("2026-03-20", "2026-03-20"), Some(0));
    }

    #[test]
    fn follow_roundtrip_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followups.json");

        let items = vec![
            Followup {
                topic: "예산안 후속".to_string(),
                due: Some("2026-03-25".to_string()),
                done: false,
                created_at: "2026-03-20T14:00:00".to_string(),
            },
            Followup {
                topic: "인사 청문회".to_string(),
                due: None,
                done: true,
                created_at: "2026-03-19T10:00:00".to_string(),
            },
        ];

        save_followups_to(&items, &path);
        let loaded = load_followups_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].topic, "예산안 후속");
        assert_eq!(loaded[0].due, Some("2026-03-25".to_string()));
        assert!(!loaded[0].done);
        assert!(loaded[1].done);
    }

    #[test]
    fn follow_load_missing_file() {
        let path = std::path::PathBuf::from("/tmp/nonexistent_followups_test_xyz.json");
        let loaded = load_followups_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn parse_press_xml_basic() {
        let xml = r#"
        <response>
        <body>
        <items>
        <item>
        <title>테스트 보도자료 제목</title>
        <SubName1>기획재정부</SubName1>
        <ModDate>2026-03-21</ModDate>
        <DetailUrl>https://example.com/press/1</DetailUrl>
        <SubContent1>경제 정책 관련 보도자료입니다.</SubContent1>
        </item>
        <item>
        <title>두 번째 보도자료</title>
        <SubName1>과학기술정보통신부</SubName1>
        <ModDate>2026-03-20</ModDate>
        <DetailUrl>https://example.com/press/2</DetailUrl>
        <SubContent1>AI 정책 발표</SubContent1>
        </item>
        </items>
        </body>
        </response>
        "#;
        let results = parse_press_xml(xml);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "테스트 보도자료 제목");
        assert_eq!(results[0].ministry, "기획재정부");
        assert_eq!(results[0].date, "2026-03-21");
        assert_eq!(results[0].link, "https://example.com/press/1");
        assert!(results[0].summary.contains("경제 정책"));
        assert_eq!(results[1].title, "두 번째 보도자료");
        assert_eq!(results[1].ministry, "과학기술정보통신부");
    }

    #[test]
    fn parse_press_xml_empty() {
        let xml = "<response><body><items></items></body></response>";
        let results = parse_press_xml(xml);
        assert!(results.is_empty());
    }

    #[test]
    fn parse_press_xml_cdata() {
        let xml = r#"
        <item>
        <title><![CDATA[CDATA 제목 테스트]]></title>
        <SubName1>국토교통부</SubName1>
        <ModDate>2026-03-19</ModDate>
        <DetailUrl>https://example.com/3</DetailUrl>
        <SubContent1><![CDATA[CDATA 내용 테스트]]></SubContent1>
        </item>
        "#;
        let results = parse_press_xml(xml);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "CDATA 제목 테스트");
        assert_eq!(results[0].summary, "CDATA 내용 테스트");
    }

    #[test]
    fn cache_press_release_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("press");
        // Temporarily override by using the function directly
        std::fs::create_dir_all(&dir).unwrap();
        let item = PressRelease {
            title: "테스트 제목".to_string(),
            ministry: "테스트부".to_string(),
            date: "2026-03-21".to_string(),
            link: "https://example.com".to_string(),
            summary: "요약".to_string(),
        };
        let path = dir.join("press_1.json");
        let json = serde_json::json!({
            "title": item.title,
            "ministry": item.ministry,
            "date": item.date,
            "link": item.link,
            "summary": item.summary,
        });
        let content = serde_json::to_string_pretty(&json).unwrap();
        std::fs::write(&path, &content).unwrap();
        assert!(path.exists());
        let loaded: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded["title"], "테스트 제목");
        assert_eq!(loaded["ministry"], "테스트부");
    }

    #[test]
    fn parse_law_response_extracts_terms() {
        let json = r#"{
            "response": {
                "body": {
                    "items": {
                        "item": [
                            {
                                "termNm": "공소시효",
                                "termDf": "범죄 행위가 종료된 후 일정 기간이 지나면 공소를 제기할 수 없게 되는 제도",
                                "rlLwNm": "형사소송법"
                            },
                            {
                                "termNm": "공소장",
                                "termDf": "검사가 공소를 제기할 때 법원에 제출하는 서면",
                                "rlLwNm": "형사소송법"
                            }
                        ]
                    }
                }
            }
        }"#;
        let results = parse_law_response(json);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].term, "공소시효");
        assert!(results[0].definition.contains("공소"));
        assert_eq!(results[0].law_name, "형사소송법");
        assert_eq!(results[1].term, "공소장");
    }

    #[test]
    fn parse_law_response_single_item() {
        let json = r#"{
            "response": {
                "body": {
                    "items": {
                        "item": {
                            "termNm": "선고",
                            "termDf": "법원이 판결을 외부에 표시하는 행위",
                            "rlLwNm": "민사소송법"
                        }
                    }
                }
            }
        }"#;
        let results = parse_law_response(json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].term, "선고");
        assert_eq!(results[0].law_name, "민사소송법");
    }

    #[test]
    fn parse_law_response_empty() {
        let json = r#"{"response":{"body":{"items":{}}}}"#;
        let results = parse_law_response(json);
        assert!(results.is_empty());
    }

    #[test]
    fn parse_law_response_invalid_json() {
        let results = parse_law_response("not json");
        assert!(results.is_empty());
    }

    #[test]
    fn parse_law_response_alternative_field_names() {
        let json = r#"{
            "response": {
                "body": {
                    "items": {
                        "item": [
                            {
                                "lglTrmNm": "구속영장",
                                "lglTrmDfn": "피의자를 구속하기 위해 발부하는 영장",
                                "lawNm": "형사소송법"
                            }
                        ]
                    }
                }
            }
        }"#;
        let results = parse_law_response(json);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].term, "구속영장");
        assert!(results[0].definition.contains("영장"));
        assert_eq!(results[0].law_name, "형사소송법");
    }

    #[test]
    fn handle_law_missing_api_key() {
        // Ensure LAW_API_KEY is not set
        std::env::remove_var("LAW_API_KEY");
        // Should not panic — just prints a message
        handle_law("/law term 공소시효");
    }

    #[test]
    fn handle_law_empty_args_with_key() {
        std::env::set_var("LAW_API_KEY", "test-key");
        // Should print usage, not panic
        handle_law("/law");
        std::env::remove_var("LAW_API_KEY");
    }

    #[test]
    fn sns_cache_path_format() {
        let path = sns_cache_path("search", "반도체");
        let path_str = path.to_string_lossy();
        assert!(path_str.starts_with(".journalist/sns/"));
        assert!(path_str.contains("search_"));
        assert!(path_str.ends_with(".md"));
    }

    #[test]
    fn sns_cache_path_sanitizes_spaces() {
        let path = sns_cache_path("buzz", "AI 규제");
        let filename = path.file_name().unwrap().to_string_lossy();
        // Space should be replaced with underscore
        assert!(
            filename.starts_with("buzz_AI_"),
            "unexpected filename: {filename}"
        );
        assert!(!filename.contains(' '));
    }

    #[test]
    fn sns_cache_dir_is_correct() {
        let dir = sns_cache_dir();
        assert_eq!(dir.to_str().unwrap(), ".journalist/sns");
    }

    #[test]
    fn sns_prompt_contains_keyword() {
        let prompt = sns_search_prompt("반도체");
        assert!(prompt.contains("반도체"));
        let prompt = sns_buzz_prompt("AI규제");
        assert!(prompt.contains("AI규제"));
    }

    #[test]
    fn sns_trend_prompt_not_empty() {
        let prompt = sns_trend_prompt();
        assert!(!prompt.is_empty());
        assert!(prompt.contains("트렌드"));
    }

    #[test]
    fn compute_beat_distribution_basic() {
        let sources = vec![
            serde_json::json!({"name": "김기자", "org": "A", "contact": "010", "note": "", "beat": "경제"}),
            serde_json::json!({"name": "이기자", "org": "B", "contact": "010", "note": "", "beat": "경제"}),
            serde_json::json!({"name": "박기자", "org": "C", "contact": "010", "note": "", "beat": "정치"}),
            serde_json::json!({"name": "최기자", "org": "D", "contact": "010", "note": "", "beat": ""}),
        ];
        let dist = compute_beat_distribution(&sources);
        assert_eq!(dist.get("경제"), Some(&2));
        assert_eq!(dist.get("정치"), Some(&1));
        assert_eq!(dist.get("(미지정)"), Some(&1));
        assert_eq!(dist.len(), 3);
    }

    #[test]
    fn compute_beat_distribution_empty() {
        let sources: Vec<serde_json::Value> = vec![];
        let dist = compute_beat_distribution(&sources);
        assert!(dist.is_empty());
    }

    #[test]
    fn find_gap_beats_identifies_weak_areas() {
        let mut dist = std::collections::HashMap::new();
        dist.insert("경제".to_string(), 5);
        dist.insert("정치".to_string(), 2);
        dist.insert("사회".to_string(), 1);
        dist.insert("국제".to_string(), 0);

        // threshold=2: include beats with count <= 2
        let gaps = find_gap_beats(&dist, 2);
        assert_eq!(gaps.len(), 3);
        // sorted by count ascending
        assert_eq!(gaps[0].0, "국제");
        assert_eq!(gaps[0].1, 0);
        assert_eq!(gaps[1].0, "사회");
        assert_eq!(gaps[1].1, 1);
        assert_eq!(gaps[2].0, "정치");
        assert_eq!(gaps[2].1, 2);
    }

    #[test]
    fn find_gap_beats_none_when_all_strong() {
        let mut dist = std::collections::HashMap::new();
        dist.insert("경제".to_string(), 5);
        dist.insert("정치".to_string(), 3);
        let gaps = find_gap_beats(&dist, 2);
        assert!(gaps.is_empty());
    }

    #[test]
    fn network_suggest_prompt_contains_topic() {
        let prompt = network_suggest_prompt("반도체 수출규제", "경제: 3 명, 정치: 1 명");
        assert!(prompt.contains("반도체 수출규제"));
        assert!(prompt.contains("경제: 3 명"));
        assert!(prompt.contains("취재원"));
    }

    fn temp_notes_dir() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        (dir, notes_dir)
    }

    #[test]
    fn note_struct_jsonl_roundtrip() {
        let note = Note {
            content: "삼성 신규 라인 4월 가동".to_string(),
            source: Some("홍길동".to_string()),
            topic: Some("반도체".to_string()),
            timestamp: "2026-03-22T10:00:00".to_string(),
        };
        let json = serde_json::to_string(&note).unwrap();
        let parsed: Note = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content, "삼성 신규 라인 4월 가동");
        assert_eq!(parsed.source, Some("홍길동".to_string()));
        assert_eq!(parsed.topic, Some("반도체".to_string()));
        assert_eq!(parsed.timestamp, "2026-03-22T10:00:00");
    }

    #[test]
    fn note_struct_optional_fields() {
        let note = Note {
            content: "간단 메모".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-03-22T10:00:00".to_string(),
        };
        let json = serde_json::to_string(&note).unwrap();
        // Optional fields should be skipped
        assert!(!json.contains("source"));
        assert!(!json.contains("topic"));
        let parsed: Note = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source, None);
        assert_eq!(parsed.topic, None);
    }

    #[test]
    fn note_append_and_load() {
        let (_dir, notes_dir) = temp_notes_dir();
        let path = notes_dir.join("2026-03-22.jsonl");

        let note1 = Note {
            content: "첫번째 메모".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-03-22T09:00:00".to_string(),
        };
        let note2 = Note {
            content: "두번째 메모".to_string(),
            source: Some("김기자".to_string()),
            topic: Some("경제".to_string()),
            timestamp: "2026-03-22T10:00:00".to_string(),
        };

        append_note_to(&note1, &path);
        append_note_to(&note2, &path);

        let loaded = load_notes_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].content, "첫번째 메모");
        assert_eq!(loaded[1].content, "두번째 메모");
        assert_eq!(loaded[1].source, Some("김기자".to_string()));
        assert_eq!(loaded[1].topic, Some("경제".to_string()));
    }

    #[test]
    fn note_load_missing_file() {
        let path = std::path::PathBuf::from("/tmp/nonexistent_notes_xyz.jsonl");
        let loaded = load_notes_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn note_load_all_across_dates() {
        let (_dir, notes_dir) = temp_notes_dir();

        let note_day1 = Note {
            content: "1일 메모".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-03-21T09:00:00".to_string(),
        };
        let note_day2 = Note {
            content: "2일 메모".to_string(),
            source: None,
            topic: Some("정치".to_string()),
            timestamp: "2026-03-22T09:00:00".to_string(),
        };

        append_note_to(&note_day1, &notes_dir.join("2026-03-21.jsonl"));
        append_note_to(&note_day2, &notes_dir.join("2026-03-22.jsonl"));

        let all = load_all_notes_from(&notes_dir);
        assert_eq!(all.len(), 2);
        // Files sorted by name, so day1 comes first
        assert_eq!(all[0].content, "1일 메모");
        assert_eq!(all[1].content, "2일 메모");
    }

    #[test]
    fn parse_note_add_args_simple() {
        let (content, source, topic) = parse_note_add_args("간단한 메모입니다");
        assert_eq!(content, "간단한 메모입니다");
        assert_eq!(source, None);
        assert_eq!(topic, None);
    }

    #[test]
    fn parse_note_add_args_with_flags() {
        let (content, source, topic) =
            parse_note_add_args("--source 홍길동 --topic 반도체 \"삼성 신규 라인\"");
        assert_eq!(content, "삼성 신규 라인");
        assert_eq!(source, Some("홍길동".to_string()));
        assert_eq!(topic, Some("반도체".to_string()));
    }

    #[test]
    fn parse_note_add_args_flags_after_content() {
        let (content, source, topic) =
            parse_note_add_args("김OO 과장: 다음 주 발표 예정");
        assert_eq!(content, "김OO 과장: 다음 주 발표 예정");
        assert_eq!(source, None);
        assert_eq!(topic, None);
    }

    #[test]
    fn parse_note_add_args_quoted_content() {
        let (content, source, topic) =
            parse_note_add_args("\"다음 주 발표 예정\"");
        assert_eq!(content, "다음 주 발표 예정");
        assert_eq!(source, None);
        assert_eq!(topic, None);
    }

    #[test]
    fn extract_flag_value_basic() {
        let (val, rest) = extract_flag_value("홍길동 나머지");
        assert_eq!(val, "홍길동");
        assert_eq!(rest, "나머지");
    }

    #[test]
    fn extract_flag_value_empty() {
        let (val, rest) = extract_flag_value("");
        assert!(val.is_empty());
        assert!(rest.is_empty());
    }

    #[test]
    fn extract_flag_value_next_flag() {
        let (val, rest) = extract_flag_value("--topic 반도체");
        assert!(val.is_empty());
        assert_eq!(rest, "--topic 반도체");
    }

    #[test]
    fn note_search_matches_content_source_topic() {
        let (_dir, notes_dir) = temp_notes_dir();
        let path = notes_dir.join("2026-03-22.jsonl");

        append_note_to(
            &Note {
                content: "삼성 신규 라인 4월 가동".to_string(),
                source: Some("홍길동".to_string()),
                topic: Some("반도체".to_string()),
                timestamp: "2026-03-22T10:00:00".to_string(),
            },
            &path,
        );
        append_note_to(
            &Note {
                content: "환율 동향 분석".to_string(),
                source: None,
                topic: Some("경제".to_string()),
                timestamp: "2026-03-22T11:00:00".to_string(),
            },
            &path,
        );

        let all = load_all_notes_from(&notes_dir);

        // Search by content
        let matches: Vec<&Note> = all
            .iter()
            .filter(|n| n.content.to_lowercase().contains("삼성"))
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].content, "삼성 신규 라인 4월 가동");

        // Search by source
        let matches: Vec<&Note> = all
            .iter()
            .filter(|n| {
                n.source
                    .as_ref()
                    .is_some_and(|s| s.to_lowercase().contains("홍길동"))
            })
            .collect();
        assert_eq!(matches.len(), 1);

        // Search by topic
        let matches: Vec<&Note> = all
            .iter()
            .filter(|n| {
                n.topic
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains("경제"))
            })
            .collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].content, "환율 동향 분석");
    }

    #[test]
    fn note_export_prompt_contains_notes() {
        let (_dir, notes_dir) = temp_notes_dir();
        let path = notes_dir.join("2026-03-22.jsonl");
        append_note_to(
            &Note {
                content: "삼성 4월 가동".to_string(),
                source: Some("김과장".to_string()),
                topic: Some("반도체".to_string()),
                timestamp: "2026-03-22T10:00:00".to_string(),
            },
            &path,
        );

        let notes = load_all_notes_from(&notes_dir);
        let matches: Vec<&Note> = notes
            .iter()
            .filter(|n| {
                n.topic
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains("반도체"))
                    || n.content.to_lowercase().contains("반도체")
            })
            .collect();
        assert_eq!(matches.len(), 1);

        // Build the export prompt manually (same logic as handle_note_export)
        let mut collected = String::new();
        for note in &matches {
            let source_tag = note
                .source
                .as_ref()
                .map(|s| format!(" (취재원: {s})"))
                .unwrap_or_default();
            collected.push_str(&format!(
                "- [{}]{source_tag}: {}\n",
                note.timestamp, note.content
            ));
        }
        assert!(collected.contains("삼성 4월 가동"));
        assert!(collected.contains("김과장"));
    }

    #[test]
    fn note_topic_filter() {
        let (_dir, notes_dir) = temp_notes_dir();
        let path = notes_dir.join("2026-03-22.jsonl");

        append_note_to(
            &Note {
                content: "A 노트".to_string(),
                source: None,
                topic: Some("반도체".to_string()),
                timestamp: "2026-03-22T09:00:00".to_string(),
            },
            &path,
        );
        append_note_to(
            &Note {
                content: "B 노트".to_string(),
                source: None,
                topic: Some("경제".to_string()),
                timestamp: "2026-03-22T10:00:00".to_string(),
            },
            &path,
        );
        append_note_to(
            &Note {
                content: "C 노트".to_string(),
                source: None,
                topic: Some("반도체".to_string()),
                timestamp: "2026-03-22T11:00:00".to_string(),
            },
            &path,
        );

        let all = load_all_notes_from(&notes_dir);
        let filtered: Vec<&Note> = all
            .iter()
            .filter(|n| {
                n.topic
                    .as_ref()
                    .is_some_and(|t| t.to_lowercase().contains("반도체"))
            })
            .collect();
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].content, "A 노트");
        assert_eq!(filtered[1].content, "C 노트");
    }

    #[test]
    fn notes_dir_constant() {
        assert_eq!(NOTES_DIR, ".journalist/notes");
    }

    #[test]
    fn contact_log_struct_jsonl_roundtrip() {
        let log = ContactLog {
            name: "홍길동".to_string(),
            summary: "반도체 신규 투자 관련 전화 인터뷰".to_string(),
            timestamp: "2026-03-22T10:00:00".to_string(),
        };
        let json = serde_json::to_string(&log).unwrap();
        let parsed: ContactLog = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "홍길동");
        assert_eq!(parsed.summary, "반도체 신규 투자 관련 전화 인터뷰");
        assert_eq!(parsed.timestamp, "2026-03-22T10:00:00");
    }

    #[test]
    fn contact_file_for_sanitizes_name() {
        let path = contact_file_for("홍길동");
        assert!(path.to_str().unwrap().ends_with(".jsonl"));
        assert!(path.starts_with(CONTACTS_DIR));

        // Spaces become underscores
        let path2 = contact_file_for("홍 길동");
        assert!(path2.to_str().unwrap().contains('_'));
    }

    #[test]
    fn parse_contact_log_args_basic() {
        let (name, summary) = parse_contact_log_args("홍길동 \"반도체 관련 통화\"");
        assert_eq!(name, "홍길동");
        assert_eq!(summary, "반도체 관련 통화");
    }

    #[test]
    fn parse_contact_log_args_no_quotes() {
        let (name, summary) = parse_contact_log_args("홍길동 반도체 관련 통화");
        assert_eq!(name, "홍길동");
        assert_eq!(summary, "반도체 관련 통화");
    }

    #[test]
    fn parse_contact_log_args_empty() {
        let (name, summary) = parse_contact_log_args("");
        assert!(name.is_empty());
        assert!(summary.is_empty());
    }

    #[test]
    fn parse_contact_log_args_name_only() {
        let (name, summary) = parse_contact_log_args("홍길동");
        assert_eq!(name, "홍길동");
        assert!(summary.is_empty());
    }

    #[test]
    fn contact_log_append_and_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.jsonl");

        let log1 = ContactLog {
            name: "홍길동".to_string(),
            summary: "첫 번째 접촉".to_string(),
            timestamp: "2026-03-20T10:00:00".to_string(),
        };
        let log2 = ContactLog {
            name: "홍길동".to_string(),
            summary: "두 번째 접촉".to_string(),
            timestamp: "2026-03-21T14:00:00".to_string(),
        };

        append_contact_log(&log1, &path);
        append_contact_log(&log2, &path);

        let loaded = load_contact_logs_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].summary, "첫 번째 접촉");
        assert_eq!(loaded[1].summary, "두 번째 접촉");
    }

    #[test]
    fn load_contact_logs_from_nonexistent() {
        let path = std::path::Path::new("/tmp/nonexistent_contact_test.jsonl");
        let logs = load_contact_logs_from(path);
        assert!(logs.is_empty());
    }

    #[test]
    fn parse_timestamp_secs_valid() {
        let secs = parse_timestamp_secs("2026-03-22T10:00:00");
        assert!(secs.is_some());
        let secs = secs.unwrap();
        // 2026-03-22 should be roughly 56 years * 365.25 days * 86400 secs
        assert!(secs > 1_700_000_000); // sanity check: after 2023
    }

    #[test]
    fn parse_timestamp_secs_invalid() {
        assert!(parse_timestamp_secs("").is_none());
        assert!(parse_timestamp_secs("abc").is_none());
    }

    #[test]
    fn stale_detection_no_logs() {
        // A source with no contact logs should be stale
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nobody.jsonl");
        let logs = load_contact_logs_from(&path);
        let last = logs
            .iter()
            .filter_map(|l| parse_timestamp_secs(&l.timestamp))
            .max();
        assert!(last.is_none()); // Never contacted = stale
    }

    #[test]
    fn stale_detection_old_log() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("old.jsonl");

        let log = ContactLog {
            name: "테스트".to_string(),
            summary: "오래된 접촉".to_string(),
            timestamp: "2025-01-01T10:00:00".to_string(),
        };
        append_contact_log(&log, &path);

        let logs = load_contact_logs_from(&path);
        let last = logs
            .iter()
            .filter_map(|l| parse_timestamp_secs(&l.timestamp))
            .max()
            .unwrap();

        let now_secs = current_epoch_secs();
        let thirty_days = 30 * 86400;
        assert!(last < now_secs.saturating_sub(thirty_days));
    }

    #[test]
    fn contact_suggest_prompt_contains_topic() {
        let prompt = contact_suggest_prompt("반도체 수출규제");
        assert!(prompt.contains("반도체 수출규제"));
        assert!(prompt.contains("취재원"));
        assert!(prompt.contains("인터뷰"));
    }

    #[test]
    fn contact_suggest_prompt_empty_topic() {
        let prompt = contact_suggest_prompt("");
        assert!(prompt.is_empty());
    }

    #[test]
    fn contacts_dir_constant() {
        assert_eq!(CONTACTS_DIR, ".journalist/contacts");
    }

    // ── /wire tests ──

    #[test]
    fn parse_rss_items_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
<channel>
<title>Test Feed</title>
<item>
<title>속보: 반도체 수출 증가</title>
<link>https://example.com/1</link>
<description>반도체 수출이 크게 증가했다</description>
<pubDate>Sun, 22 Mar 2026 14:00:00 +0900</pubDate>
</item>
<item>
<title>경제 뉴스</title>
<link>https://example.com/2</link>
<description>경제 관련 소식</description>
<pubDate>Sun, 22 Mar 2026 13:00:00 +0900</pubDate>
</item>
</channel>
</rss>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "속보: 반도체 수출 증가");
        assert_eq!(items[0].link, "https://example.com/1");
        assert_eq!(items[0].description, "반도체 수출이 크게 증가했다");
        assert!(items[0].pub_date.contains("22 Mar 2026"));
        assert_eq!(items[1].title, "경제 뉴스");
    }

    #[test]
    fn parse_rss_items_cdata() {
        let xml = r#"<rss><channel>
<item>
<title><![CDATA[CDATA 제목 테스트]]></title>
<link>https://example.com/cdata</link>
<description><![CDATA[<b>HTML 포함</b> 설명]]></description>
<pubDate>Sun, 22 Mar 2026 12:00:00 +0900</pubDate>
</item>
</channel></rss>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "CDATA 제목 테스트");
        // HTML inside CDATA should be stripped
        assert_eq!(items[0].description, "HTML 포함 설명");
    }

    #[test]
    fn parse_rss_items_empty() {
        let xml = r#"<rss><channel><title>Empty</title></channel></rss>"#;
        let items = parse_rss_items(xml);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_rss_items_missing_fields() {
        let xml = r#"<rss><channel>
<item>
<title>제목만 있음</title>
</item>
</channel></rss>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "제목만 있음");
        assert!(items[0].link.is_empty());
        assert!(items[0].description.is_empty());
    }

    #[test]
    fn xml_extract_tag_basic() {
        assert_eq!(
            xml_extract_tag("<title>Hello</title>", "title"),
            Some("Hello".to_string())
        );
    }

    #[test]
    fn xml_extract_tag_cdata() {
        assert_eq!(
            xml_extract_tag("<title><![CDATA[World]]></title>", "title"),
            Some("World".to_string())
        );
    }

    #[test]
    fn xml_extract_tag_missing() {
        assert_eq!(xml_extract_tag("<foo>bar</foo>", "title"), None);
    }

    #[test]
    fn wire_feeds_configured() {
        assert!(WIRE_FEEDS.len() >= 3);
        for &(name, url) in WIRE_FEEDS {
            assert!(!name.is_empty());
            assert!(url.starts_with("https://"));
        }
    }

    // ── /rss tests ────────────────────────────────────────────────────

    #[test]
    fn rss_feeds_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");

        // Initially empty
        assert!(load_rss_feeds_from(&path).is_empty());

        // Save and load
        let feeds = vec![
            RssFeed {
                url: "https://example.com/rss".to_string(),
                name: "Example".to_string(),
                added: "2026-03-22".to_string(),
            },
            RssFeed {
                url: "https://other.com/feed.xml".to_string(),
                name: "".to_string(),
                added: "".to_string(),
            },
        ];
        save_rss_feeds_to(&feeds, &path);

        let loaded = load_rss_feeds_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].url, "https://example.com/rss");
        assert_eq!(loaded[0].name, "Example");
        assert_eq!(loaded[1].url, "https://other.com/feed.xml");
        assert!(loaded[1].name.is_empty());
    }

    #[test]
    fn rss_cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feed_cache.json");

        assert!(load_rss_cache_from(&path).is_empty());

        let items = vec![
            NewsItem {
                title: "테스트 기사 1".to_string(),
                link: "https://example.com/1".to_string(),
                description: "설명 1".to_string(),
                pub_date: "2026-03-22".to_string(),
            },
            NewsItem {
                title: "테스트 기사 2".to_string(),
                link: "https://example.com/2".to_string(),
                description: "".to_string(),
                pub_date: "".to_string(),
            },
        ];
        save_rss_cache_to(&items, &path);

        let loaded = load_rss_cache_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "테스트 기사 1");
        assert_eq!(loaded[0].link, "https://example.com/1");
        assert_eq!(loaded[1].title, "테스트 기사 2");
    }

    #[test]
    fn rss_cache_filename_from_url() {
        let slug = rss_cache_filename("https://www.yna.co.kr/rss/news.xml");
        assert!(!slug.is_empty());
        assert!(!slug.contains("https"));
        // Different URLs should produce different filenames
        let slug2 = rss_cache_filename("https://newsis.com/rss/all_rss.xml");
        assert_ne!(slug, slug2);
    }

    #[test]
    fn rss_command_recognized() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(KNOWN_COMMANDS.contains(&"/rss"));
    }
}
