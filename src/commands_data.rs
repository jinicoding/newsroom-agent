//! Korean public data API command handlers (한국 공공데이터 API 도메인)
//! Commands: /bigkinds, /dart, /assembly, /jsearch

use crate::commands_project::{today_str, topic_to_slug};
use crate::commands_research::{json_extract_string, strip_html_tags};
use crate::format::*;

// ── /jsearch ────────────────────────────────────────────────────────────

/// Search result entry for `/jsearch`.
#[derive(Debug)]
pub struct JSearchResult {
    pub category: &'static str,
    pub file: String,
    pub preview: String,
}

/// Search all `.journalist/` data files for a keyword (case-insensitive).
/// Returns results grouped by category.
pub fn jsearch_in(keyword: &str, base: &std::path::Path) -> Vec<JSearchResult> {
    let kw = keyword.trim().to_lowercase();
    if kw.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::new();

    // Define search targets: (subdirectory or file, category label, extension filter)
    let targets: Vec<(&str, &'static str, &[&str])> = vec![
        ("research", "리서치", &["md"]),
        ("notes", "취재노트", &["jsonl"]),
        ("drafts", "초안", &["md"]),
        ("contacts", "접촉기록", &["jsonl"]),
        ("archive", "아카이브", &["json"]),
        ("sources.json", "취재원", &["json"]),
        ("quotes.json", "인용", &["json"]),
        ("corrections/corrections.jsonl", "정정기록", &["jsonl"]),
    ];

    for (rel_path, category, exts) in &targets {
        let target = base.join(rel_path);
        if !target.exists() {
            continue;
        }
        if target.is_file() {
            search_file(&target, &kw, category, &mut results);
        } else if target.is_dir() {
            search_dir_recursive(&target, &kw, category, exts, &mut results);
        }
    }

    results
}

/// Recursively search a directory for files matching extensions.
fn search_dir_recursive(
    dir: &std::path::Path,
    kw: &str,
    category: &'static str,
    exts: &[&str],
    results: &mut Vec<JSearchResult>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            search_dir_recursive(&path, kw, category, exts, results);
        } else if path.is_file() {
            let ext_match = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e));
            if ext_match {
                search_file(&path, kw, category, results);
            }
        }
    }
}

/// Search a single file for keyword matches (filename + content).
fn search_file(
    path: &std::path::Path,
    kw: &str,
    category: &'static str,
    results: &mut Vec<JSearchResult>,
) {
    let filename = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("")
        .to_string();

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let filename_lower = filename.to_lowercase();
    let content_lower = content.to_lowercase();

    if !filename_lower.contains(kw) && !content_lower.contains(kw) {
        return;
    }

    // Build preview: first matching line, truncated to 80 chars
    let preview = content
        .lines()
        .find(|l| l.to_lowercase().contains(kw))
        .map(|l| {
            let trimmed = l.trim();
            if trimmed.len() > 80 {
                format!("{}…", &trimmed[..trimmed.floor_char_boundary(80)])
            } else {
                trimmed.to_string()
            }
        })
        .unwrap_or_else(|| {
            // Filename matched but no content line matched
            format!("(파일명 매칭: {filename})")
        });

    results.push(JSearchResult {
        category,
        file: filename,
        preview,
    });
}

/// Handle `/jsearch <keyword>` — integrated search across all journalist data.
pub fn handle_jsearch(input: &str) {
    let keyword = input.strip_prefix("/jsearch").unwrap_or("").trim();
    if keyword.is_empty() {
        println!("{BOLD}  /jsearch <키워드>{RESET} — 기자 데이터 통합 검색");
        println!("{DIM}  검색 대상: 리서치, 취재노트, 초안, 취재원, 인용, 아카이브, 접촉기록, 정정기록{RESET}");
        println!("{DIM}  예시: /jsearch 반도체{RESET}\n");
        return;
    }

    let base = std::path::Path::new(".journalist");
    let results = jsearch_in(keyword, base);

    if results.is_empty() {
        println!("{DIM}  \"{keyword}\" 검색 결과가 없습니다.{RESET}\n");
        return;
    }

    // Group by category
    let mut grouped: std::collections::BTreeMap<&str, Vec<&JSearchResult>> =
        std::collections::BTreeMap::new();
    for r in &results {
        grouped.entry(r.category).or_default().push(r);
    }

    println!(
        "{BOLD}  🔍 \"{keyword}\" 통합 검색 결과 ({count}건){RESET}",
        count = results.len()
    );
    println!("{DIM}  ──────────────────────────────{RESET}");

    for (category, items) in &grouped {
        println!("{GREEN}  [{category}]{RESET}");
        for item in items {
            println!("{DIM}    • {file}{RESET}", file = item.file);
            println!("{DIM}      {preview}{RESET}", preview = item.preview);
        }
    }
    println!();
}

// ── /bigkinds — 빅카인즈 뉴스 데이터베이스 검색 ──────────────────────────

/// Directory for cached bigkinds search results.
pub const BIGKINDS_DIR: &str = ".journalist/bigkinds";

/// Subcommand names for `/bigkinds <Tab>` completion.
pub const BIGKINDS_SUBCOMMANDS: &[&str] = &["search", "trend", "related"];

/// A single BIG KINDS search result.
#[derive(Debug, Clone)]
pub struct BigKindsItem {
    pub title: String,
    pub provider: String,
    pub date: String,
    pub url: String,
    pub summary: String,
}

/// A trend data point for BIG KINDS trend analysis.
#[derive(Debug, Clone)]
pub struct BigKindsTrend {
    pub date: String,
    pub count: u64,
}

/// A related keyword from BIG KINDS.
#[derive(Debug, Clone)]
pub struct BigKindsRelated {
    pub keyword: String,
    pub score: f64,
}

/// Parse BIG KINDS search response JSON into items.
pub fn parse_bigkinds_search(json: &str) -> Vec<BigKindsItem> {
    // BIG KINDS API returns: {"result":{"docs":[{"TITLE":"...", "PROVIDER":"...", "DATE":"...", "PROVIDER_LINK_PAGE":"...", "CONTENT":"..."}]}}
    let docs_start = match json.find("\"docs\"") {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let after_docs = &json[docs_start..];
    let arr_start = match after_docs.find('[') {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let arr_content = &after_docs[arr_start..];

    // Find matching bracket
    let arr_end = find_matching_bracket(arr_content);
    let arr_str = &arr_content[..arr_end + 1];

    parse_bigkinds_items_from_array(arr_str)
}

/// Find the position of the closing bracket for an array string starting with '['.
pub(crate) fn find_matching_bracket(s: &str) -> usize {
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    for (i, ch) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
    }
    s.len().saturating_sub(1)
}

/// Parse individual items from a JSON array string of BIG KINDS docs.
fn parse_bigkinds_items_from_array(arr: &str) -> Vec<BigKindsItem> {
    let mut results = Vec::new();
    // Split by object boundaries
    let mut depth = 0;
    let mut obj_start = None;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in arr.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
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
                        let obj = &arr[start..=i];
                        if let Some(item) = parse_single_bigkinds_item(obj) {
                            results.push(item);
                        }
                    }
                    obj_start = None;
                }
            }
            _ => {}
        }
    }
    results
}

/// Parse a single BIG KINDS document JSON object.
pub(crate) fn parse_single_bigkinds_item(obj: &str) -> Option<BigKindsItem> {
    let title = json_extract_string(obj, "TITLE").unwrap_or_default();
    let provider = json_extract_string(obj, "PROVIDER").unwrap_or_default();
    let date = json_extract_string(obj, "DATE").unwrap_or_default();
    let url = json_extract_string(obj, "PROVIDER_LINK_PAGE").unwrap_or_default();
    let summary = json_extract_string(obj, "CONTENT")
        .map(|c| {
            let trimmed = c.chars().take(200).collect::<String>();
            if c.len() > 200 {
                format!("{trimmed}…")
            } else {
                trimmed
            }
        })
        .unwrap_or_default();

    if title.is_empty() {
        return None;
    }
    Some(BigKindsItem {
        title: strip_html_tags(&title),
        provider,
        date,
        url,
        summary: strip_html_tags(&summary),
    })
}

/// Parse BIG KINDS trend response JSON into trend data points.
pub fn parse_bigkinds_trend(json: &str) -> Vec<BigKindsTrend> {
    // Returns: {"result":{"timeline":[{"date":"2026-03-01","count":42},...]}}
    let timeline_start = match json.find("\"timeline\"") {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let after = &json[timeline_start..];
    let arr_start = match after.find('[') {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let arr_content = &after[arr_start..];
    let arr_end = find_matching_bracket(arr_content);
    let arr_str = &arr_content[..arr_end + 1];

    let mut results = Vec::new();
    let mut search_from = 0;
    while let Some(obj_start) = arr_str[search_from..].find('{') {
        let abs_start = search_from + obj_start;
        let obj_end = match arr_str[abs_start..].find('}') {
            Some(pos) => abs_start + pos + 1,
            None => break,
        };
        let obj = &arr_str[abs_start..obj_end];
        let date = json_extract_string(obj, "date").unwrap_or_default();
        let count = json_extract_string(obj, "count")
            .and_then(|c| c.parse::<u64>().ok())
            .unwrap_or(0);
        if !date.is_empty() {
            results.push(BigKindsTrend { date, count });
        }
        search_from = obj_end;
    }
    results
}

/// Parse BIG KINDS related keywords response.
pub fn parse_bigkinds_related(json: &str) -> Vec<BigKindsRelated> {
    // Returns: {"result":{"nodes":[{"name":"키워드","weight":0.85},...]}}
    let nodes_start = match json.find("\"nodes\"") {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let after = &json[nodes_start..];
    let arr_start = match after.find('[') {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let arr_content = &after[arr_start..];
    let arr_end = find_matching_bracket(arr_content);
    let arr_str = &arr_content[..arr_end + 1];

    let mut results = Vec::new();
    let mut search_from = 0;
    while let Some(obj_start) = arr_str[search_from..].find('{') {
        let abs_start = search_from + obj_start;
        let obj_end = match arr_str[abs_start..].find('}') {
            Some(pos) => abs_start + pos + 1,
            None => break,
        };
        let obj = &arr_str[abs_start..obj_end];
        let name = json_extract_string(obj, "name").unwrap_or_default();
        let weight = json_extract_string(obj, "weight")
            .and_then(|w| w.parse::<f64>().ok())
            .unwrap_or(0.0);
        if !name.is_empty() {
            results.push(BigKindsRelated {
                keyword: name,
                score: weight,
            });
        }
        search_from = obj_end;
    }
    results
}

/// Format trend data as a simple bar chart.
pub fn format_trend_chart(trends: &[BigKindsTrend]) -> String {
    if trends.is_empty() {
        return String::from("  데이터 없음");
    }
    let max_count = trends.iter().map(|t| t.count).max().unwrap_or(1).max(1);
    let bar_width = 30;
    let mut out = String::new();
    for t in trends {
        let bar_len = (t.count as f64 / max_count as f64 * bar_width as f64) as usize;
        let bar: String = "█".repeat(bar_len);
        let pad: String = "░".repeat(bar_width - bar_len);
        out.push_str(&format!(
            "  {} {} {}{} ({}건)\n",
            t.date, " ", bar, pad, t.count
        ));
    }
    out
}

/// Save bigkinds search results to cache.
fn save_bigkinds_cache(keyword: &str, subcommand: &str, content: &str) -> Result<(), std::io::Error> {
    let dir = std::path::Path::new(BIGKINDS_DIR);
    std::fs::create_dir_all(dir)?;
    let slug = topic_to_slug(keyword, 30);
    let date = today_str();
    let filename = format!("{date}_{subcommand}_{slug}.json");
    std::fs::write(dir.join(filename), content)
}

/// Build BIG KINDS search API request via curl.
fn bigkinds_search(keyword: &str, count: u32) -> Result<String, String> {
    let api_key = std::env::var("BIGKINDS_API_KEY")
        .map_err(|_| "BIGKINDS_API_KEY 환경변수가 설정되지 않았습니다. https://www.bigkinds.or.kr 에서 API 키를 발급받으세요.".to_string())?;

    let body = format!(
        r#"{{"access_key":"{}","argument":{{"query":"{}","published_at":{{"from":"{}","until":"{}"}},"provider":[],"category":[],"sort":{{"date":"desc"}},"hilight":200,"return_from":0,"return_size":{}}}}}"#,
        api_key,
        keyword.replace('"', "\\\""),
        thirty_days_ago(),
        today_str(),
        count
    );

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "15",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
            "https://tools.kinds.or.kr:8443/search/news",
        ])
        .output()
        .map_err(|e| format!("curl 실행 실패: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "API 요청 실패: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Build BIG KINDS trend API request via curl.
fn bigkinds_trend(keyword: &str) -> Result<String, String> {
    let api_key = std::env::var("BIGKINDS_API_KEY")
        .map_err(|_| "BIGKINDS_API_KEY 환경변수가 설정되지 않았습니다.".to_string())?;

    let body = format!(
        r#"{{"access_key":"{}","argument":{{"query":"{}","published_at":{{"from":"{}","until":"{}"}},"provider":[]}}}}"#,
        api_key,
        keyword.replace('"', "\\\""),
        thirty_days_ago(),
        today_str(),
    );

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "15",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
            "https://tools.kinds.or.kr:8443/search/news",
        ])
        .output()
        .map_err(|e| format!("curl 실행 실패: {e}"))?;

    if !output.status.success() {
        return Err("API 요청 실패".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Build BIG KINDS related keywords API request via curl.
fn bigkinds_related(keyword: &str) -> Result<String, String> {
    let api_key = std::env::var("BIGKINDS_API_KEY")
        .map_err(|_| "BIGKINDS_API_KEY 환경변수가 설정되지 않았습니다.".to_string())?;

    let body = format!(
        r#"{{"access_key":"{}","argument":{{"query":"{}","published_at":{{"from":"{}","until":"{}"}},"provider":[]}}}}"#,
        api_key,
        keyword.replace('"', "\\\""),
        thirty_days_ago(),
        today_str(),
    );

    let output = std::process::Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "15",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            &body,
            "https://tools.kinds.or.kr:8443/search/news",
        ])
        .output()
        .map_err(|e| format!("curl 실행 실패: {e}"))?;

    if !output.status.success() {
        return Err("API 요청 실패".to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get date string for 30 days ago (YYYY-MM-DD).
fn thirty_days_ago() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let thirty_days = 30 * 24 * 60 * 60;
    let past = now - thirty_days;
    // Convert epoch to YYYY-MM-DD
    let days_since_epoch = past / 86400;
    epoch_days_to_date(days_since_epoch)
}

/// Convert days since Unix epoch to YYYY-MM-DD string.
pub(crate) fn epoch_days_to_date(days: u64) -> String {
    // Simple Gregorian calendar conversion
    let mut y = 1970i64;
    let mut remaining = days as i64;

    loop {
        let days_in_year = if is_leap_year(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }

    let month_days = if is_leap_year(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut m = 0;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md as i64 {
            m = i;
            break;
        }
        remaining -= md as i64;
    }

    format!("{:04}-{:02}-{:02}", y, m + 1, remaining + 1)
}

pub(crate) fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Handle the `/bigkinds` command.
pub fn handle_bigkinds(input: &str) {
    let args = input.strip_prefix("/bigkinds").unwrap_or("").trim();

    if args.is_empty() || args == "help" {
        println!("{DIM}  사용법: /bigkinds search <키워드>   빅카인즈 뉴스 검색{RESET}");
        println!("{DIM}          /bigkinds trend <키워드>    키워드 언급량 추이{RESET}");
        println!("{DIM}          /bigkinds related <키워드>  연관어 분석{RESET}");
        println!("{DIM}  환경변수: BIGKINDS_API_KEY (https://www.bigkinds.or.kr 에서 발급){RESET}");
        println!("{DIM}  예시:   /bigkinds search 반도체 수출{RESET}");
        println!("{DIM}          /bigkinds trend AI{RESET}\n");
        return;
    }

    if let Some(keyword) = args.strip_prefix("search") {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            eprintln!("{RED}  검색 키워드를 입력하세요. 예: /bigkinds search 반도체{RESET}\n");
            return;
        }
        handle_bigkinds_search(keyword);
    } else if let Some(keyword) = args.strip_prefix("trend") {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            eprintln!("{RED}  키워드를 입력하세요. 예: /bigkinds trend AI{RESET}\n");
            return;
        }
        handle_bigkinds_trend(keyword);
    } else if let Some(keyword) = args.strip_prefix("related") {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            eprintln!("{RED}  키워드를 입력하세요. 예: /bigkinds related 반도체{RESET}\n");
            return;
        }
        handle_bigkinds_related(keyword);
    } else {
        // Treat bare argument as search
        handle_bigkinds_search(args);
    }
}

fn handle_bigkinds_search(keyword: &str) {
    println!("{DIM}  빅카인즈에서 '{keyword}' 검색 중...{RESET}");
    match bigkinds_search(keyword, 10) {
        Ok(json) => {
            let items = parse_bigkinds_search(&json);
            if items.is_empty() {
                println!("{DIM}  검색 결과가 없습니다.{RESET}\n");
                return;
            }
            println!();
            for (i, item) in items.iter().enumerate() {
                println!(
                    "  {BOLD}{YELLOW}[{}]{RESET} {BOLD}{}{RESET}",
                    i + 1,
                    item.title
                );
                let meta_parts: Vec<&str> = [item.provider.as_str(), item.date.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
                if !meta_parts.is_empty() {
                    println!("  {DIM}    {}{RESET}", meta_parts.join(" | "));
                }
                if !item.summary.is_empty() {
                    println!("  {DIM}    {}{RESET}", item.summary);
                }
                if !item.url.is_empty() {
                    println!("  {DIM}    {}{RESET}", item.url);
                }
                println!();
            }
            // Cache results
            let _ = save_bigkinds_cache(keyword, "search", &json);
            println!(
                "{DIM}  총 {}건 | 최근 30일 | 빅카인즈 뉴스 빅데이터{RESET}\n",
                items.len()
            );
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

fn handle_bigkinds_trend(keyword: &str) {
    println!("{DIM}  빅카인즈에서 '{keyword}' 트렌드 분석 중...{RESET}");
    match bigkinds_trend(keyword) {
        Ok(json) => {
            let trends = parse_bigkinds_trend(&json);
            if trends.is_empty() {
                println!("{DIM}  트렌드 데이터가 없습니다.{RESET}\n");
                return;
            }
            println!("\n  {BOLD}'{keyword}' 언급량 추이 (최근 30일){RESET}\n");
            print!("{}", format_trend_chart(&trends));
            let total: u64 = trends.iter().map(|t| t.count).sum();
            println!(
                "\n{DIM}  총 {total}건 | 기간: {} ~ {}{RESET}\n",
                trends.first().map(|t| t.date.as_str()).unwrap_or("?"),
                trends.last().map(|t| t.date.as_str()).unwrap_or("?"),
            );
            let _ = save_bigkinds_cache(keyword, "trend", &json);
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

fn handle_bigkinds_related(keyword: &str) {
    println!("{DIM}  빅카인즈에서 '{keyword}' 연관어 분석 중...{RESET}");
    match bigkinds_related(keyword) {
        Ok(json) => {
            let related = parse_bigkinds_related(&json);
            if related.is_empty() {
                println!("{DIM}  연관어 데이터가 없습니다.{RESET}\n");
                return;
            }
            println!("\n  {BOLD}'{keyword}' 연관 키워드{RESET}\n");
            for (i, r) in related.iter().enumerate() {
                let bar_len = (r.score * 20.0) as usize;
                let bar: String = "●".repeat(bar_len.min(20));
                println!(
                    "  {DIM}{:>2}.{RESET} {BOLD}{}{RESET} {DIM}{} ({:.0}%){RESET}",
                    i + 1,
                    r.keyword,
                    bar,
                    r.score * 100.0
                );
            }
            println!();
            let _ = save_bigkinds_cache(keyword, "related", &json);
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

// ── /dart — DART 전자공시 검색 ──────────────────────────────────────────

/// Directory for cached DART disclosure results and watch lists.
pub const DART_DIR: &str = ".journalist/dart";

/// Subcommand names for `/dart <Tab>` completion.
pub const DART_SUBCOMMANDS: &[&str] = &["search", "report", "watch"];

/// A single DART disclosure item.
#[derive(Debug, Clone)]
pub struct DartItem {
    pub corp_name: String,
    pub report_nm: String,
    pub rcept_no: String,
    pub rcept_dt: String,
    pub flr_nm: String,
}

/// Parse DART disclosure list JSON into items.
/// DART API returns: {"status":"000","message":"정상","list":[{"corp_name":"...", "report_nm":"...", "rcept_no":"...", "rcept_dt":"...", "flr_nm":"..."}]}
pub fn parse_dart_list(json: &str) -> Vec<DartItem> {
    let list_start = match json.find("\"list\"") {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let after_list = &json[list_start..];
    let arr_start = match after_list.find('[') {
        Some(pos) => pos,
        None => return Vec::new(),
    };
    let arr_content = &after_list[arr_start..];
    let arr_end = find_matching_bracket(arr_content);
    let arr_str = &arr_content[..arr_end + 1];

    parse_dart_items_from_array(arr_str)
}

/// Parse individual items from a JSON array of DART disclosures.
fn parse_dart_items_from_array(arr: &str) -> Vec<DartItem> {
    let mut results = Vec::new();
    let mut depth = 0;
    let mut obj_start = None;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in arr.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
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
                        let obj = &arr[start..=i];
                        if let Some(item) = parse_single_dart_item(obj) {
                            results.push(item);
                        }
                    }
                    obj_start = None;
                }
            }
            _ => {}
        }
    }
    results
}

/// Parse a single DART disclosure JSON object.
pub(crate) fn parse_single_dart_item(obj: &str) -> Option<DartItem> {
    let corp_name = json_extract_string(obj, "corp_name").unwrap_or_default();
    let report_nm = json_extract_string(obj, "report_nm").unwrap_or_default();
    let rcept_no = json_extract_string(obj, "rcept_no").unwrap_or_default();
    let rcept_dt = json_extract_string(obj, "rcept_dt").unwrap_or_default();
    let flr_nm = json_extract_string(obj, "flr_nm").unwrap_or_default();

    if corp_name.is_empty() && report_nm.is_empty() {
        return None;
    }

    Some(DartItem {
        corp_name,
        report_nm,
        rcept_no,
        rcept_dt,
        flr_nm,
    })
}

/// Format DART date from "YYYYMMDD" to "YYYY-MM-DD".
pub fn format_dart_date(raw: &str) -> String {
    if raw.len() == 8 {
        format!("{}-{}-{}", &raw[..4], &raw[4..6], &raw[6..8])
    } else {
        raw.to_string()
    }
}

/// Build DART disclosure list API request via curl.
fn dart_search(corp_name: &str) -> Result<String, String> {
    let api_key = std::env::var("DART_API_KEY")
        .map_err(|_| "DART_API_KEY 환경변수가 설정되지 않았습니다. https://opendart.fss.or.kr 에서 API 키를 발급받으세요.".to_string())?;

    // URL-encode the corporation name
    let encoded_name = url_encode(corp_name);

    // Search recent disclosures (last 30 days)
    let bgn_de = thirty_days_ago().replace('-', "");
    let end_de = today_str().replace('-', "");

    let url = format!(
        "https://opendart.fss.or.kr/api/list.json?crtfc_key={}&corp_name={}&bgn_de={}&end_de={}&page_count=20&sort=date&sort_mth=desc",
        api_key, encoded_name, bgn_de, end_de
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

    // Check DART API error status
    if let Some(status) = json_extract_string(&body, "status") {
        if status != "000" && status != "013" {
            let msg = json_extract_string(&body, "message").unwrap_or_default();
            return Err(format!("DART API 오류 ({}): {}", status, msg));
        }
    }

    Ok(body)
}

/// Build DART single disclosure detail API request via curl.
fn dart_report(rcept_no: &str) -> Result<String, String> {
    let api_key = std::env::var("DART_API_KEY")
        .map_err(|_| "DART_API_KEY 환경변수가 설정되지 않았습니다.".to_string())?;

    let url = format!(
        "https://opendart.fss.or.kr/api/document.xml?crtfc_key={}&rcept_no={}",
        api_key, rcept_no
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
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Simple URL encoding for Korean text.
pub(crate) fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for byte in s.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

/// Save DART cache to file.
fn save_dart_cache(keyword: &str, subcommand: &str, content: &str) -> Result<(), std::io::Error> {
    let dir = std::path::Path::new(DART_DIR);
    std::fs::create_dir_all(dir)?;
    let slug = topic_to_slug(keyword, 30);
    let date = today_str();
    let filename = format!("{date}_{subcommand}_{slug}.json");
    std::fs::write(dir.join(filename), content)
}

/// Load DART watch list.
fn load_dart_watchlist() -> Vec<String> {
    let path = std::path::Path::new(DART_DIR).join("watchlist.json");
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            // Simple JSON array parsing: ["name1","name2"]
            let trimmed = content.trim();
            if !trimmed.starts_with('[') {
                return Vec::new();
            }
            let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
            inner
                .split(',')
                .filter_map(|s| {
                    let s = s.trim().trim_matches('"');
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                })
                .collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Save DART watch list.
fn save_dart_watchlist(list: &[String]) -> Result<(), std::io::Error> {
    let dir = std::path::Path::new(DART_DIR);
    std::fs::create_dir_all(dir)?;
    let entries: Vec<String> = list.iter().map(|s| format!("\"{}\"", s)).collect();
    let json = format!("[{}]", entries.join(","));
    std::fs::write(dir.join("watchlist.json"), json)
}

/// Handle the `/dart` command.
pub fn handle_dart(input: &str) {
    let args = input.strip_prefix("/dart").unwrap_or("").trim();

    if args.is_empty() || args == "help" {
        println!("{DIM}  사용법: /dart search <기업명>       최근 공시 목록 조회{RESET}");
        println!("{DIM}          /dart report <공시번호>     공시 상세 내용 조회{RESET}");
        println!("{DIM}          /dart watch <기업명>        공시 모니터링 등록/해제{RESET}");
        println!("{DIM}  환경변수: DART_API_KEY (https://opendart.fss.or.kr 에서 발급){RESET}");
        println!("{DIM}  예시:   /dart search 삼성전자{RESET}");
        println!("{DIM}          /dart report 20240315000123{RESET}");
        println!("{DIM}          /dart watch LG에너지솔루션{RESET}\n");
        return;
    }

    if let Some(corp) = args.strip_prefix("search") {
        let corp = corp.trim();
        if corp.is_empty() {
            eprintln!("{RED}  기업명을 입력하세요. 예: /dart search 삼성전자{RESET}\n");
            return;
        }
        handle_dart_search(corp);
    } else if let Some(rcept_no) = args.strip_prefix("report") {
        let rcept_no = rcept_no.trim();
        if rcept_no.is_empty() {
            eprintln!("{RED}  공시번호를 입력하세요. 예: /dart report 20240315000123{RESET}\n");
            return;
        }
        handle_dart_report(rcept_no);
    } else if let Some(corp) = args.strip_prefix("watch") {
        let corp = corp.trim();
        if corp.is_empty() {
            // Show current watchlist
            let list = load_dart_watchlist();
            if list.is_empty() {
                println!("{DIM}  등록된 모니터링 기업이 없습니다.{RESET}");
                println!("{DIM}  사용법: /dart watch <기업명>{RESET}\n");
            } else {
                println!("\n  {BOLD}DART 공시 모니터링 목록{RESET}\n");
                for (i, name) in list.iter().enumerate() {
                    println!("  {DIM}{:>2}.{RESET} {BOLD}{}{RESET}", i + 1, name);
                }
                println!();
            }
            return;
        }
        handle_dart_watch(corp);
    } else {
        // Treat bare argument as search
        handle_dart_search(args);
    }
}

fn handle_dart_search(corp_name: &str) {
    println!("{DIM}  DART에서 '{corp_name}' 공시 검색 중...{RESET}");
    match dart_search(corp_name) {
        Ok(json) => {
            let items = parse_dart_list(&json);
            if items.is_empty() {
                println!("{DIM}  검색 결과가 없습니다.{RESET}\n");
                return;
            }
            println!();
            for (i, item) in items.iter().enumerate() {
                println!(
                    "  {BOLD}{YELLOW}[{}]{RESET} {BOLD}{}{RESET}",
                    i + 1,
                    item.report_nm
                );
                let date_fmt = format_dart_date(&item.rcept_dt);
                let meta_parts: Vec<&str> = [item.corp_name.as_str(), date_fmt.as_str(), item.flr_nm.as_str()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
                if !meta_parts.is_empty() {
                    println!("  {DIM}    {}{RESET}", meta_parts.join(" | "));
                }
                println!("  {DIM}    공시번호: {}{RESET}", item.rcept_no);
                println!();
            }
            let _ = save_dart_cache(corp_name, "search", &json);
            println!(
                "{DIM}  총 {}건 | 최근 30일 | DART 전자공시{RESET}\n",
                items.len()
            );
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

fn handle_dart_report(rcept_no: &str) {
    // Validate: rcept_no should be numeric
    if !rcept_no.chars().all(|c| c.is_ascii_digit()) {
        eprintln!("{RED}  공시번호는 숫자만 입력하세요. 예: 20240315000123{RESET}\n");
        return;
    }
    println!("{DIM}  DART 공시 '{rcept_no}' 상세 조회 중...{RESET}");
    match dart_report(rcept_no) {
        Ok(content) => {
            // The document.xml endpoint returns XML/HTML content
            // Extract text content, stripping HTML tags for readability
            let text = strip_html_tags(&content);
            let trimmed: String = text.chars().take(3000).collect();
            if trimmed.is_empty() {
                println!("{DIM}  공시 내용을 가져올 수 없습니다.{RESET}\n");
                return;
            }
            println!("\n  {BOLD}공시 상세 내용 (공시번호: {rcept_no}){RESET}\n");
            // Print with wrapping
            for line in trimmed.lines().take(80) {
                let line = line.trim();
                if !line.is_empty() {
                    println!("  {DIM}{}{RESET}", line);
                }
            }
            if content.len() > 3000 {
                println!("\n  {DIM}... (전체 내용 중 일부만 표시){RESET}");
            }
            println!();
            let _ = save_dart_cache(rcept_no, "report", &content);
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

fn handle_dart_watch(corp_name: &str) {
    let mut list = load_dart_watchlist();

    if let Some(pos) = list.iter().position(|s| s == corp_name) {
        list.remove(pos);
        match save_dart_watchlist(&list) {
            Ok(()) => {
                println!(
                    "  {GREEN}'{corp_name}' 모니터링 해제 완료{RESET} (현재 {}개 기업 등록 중)\n",
                    list.len()
                );
            }
            Err(e) => {
                eprintln!("{RED}  저장 실패: {e}{RESET}\n");
            }
        }
    } else {
        list.push(corp_name.to_string());
        match save_dart_watchlist(&list) {
            Ok(()) => {
                println!(
                    "  {GREEN}'{corp_name}' 모니터링 등록 완료{RESET} (현재 {}개 기업 등록 중)\n",
                    list.len()
                );
                println!("{DIM}  저장 위치: {DART_DIR}/watchlist.json{RESET}\n");
            }
            Err(e) => {
                eprintln!("{RED}  저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /assembly — 국회 입법정보 검색 ──────────────────────────────────────

/// Subcommand names for `/assembly <Tab>` completion.
pub const ASSEMBLY_SUBCOMMANDS: &[&str] = &["search", "recent", "bill"];

/// A single National Assembly bill item.
#[derive(Debug, Clone)]
pub struct AssemblyBill {
    pub bill_id: String,
    pub bill_no: String,
    pub bill_name: String,
    pub proposer: String,
    pub propose_dt: String,
    pub committee: String,
    pub proc_result: String,
}

/// Parse National Assembly bill list XML into items.
/// LIKMS API returns XML with <row> elements containing bill fields.
pub fn parse_assembly_list(xml: &str) -> Vec<AssemblyBill> {
    let mut results = Vec::new();
    let mut search_from = 0;

    loop {
        let row_start = match xml[search_from..].find("<row>") {
            Some(pos) => search_from + pos,
            None => break,
        };
        let row_end = match xml[row_start..].find("</row>") {
            Some(pos) => row_start + pos + 6,
            None => break,
        };
        let row = &xml[row_start..row_end];

        let bill = AssemblyBill {
            bill_id: xml_extract(row, "BILL_ID"),
            bill_no: xml_extract(row, "BILL_NO"),
            bill_name: xml_extract(row, "BILL_NAME"),
            proposer: xml_extract(row, "PROPOSER"),
            propose_dt: xml_extract(row, "PROPOSE_DT"),
            committee: xml_extract(row, "COMMITTEE"),
            proc_result: xml_extract(row, "PROC_RESULT"),
        };

        if !bill.bill_name.is_empty() {
            results.push(bill);
        }

        search_from = row_end;
    }

    results
}

/// Extract text content from an XML tag. Returns empty string if not found.
pub(crate) fn xml_extract(xml: &str, tag: &str) -> String {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = match xml.find(&open) {
        Some(pos) => pos + open.len(),
        None => return String::new(),
    };
    let end = match xml[start..].find(&close) {
        Some(pos) => start + pos,
        None => return String::new(),
    };
    xml[start..end].trim().to_string()
}

/// Format assembly date from "YYYY-MM-DD" or "YYYYMMDD" to display format.
pub fn format_assembly_date(raw: &str) -> String {
    if raw.len() == 8 && raw.chars().all(|c| c.is_ascii_digit()) {
        format!("{}-{}-{}", &raw[..4], &raw[4..6], &raw[6..8])
    } else {
        raw.to_string()
    }
}

/// Build National Assembly bill search API request via curl.
fn assembly_search(keyword: &str) -> Result<String, String> {
    let api_key = std::env::var("ASSEMBLY_API_KEY")
        .map_err(|_| "ASSEMBLY_API_KEY 환경변수를 설정하세요 (https://open.assembly.go.kr 에서 발급)".to_string())?;
    let encoded = url_encode(keyword);
    let url = format!(
        "https://open.assembly.go.kr/portal/openapi/nzmimeepazxkubdpn?KEY={}&Type=xml&pSize=20&BILL_NAME={}",
        api_key, encoded
    );
    run_curl(&url)
}

/// Fetch recent bills from National Assembly API.
fn assembly_recent() -> Result<String, String> {
    let api_key = std::env::var("ASSEMBLY_API_KEY")
        .map_err(|_| "ASSEMBLY_API_KEY 환경변수를 설정하세요 (https://open.assembly.go.kr 에서 발급)".to_string())?;
    let url = format!(
        "https://open.assembly.go.kr/portal/openapi/nzmimeepazxkubdpn?KEY={}&Type=xml&pSize=20&AGE=22",
        api_key
    );
    run_curl(&url)
}

/// Fetch bill detail from National Assembly API.
fn assembly_bill_detail(bill_no: &str) -> Result<String, String> {
    let api_key = std::env::var("ASSEMBLY_API_KEY")
        .map_err(|_| "ASSEMBLY_API_KEY 환경변수를 설정하세요 (https://open.assembly.go.kr 에서 발급)".to_string())?;
    let encoded = url_encode(bill_no);
    let url = format!(
        "https://open.assembly.go.kr/portal/openapi/nzmimeepazxkubdpn?KEY={}&Type=xml&pSize=5&BILL_NO={}",
        api_key, encoded
    );
    run_curl(&url)
}

/// Run curl and return the response body.
fn run_curl(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args(["-s", "--max-time", "15", url])
        .output()
        .map_err(|e| format!("curl 실행 실패: {e}"))?;
    if !output.status.success() {
        return Err(format!("HTTP 요청 실패 (status: {:?})", output.status.code()));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("응답 디코딩 실패: {e}"))
}

/// Handle the `/assembly` command.
pub fn handle_assembly(input: &str) {
    let args = input.strip_prefix("/assembly").unwrap_or("").trim();

    if args.is_empty() || args == "help" {
        println!("{DIM}  사용법: /assembly search <키워드>    법안명 검색{RESET}");
        println!("{DIM}          /assembly recent             최근 발의 법안{RESET}");
        println!("{DIM}          /assembly bill <의안번호>     법안 상세 조회{RESET}");
        println!("{DIM}  환경변수: ASSEMBLY_API_KEY (https://open.assembly.go.kr 에서 발급){RESET}");
        println!("{DIM}  예시:   /assembly search 반도체{RESET}");
        println!("{DIM}          /assembly bill 2200001{RESET}\n");
        return;
    }

    if let Some(keyword) = args.strip_prefix("search") {
        let keyword = keyword.trim();
        if keyword.is_empty() {
            eprintln!("{RED}  검색 키워드를 입력하세요. 예: /assembly search 반도체{RESET}\n");
            return;
        }
        handle_assembly_search(keyword);
    } else if args == "recent" || args.starts_with("recent ") {
        handle_assembly_recent();
    } else if let Some(bill_no) = args.strip_prefix("bill") {
        let bill_no = bill_no.trim();
        if bill_no.is_empty() {
            eprintln!("{RED}  의안번호를 입력하세요. 예: /assembly bill 2200001{RESET}\n");
            return;
        }
        handle_assembly_bill(bill_no);
    } else {
        // Treat bare argument as search
        handle_assembly_search(args);
    }
}

fn print_assembly_bills(bills: &[AssemblyBill]) {
    for (i, bill) in bills.iter().enumerate() {
        println!(
            "  {BOLD}{YELLOW}[{}]{RESET} {BOLD}{}{RESET}",
            i + 1,
            bill.bill_name
        );
        let date_fmt = format_assembly_date(&bill.propose_dt);
        let mut meta_parts: Vec<&str> = Vec::new();
        if !bill.proposer.is_empty() {
            meta_parts.push(&bill.proposer);
        }
        if !bill.propose_dt.is_empty() {
            meta_parts.push(&date_fmt);
        }
        if !bill.committee.is_empty() {
            meta_parts.push(&bill.committee);
        }
        if !meta_parts.is_empty() {
            println!("  {DIM}    {}{RESET}", meta_parts.join(" | "));
        }
        if !bill.proc_result.is_empty() {
            println!("  {DIM}    처리상태: {}{RESET}", bill.proc_result);
        }
        if !bill.bill_no.is_empty() {
            println!("  {DIM}    의안번호: {}{RESET}", bill.bill_no);
        }
        println!();
    }
}

fn handle_assembly_search(keyword: &str) {
    println!("{DIM}  국회의안정보에서 '{keyword}' 검색 중...{RESET}");
    match assembly_search(keyword) {
        Ok(xml) => {
            let bills = parse_assembly_list(&xml);
            if bills.is_empty() {
                println!("{DIM}  검색 결과가 없습니다.{RESET}\n");
                return;
            }
            println!();
            print_assembly_bills(&bills);
            println!(
                "{DIM}  총 {}건 | 국회의안정보시스템{RESET}\n",
                bills.len()
            );
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

fn handle_assembly_recent() {
    println!("{DIM}  최근 발의 법안 조회 중...{RESET}");
    match assembly_recent() {
        Ok(xml) => {
            let bills = parse_assembly_list(&xml);
            if bills.is_empty() {
                println!("{DIM}  조회 결과가 없습니다.{RESET}\n");
                return;
            }
            println!();
            print_assembly_bills(&bills);
            println!(
                "{DIM}  총 {}건 | 제22대 국회 | 국회의안정보시스템{RESET}\n",
                bills.len()
            );
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

fn handle_assembly_bill(bill_no: &str) {
    println!("{DIM}  의안번호 '{bill_no}' 상세 조회 중...{RESET}");
    match assembly_bill_detail(bill_no) {
        Ok(xml) => {
            let bills = parse_assembly_list(&xml);
            if bills.is_empty() {
                println!("{DIM}  해당 의안을 찾을 수 없습니다.{RESET}\n");
                return;
            }
            let bill = &bills[0];
            println!("\n  {BOLD}법안 상세 정보 (의안번호: {}){RESET}\n", bill.bill_no);
            println!("  {BOLD}법안명:{RESET}   {}", bill.bill_name);
            if !bill.proposer.is_empty() {
                println!("  {BOLD}발의자:{RESET}   {}", bill.proposer);
            }
            if !bill.propose_dt.is_empty() {
                println!(
                    "  {BOLD}발의일:{RESET}   {}",
                    format_assembly_date(&bill.propose_dt)
                );
            }
            if !bill.committee.is_empty() {
                println!("  {BOLD}소관위:{RESET}   {}", bill.committee);
            }
            if !bill.proc_result.is_empty() {
                println!("  {BOLD}처리상태:{RESET} {}", bill.proc_result);
            }
            if !bill.bill_id.is_empty() {
                println!(
                    "  {BOLD}상세URL:{RESET}  https://likms.assembly.go.kr/bill/billDetail.do?billId={}",
                    bill.bill_id
                );
            }
            println!();
        }
        Err(e) => {
            eprintln!("{RED}  {e}{RESET}\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── jsearch_in ──────────────────────────────────────────────────────

    #[test]
    fn test_jsearch_keyword_match() {
        let dir = tempfile::tempdir().unwrap();
        let research = dir.path().join("research");
        fs::create_dir_all(&research).unwrap();
        fs::write(research.join("topic.md"), "반도체 산업 동향 분석").unwrap();
        let results = jsearch_in("반도체", dir.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, "리서치");
        assert!(results[0].preview.contains("반도체"));
    }

    #[test]
    fn test_jsearch_empty_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let results = jsearch_in("", dir.path());
        assert!(results.is_empty());
    }

    #[test]
    fn test_jsearch_whitespace_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let results = jsearch_in("   ", dir.path());
        assert!(results.is_empty());
    }

    #[test]
    fn test_jsearch_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let research = dir.path().join("research");
        fs::create_dir_all(&research).unwrap();
        fs::write(research.join("topic.md"), "AI 기술 동향").unwrap();
        let results = jsearch_in("반도체", dir.path());
        assert!(results.is_empty());
    }

    #[test]
    fn test_jsearch_category_classification() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("meeting.jsonl"), r#"{"note":"삼성전자 취재"}"#).unwrap();
        let drafts = dir.path().join("drafts");
        fs::create_dir_all(&drafts).unwrap();
        fs::write(drafts.join("article.md"), "삼성전자 실적 발표").unwrap();

        let results = jsearch_in("삼성전자", dir.path());
        assert_eq!(results.len(), 2);
        let categories: Vec<&str> = results.iter().map(|r| r.category).collect();
        assert!(categories.contains(&"취재노트"));
        assert!(categories.contains(&"초안"));
    }

    #[test]
    fn test_jsearch_filename_match() {
        let dir = tempfile::tempdir().unwrap();
        let research = dir.path().join("research");
        fs::create_dir_all(&research).unwrap();
        fs::write(research.join("반도체.md"), "내용 없음").unwrap();
        let results = jsearch_in("반도체", dir.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].preview.contains("파일명 매칭"));
    }

    // ── parse_bigkinds_search ───────────────────────────────────────────

    #[test]
    fn test_parse_bigkinds_search_normal() {
        let json = r#"{"result":{"docs":[{"TITLE":"반도체 수출 증가","PROVIDER":"조선일보","DATE":"2026-03-29","PROVIDER_LINK_PAGE":"https://example.com","CONTENT":"반도체 수출이 크게 증가했다"}]}}"#;
        let items = parse_bigkinds_search(json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "반도체 수출 증가");
        assert_eq!(items[0].provider, "조선일보");
        assert_eq!(items[0].date, "2026-03-29");
    }

    #[test]
    fn test_parse_bigkinds_search_empty() {
        let json = r#"{"result":{"docs":[]}}"#;
        let items = parse_bigkinds_search(json);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_bigkinds_search_invalid_json() {
        let items = parse_bigkinds_search("not json at all");
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_bigkinds_search_no_title() {
        let json = r#"{"result":{"docs":[{"TITLE":"","PROVIDER":"A","DATE":"2026-01-01","PROVIDER_LINK_PAGE":"","CONTENT":""}]}}"#;
        let items = parse_bigkinds_search(json);
        assert!(items.is_empty());
    }

    // ── parse_single_bigkinds_item ──────────────────────────────────────

    #[test]
    fn test_parse_single_bigkinds_item_valid() {
        let obj = r#"{"TITLE":"테스트 기사","PROVIDER":"한겨레","DATE":"2026-03-28","PROVIDER_LINK_PAGE":"https://ex.com","CONTENT":"짧은 내용"}"#;
        let item = parse_single_bigkinds_item(obj).unwrap();
        assert_eq!(item.title, "테스트 기사");
        assert_eq!(item.provider, "한겨레");
    }

    #[test]
    fn test_parse_single_bigkinds_item_missing_title() {
        let obj = r#"{"PROVIDER":"A","DATE":"2026-01-01"}"#;
        assert!(parse_single_bigkinds_item(obj).is_none());
    }

    // ── parse_bigkinds_trend ────────────────────────────────────────────

    #[test]
    fn test_parse_bigkinds_trend_normal() {
        let json = r#"{"result":{"timeline":[{"date":"2026-03-01","count":"42"},{"date":"2026-03-02","count":"55"}]}}"#;
        let trends = parse_bigkinds_trend(json);
        assert_eq!(trends.len(), 2);
        assert_eq!(trends[0].date, "2026-03-01");
        assert_eq!(trends[0].count, 42);
        assert_eq!(trends[1].count, 55);
    }

    #[test]
    fn test_parse_bigkinds_trend_empty() {
        let json = r#"{"result":{"timeline":[]}}"#;
        let trends = parse_bigkinds_trend(json);
        assert!(trends.is_empty());
    }

    #[test]
    fn test_parse_bigkinds_trend_no_timeline() {
        let trends = parse_bigkinds_trend(r#"{"result":{}}"#);
        assert!(trends.is_empty());
    }

    // ── parse_bigkinds_related ──────────────────────────────────────────

    #[test]
    fn test_parse_bigkinds_related_normal() {
        let json = r#"{"result":{"nodes":[{"name":"AI","weight":"0.85"},{"name":"GPU","weight":"0.72"}]}}"#;
        let related = parse_bigkinds_related(json);
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].keyword, "AI");
        assert!((related[0].score - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_parse_bigkinds_related_empty() {
        let json = r#"{"result":{"nodes":[]}}"#;
        let related = parse_bigkinds_related(json);
        assert!(related.is_empty());
    }

    // ── format_trend_chart ──────────────────────────────────────────────

    #[test]
    fn test_format_trend_chart_normal() {
        let trends = vec![
            BigKindsTrend { date: "2026-03-01".into(), count: 10 },
            BigKindsTrend { date: "2026-03-02".into(), count: 20 },
        ];
        let chart = format_trend_chart(&trends);
        assert!(chart.contains("2026-03-01"));
        assert!(chart.contains("2026-03-02"));
        assert!(chart.contains("10건"));
        assert!(chart.contains("20건"));
        assert!(chart.contains('█'));
    }

    #[test]
    fn test_format_trend_chart_empty() {
        let chart = format_trend_chart(&[]);
        assert!(chart.contains("데이터 없음"));
    }

    // ── parse_dart_list ─────────────────────────────────────────────────

    #[test]
    fn test_parse_dart_list_normal() {
        let json = r#"{"status":"000","message":"정상","list":[{"corp_name":"삼성전자","report_nm":"분기보고서","rcept_no":"20260315000123","rcept_dt":"20260315","flr_nm":"삼성전자"}]}"#;
        let items = parse_dart_list(json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].corp_name, "삼성전자");
        assert_eq!(items[0].report_nm, "분기보고서");
        assert_eq!(items[0].rcept_no, "20260315000123");
    }

    #[test]
    fn test_parse_dart_list_empty() {
        let json = r#"{"status":"013","message":"조회된 데이터가 없습니다.","list":[]}"#;
        let items = parse_dart_list(json);
        assert!(items.is_empty());
    }

    #[test]
    fn test_parse_dart_list_no_list_key() {
        let items = parse_dart_list(r#"{"status":"000"}"#);
        assert!(items.is_empty());
    }

    // ── parse_single_dart_item ──────────────────────────────────────────

    #[test]
    fn test_parse_single_dart_item_valid() {
        let obj = r#"{"corp_name":"LG화학","report_nm":"사업보고서","rcept_no":"123","rcept_dt":"20260101","flr_nm":"LG화학"}"#;
        let item = parse_single_dart_item(obj).unwrap();
        assert_eq!(item.corp_name, "LG화학");
        assert_eq!(item.report_nm, "사업보고서");
    }

    #[test]
    fn test_parse_single_dart_item_empty() {
        let obj = r#"{"corp_name":"","report_nm":""}"#;
        assert!(parse_single_dart_item(obj).is_none());
    }

    // ── format_dart_date ────────────────────────────────────────────────

    #[test]
    fn test_format_dart_date_valid() {
        assert_eq!(format_dart_date("20260329"), "2026-03-29");
        assert_eq!(format_dart_date("20251231"), "2025-12-31");
    }

    #[test]
    fn test_format_dart_date_passthrough() {
        assert_eq!(format_dart_date("2026-03-29"), "2026-03-29");
        assert_eq!(format_dart_date("short"), "short");
        assert_eq!(format_dart_date(""), "");
    }

    // ── parse_assembly_list ─────────────────────────────────────────────

    #[test]
    fn test_parse_assembly_list_normal() {
        let xml = r#"<response><body><items><row><BILL_ID>B001</BILL_ID><BILL_NO>2200001</BILL_NO><BILL_NAME>반도체산업 지원법</BILL_NAME><PROPOSER>홍길동</PROPOSER><PROPOSE_DT>20260301</PROPOSE_DT><COMMITTEE>산업통상자원위원회</COMMITTEE><PROC_RESULT>계류</PROC_RESULT></row></items></body></response>"#;
        let bills = parse_assembly_list(xml);
        assert_eq!(bills.len(), 1);
        assert_eq!(bills[0].bill_name, "반도체산업 지원법");
        assert_eq!(bills[0].proposer, "홍길동");
        assert_eq!(bills[0].bill_no, "2200001");
    }

    #[test]
    fn test_parse_assembly_list_multiple() {
        let xml = "<items><row><BILL_ID>1</BILL_ID><BILL_NO>100</BILL_NO><BILL_NAME>법안A</BILL_NAME><PROPOSER></PROPOSER><PROPOSE_DT></PROPOSE_DT><COMMITTEE></COMMITTEE><PROC_RESULT></PROC_RESULT></row><row><BILL_ID>2</BILL_ID><BILL_NO>200</BILL_NO><BILL_NAME>법안B</BILL_NAME><PROPOSER></PROPOSER><PROPOSE_DT></PROPOSE_DT><COMMITTEE></COMMITTEE><PROC_RESULT></PROC_RESULT></row></items>";
        let bills = parse_assembly_list(xml);
        assert_eq!(bills.len(), 2);
        assert_eq!(bills[0].bill_name, "법안A");
        assert_eq!(bills[1].bill_name, "법안B");
    }

    #[test]
    fn test_parse_assembly_list_empty() {
        let bills = parse_assembly_list("<response></response>");
        assert!(bills.is_empty());
    }

    #[test]
    fn test_parse_assembly_list_empty_name() {
        let xml = "<row><BILL_ID>1</BILL_ID><BILL_NO>100</BILL_NO><BILL_NAME></BILL_NAME><PROPOSER></PROPOSER><PROPOSE_DT></PROPOSE_DT><COMMITTEE></COMMITTEE><PROC_RESULT></PROC_RESULT></row>";
        let bills = parse_assembly_list(xml);
        assert!(bills.is_empty());
    }

    // ── format_assembly_date ────────────────────────────────────────────

    #[test]
    fn test_format_assembly_date_yyyymmdd() {
        assert_eq!(format_assembly_date("20260329"), "2026-03-29");
    }

    #[test]
    fn test_format_assembly_date_passthrough() {
        assert_eq!(format_assembly_date("2026-03-29"), "2026-03-29");
        assert_eq!(format_assembly_date(""), "");
    }

    // ── xml_extract ─────────────────────────────────────────────────────

    #[test]
    fn test_xml_extract_found() {
        assert_eq!(xml_extract("<TAG>value</TAG>", "TAG"), "value");
    }

    #[test]
    fn test_xml_extract_not_found() {
        assert_eq!(xml_extract("<OTHER>value</OTHER>", "TAG"), "");
    }

    #[test]
    fn test_xml_extract_nested() {
        let xml = "<row><A>hello</A><B>world</B></row>";
        assert_eq!(xml_extract(xml, "A"), "hello");
        assert_eq!(xml_extract(xml, "B"), "world");
    }

    #[test]
    fn test_xml_extract_whitespace() {
        assert_eq!(xml_extract("<TAG>  value  </TAG>", "TAG"), "value");
    }

    // ── url_encode ──────────────────────────────────────────────────────

    #[test]
    fn test_url_encode_ascii() {
        assert_eq!(url_encode("hello"), "hello");
        assert_eq!(url_encode("test123"), "test123");
    }

    #[test]
    fn test_url_encode_korean() {
        let encoded = url_encode("삼성전자");
        assert!(encoded.starts_with('%'));
        assert!(!encoded.contains("삼성"));
    }

    #[test]
    fn test_url_encode_special() {
        assert_eq!(url_encode("a b"), "a%20b");
        assert_eq!(url_encode("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn test_url_encode_unreserved() {
        assert_eq!(url_encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    // ── find_matching_bracket ───────────────────────────────────────────

    #[test]
    fn test_find_matching_bracket_simple() {
        assert_eq!(find_matching_bracket("[]"), 1);
        assert_eq!(find_matching_bracket("[1,2,3]"), 6);
    }

    #[test]
    fn test_find_matching_bracket_nested() {
        assert_eq!(find_matching_bracket("[[1],[2]]"), 8);
    }

    #[test]
    fn test_find_matching_bracket_with_strings() {
        let s = r#"["a]b","c"]"#;
        assert_eq!(find_matching_bracket(s), 10);
    }

    #[test]
    fn test_find_matching_bracket_escaped_quote() {
        let s = r#"["a\"b"]"#;
        assert_eq!(find_matching_bracket(s), 7);
    }

    // ── epoch_days_to_date / is_leap_year ───────────────────────────────

    #[test]
    fn test_epoch_days_to_date_epoch() {
        assert_eq!(epoch_days_to_date(0), "1970-01-01");
    }

    #[test]
    fn test_epoch_days_to_date_known() {
        // 2026-03-29 = days since epoch
        // 2026-01-01 is day 20454 (from 1970-01-01)
        // Jan=31, Feb=28, so Mar 29 = 31+28+29-1 = 87 days into 2026
        // Total = 20454 + 87 = 20541
        assert_eq!(epoch_days_to_date(20541), "2026-03-29");
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2023));
        assert!(is_leap_year(2400));
    }

    #[test]
    fn test_epoch_days_to_date_leap_day() {
        // 2024-02-29: leap year
        // 2024-01-01 is day 19723
        // Jan=31, Feb=29 → Feb 29 = day 31+28 = 59th day (0-indexed: 58)
        // 19723 + 59 = 19782
        assert_eq!(epoch_days_to_date(19782), "2024-02-29");
    }
}
