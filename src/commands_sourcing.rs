//! Sourcing & monitoring command handlers (취재원·정보흐름 관리 도메인)
//! Commands: /alert, /contact, /follow, /monitor, /note, /rss, /tip, /verify, /wire
//!
//! Split from commands_research.rs on Day 14.

use crate::commands::auto_compact_if_needed;
use crate::commands_project::*;
use crate::commands_research::{
    ensure_sources_dir_at, load_sources, news_clip_path, save_clip, strip_html_tags, NewsItem,
};
use crate::commands_workflow::today_date_string;
use crate::commands_writing::format_unix_timestamp;
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

// ── Constants ────────────────────────────────────────────────────────

/// Subcommand names for `/wire <Tab>` completion.
pub const WIRE_SUBCOMMANDS: &[&str] = &["save"];

/// Subcommand names for `/rss <Tab>` completion.
pub const RSS_SUBCOMMANDS: &[&str] = &["add", "list", "check", "search", "remove"];

/// Subcommand names for `/contact <Tab>` completion.
pub const CONTACT_SUBCOMMANDS: &[&str] = &["log", "history", "recent", "stale", "suggest"];

/// Subcommand names for `/tip <Tab>` completion.
pub const TIP_SUBCOMMANDS: &[&str] = &["add", "list", "show", "update", "search"];

// ── /wire — 통신사 속보 모니터링 ──────────────────────────────────────

/// RSS feed URLs for major Korean wire services.
pub(crate) const WIRE_FEEDS: &[(&str, &str)] = &[
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
pub(crate) fn xml_extract_tag(xml: &str, tag: &str) -> Option<String> {
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
pub(crate) struct RssFeed {
    pub(crate) url: String,
    pub(crate) name: String,
    pub(crate) added: String,
}

/// Load subscribed RSS feeds from the feeds file.
pub(crate) fn load_rss_feeds() -> Vec<RssFeed> {
    load_rss_feeds_from(std::path::Path::new(RSS_FEEDS_FILE))
}

pub(crate) fn load_rss_feeds_from(path: &std::path::Path) -> Vec<RssFeed> {
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
pub(crate) fn save_rss_feeds(feeds: &[RssFeed]) {
    save_rss_feeds_to(feeds, std::path::Path::new(RSS_FEEDS_FILE));
}

pub(crate) fn save_rss_feeds_to(feeds: &[RssFeed], path: &std::path::Path) {
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
pub(crate) fn rss_cache_filename(url: &str) -> String {
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
pub(crate) fn load_rss_cache(url: &str) -> Vec<NewsItem> {
    let filename = format!("{}.json", rss_cache_filename(url));
    let path = std::path::Path::new(RSS_CACHE_DIR).join(filename);
    load_rss_cache_from(&path)
}

pub(crate) fn load_rss_cache_from(path: &std::path::Path) -> Vec<NewsItem> {
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
pub(crate) fn save_rss_cache(url: &str, items: &[NewsItem]) {
    let filename = format!("{}.json", rss_cache_filename(url));
    let path = std::path::Path::new(RSS_CACHE_DIR).join(filename);
    save_rss_cache_to(items, &path);
}

pub(crate) fn save_rss_cache_to(items: &[NewsItem], path: &std::path::Path) {
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

pub(crate) fn load_alerts_from(path: &std::path::Path) -> Vec<serde_json::Value> {
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

pub(crate) fn save_alerts_to(alerts: &[serde_json::Value], path: &std::path::Path) {
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
pub(crate) fn extract_naver_news_headlines(html: &str, max: usize) -> Vec<String> {
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
pub(crate) fn parse_follow_add_args(args: &str) -> (String, Option<String>) {
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
pub(crate) fn is_valid_date(s: &str) -> bool {
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


// ── /note ────────────────────────────────────────────────────────────────

pub(crate) const NOTES_DIR: &str = ".journalist/notes";

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
pub(crate) fn append_note_to(note: &Note, path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let line = serde_json::to_string(note).unwrap_or_default();
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "⚠ 노트 파일을 열 수 없습니다: {} ({}). \
                 '.journalist/notes/' 디렉토리 권한을 확인하세요.",
                path.display(),
                e
            );
            return;
        }
    };
    let _ = writeln!(file, "{line}");
}

/// Load all notes across all date files, sorted by timestamp ascending.
pub(crate) fn load_all_notes() -> Vec<Note> {
    load_all_notes_from(&notes_dir())
}

pub(crate) fn load_all_notes_from(dir: &std::path::Path) -> Vec<Note> {
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
pub(crate) fn parse_note_add_args(args: &str) -> (String, Option<String>, Option<String>) {
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
pub(crate) fn extract_flag_value(s: &str) -> (String, String) {
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

pub(crate) const CONTACTS_DIR: &str = ".journalist/contacts";

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
pub(crate) fn contact_file_for(name: &str) -> std::path::PathBuf {
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
pub(crate) fn parse_timestamp_secs(ts: &str) -> Option<u64> {
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

pub(crate) fn current_epoch_secs() -> u64 {
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
pub(crate) fn parse_contact_log_args(args: &str) -> (String, String) {
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
pub(crate) fn contact_suggest_prompt(topic: &str) -> String {
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

// ── /monitor — 키워드 지속 모니터링과 변화 감지 ──────────────────────────

/// Directory for monitor data.
const MONITOR_DIR: &str = ".journalist/monitor";

/// Subcommand names for `/monitor <Tab>` completion.
pub const MONITOR_SUBCOMMANDS: &[&str] = &["add", "list", "check", "history", "remove"];

/// Load the monitor keywords list from a given path.
pub(crate) fn load_monitor_keywords_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Load the monitor keywords from the default path.
pub(crate) fn load_monitor_keywords() -> Vec<serde_json::Value> {
    load_monitor_keywords_from(std::path::Path::new(MONITOR_DIR).join("keywords.json").as_path())
}

/// Save monitor keywords to a given path.
pub(crate) fn save_monitor_keywords_to(keywords: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(keywords) {
        let _ = std::fs::write(path, json);
    }
}

/// Save monitor keywords to the default path.
pub(crate) fn save_monitor_keywords(keywords: &[serde_json::Value]) {
    let path = std::path::Path::new(MONITOR_DIR).join("keywords.json");
    save_monitor_keywords_to(keywords, &path);
}

/// Load history entries for a keyword from a given directory.
pub fn load_monitor_history_from(
    keyword: &str,
    monitor_dir: &std::path::Path,
) -> Vec<serde_json::Value> {
    let slug = topic_to_slug(keyword, 50);
    let path = monitor_dir.join(format!("{slug}_history.json"));
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Load history entries for a keyword from the default monitor directory.
pub(crate) fn load_monitor_history(keyword: &str) -> Vec<serde_json::Value> {
    load_monitor_history_from(keyword, std::path::Path::new(MONITOR_DIR))
}

/// Save history entries for a keyword to a given directory.
pub fn save_monitor_history_to(
    keyword: &str,
    history: &[serde_json::Value],
    monitor_dir: &std::path::Path,
) {
    let slug = topic_to_slug(keyword, 50);
    let path = monitor_dir.join(format!("{slug}_history.json"));
    ensure_sources_dir_at(&path);
    if let Ok(json) = serde_json::to_string_pretty(history) {
        let _ = std::fs::write(path, json);
    }
}

/// Save history entries for a keyword to the default monitor directory.
pub(crate) fn save_monitor_history(keyword: &str, history: &[serde_json::Value]) {
    save_monitor_history_to(keyword, history, std::path::Path::new(MONITOR_DIR));
}

/// Detect new headlines that don't appear in the previous check's headlines.
pub fn detect_new_headlines(
    current: &[String],
    previous: &[String],
) -> Vec<String> {
    current
        .iter()
        .filter(|h| !previous.iter().any(|p| p == *h))
        .cloned()
        .collect()
}

/// Handle `/monitor` command dispatch.
pub fn handle_monitor(input: &str) {
    let args = input.strip_prefix("/monitor").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /monitor add <키워드>{RESET}");
                println!("{DIM}  예시: /monitor add 반도체 수출규제{RESET}\n");
            } else {
                monitor_add(rest);
            }
        }
        "list" => {
            monitor_list();
        }
        "check" => {
            monitor_check();
        }
        "history" => {
            let rest = args.strip_prefix("history").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /monitor history <키워드>{RESET}");
                println!("{DIM}  예시: /monitor history 반도체{RESET}\n");
            } else {
                monitor_history_display(rest);
            }
        }
        "remove" => {
            let rest = args.strip_prefix("remove").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /monitor remove <번호>{RESET}");
                println!("{DIM}  예시: /monitor remove 2{RESET}\n");
            } else {
                monitor_remove(rest);
            }
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /monitor [add|list|check|history|remove]{RESET}\n");
        }
    }
}

fn monitor_add(keyword: &str) {
    let keyword = keyword.trim();
    let mut keywords = load_monitor_keywords();

    if keywords
        .iter()
        .any(|k| k["keyword"].as_str() == Some(keyword))
    {
        println!("{DIM}  '{keyword}' 키워드는 이미 모니터링 중입니다.{RESET}\n");
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
    keywords.push(entry);
    save_monitor_keywords(&keywords);

    println!("{DIM}  모니터링 등록됨: \"{keyword}\" [{timestamp}]{RESET}\n");
}

fn monitor_list() {
    let keywords = load_monitor_keywords();
    if keywords.is_empty() {
        println!("{DIM}  등록된 모니터링 키워드가 없습니다.");
        println!("  /monitor add <키워드> 로 추가하세요.{RESET}\n");
        return;
    }

    println!("{BOLD}  모니터링 키워드 ({} 건){RESET}", keywords.len());
    println!("{DIM}  ─────────────────────────────{RESET}");
    for (i, kw) in keywords.iter().enumerate() {
        let keyword = kw["keyword"].as_str().unwrap_or("?");
        let created = kw["created"].as_str().unwrap_or("");
        let history = load_monitor_history(keyword);
        let check_count = history.len();
        let last_check = history
            .last()
            .and_then(|h| h["checked_at"].as_str())
            .unwrap_or("없음");
        println!(
            "{DIM}  {}. {keyword}  (등록: {created}, 확인: {check_count}회, 최근: {last_check}){RESET}",
            i + 1
        );
    }
    println!();
}

fn monitor_check() {
    let keywords = load_monitor_keywords();
    if keywords.is_empty() {
        println!("{DIM}  등록된 모니터링 키워드가 없습니다.");
        println!("  /monitor add <키워드> 로 추가하세요.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  모니터링 변화 감지 — {} 개 키워드 확인 중...{RESET}\n",
        keywords.len()
    );

    for kw in &keywords {
        let keyword = kw["keyword"].as_str().unwrap_or("?");
        println!("{BOLD}  ▶ \"{keyword}\"{RESET}");

        // Fetch current headlines
        let current_headlines = fetch_naver_headlines(keyword, 10);

        match current_headlines {
            Ok(headlines) => {
                // Load previous history
                let history = load_monitor_history(keyword);
                let previous_headlines: Vec<String> = history
                    .last()
                    .and_then(|h| h["headlines"].as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                let new_headlines = detect_new_headlines(&headlines, &previous_headlines);

                if headlines.is_empty() {
                    println!("{DIM}    검색 결과 없음{RESET}");
                } else if new_headlines.is_empty() {
                    println!("{DIM}    변화 없음 (이전과 동일한 {count}건){RESET}", count = headlines.len());
                } else {
                    println!(
                        "{GREEN}    🆕 새 기사 {new}건 발견 (전체 {total}건){RESET}",
                        new = new_headlines.len(),
                        total = headlines.len()
                    );
                    for (i, h) in new_headlines.iter().enumerate() {
                        println!("{GREEN}    {idx}. {h}{RESET}", idx = i + 1);
                    }
                }

                // Save this check to history
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let timestamp = format_unix_timestamp(secs);

                let entry = serde_json::json!({
                    "checked_at": timestamp,
                    "headline_count": headlines.len(),
                    "new_count": new_headlines.len(),
                    "headlines": headlines,
                });

                let mut history = load_monitor_history(keyword);
                history.push(entry);
                // Keep last 50 checks max
                if history.len() > 50 {
                    history = history.split_off(history.len() - 50);
                }
                save_monitor_history(keyword, &history);
            }
            Err(e) => {
                eprintln!("{RED}    뉴스 조회 실패: {e}{RESET}");
            }
        }
        println!();
    }
}

/// Fetch headlines from Naver news search for a keyword.
pub fn fetch_naver_headlines(keyword: &str, max: usize) -> Result<Vec<String>, String> {
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

    let output = std::process::Command::new("curl")
        .args(["-sL", "--max-time", "10", &url])
        .output()
        .map_err(|e| format!("{e}"))?;

    let body = String::from_utf8_lossy(&output.stdout);
    Ok(extract_naver_news_headlines(&body, max))
}

fn monitor_history_display(keyword: &str) {
    let keyword = keyword.trim();
    let history = load_monitor_history(keyword);

    if history.is_empty() {
        println!("{DIM}  \"{keyword}\"의 모니터링 히스토리가 없습니다.{RESET}");
        println!("{DIM}  /monitor check 으로 먼저 확인하세요.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  \"{keyword}\" 모니터링 히스토리 ({} 회 확인){RESET}",
        history.len()
    );
    println!("{DIM}  ─────────────────────────────{RESET}");

    // Show last 10 checks
    let start = if history.len() > 10 {
        history.len() - 10
    } else {
        0
    };
    for (i, entry) in history[start..].iter().enumerate() {
        let checked_at = entry["checked_at"].as_str().unwrap_or("?");
        let total = entry["headline_count"].as_u64().unwrap_or(0);
        let new_count = entry["new_count"].as_u64().unwrap_or(0);
        let marker = if new_count > 0 {
            format!("{GREEN}🆕 +{new_count}{RESET}")
        } else {
            format!("{DIM}변화없음{RESET}")
        };
        println!(
            "{DIM}  {idx}. [{checked_at}] {total}건 — {marker}",
            idx = start + i + 1,
        );
    }
    println!();
}

fn monitor_remove(idx_str: &str) {
    let idx: usize = match idx_str.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  유효한 번호를 입력하세요: {idx_str}{RESET}\n");
            return;
        }
    };
    let mut keywords = load_monitor_keywords();
    if idx > keywords.len() {
        eprintln!(
            "{RED}  범위 밖의 번호: {idx} (전체 {} 건){RESET}\n",
            keywords.len()
        );
        return;
    }
    let removed = keywords.remove(idx - 1);
    save_monitor_keywords(&keywords);
    let keyword = removed["keyword"].as_str().unwrap_or("?");
    println!("{DIM}  모니터링 삭제됨: \"{keyword}\"{RESET}\n");
}

// (bigkinds/dart/assembly code moved to commands_data.rs)

// ── /verify ──────────────────────────────────────────────────────────────

/// Directory for cross-verification reports.
pub const VERIFY_DIR: &str = ".journalist/verify";

/// Build the verify file path: `.journalist/verify/YYYY-MM-DD_<slug>.md`
pub fn verify_file_path(claim: &str) -> std::path::PathBuf {
    verify_file_path_with_date(claim, &today_str())
}

/// Build the verify file path with an explicit date string (for testing).
pub fn verify_file_path_with_date(claim: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(claim, 50);
    let filename = if slug.is_empty() {
        format!("{date}_verify.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(VERIFY_DIR).join(filename)
}

/// Save verification report to file. Creates the verify directory if needed.
pub(crate) fn save_verify(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// List existing verification reports in `.journalist/verify/`.
fn verify_list() {
    let dir = std::path::Path::new(VERIFY_DIR);
    if !dir.exists() {
        println!("{DIM}  저장된 교차검증 보고서가 없습니다.{RESET}\n");
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect(),
        Err(_) => {
            println!("{DIM}  교차검증 디렉토리를 읽을 수 없습니다.{RESET}\n");
            return;
        }
    };
    if entries.is_empty() {
        println!("{DIM}  저장된 교차검증 보고서가 없습니다.{RESET}\n");
        return;
    }
    entries.sort_by_key(|e| e.file_name());
    println!("{BOLD}  📋 교차검증 보고서 목록:{RESET}");
    for entry in &entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        println!("    {name}");
    }
    println!();
}

/// Build the verification prompt for a given claim.
///
/// Unlike `/factcheck` which relies on AI judgment, `/verify` instructs the agent
/// to query concrete data sources (news APIs, DART, BIG Kinds, public data portals)
/// and compile a structured cross-verification report with source citations.
pub fn build_verify_prompt(claim: &str) -> Option<String> {
    if claim.is_empty() {
        return None;
    }
    let ctx = profile_context();
    Some(format!(
        "다음 주장을 실제 데이터 소스를 활용해 교차검증해주세요: \"{claim}\"{ctx}\n\n\
         **반드시 다음 데이터 소스를 순서대로 조회하세요:**\n\n\
         1. **뉴스 검색**: 네이버 뉴스 API 또는 DuckDuckGo로 관련 보도 검색\n\
         2. **BIG Kinds (빅카인즈)**: 한국언론진흥재단 뉴스 빅데이터 — 해당 주장과 관련된 기사 건수·추이 확인\n\
         3. **DART (전자공시)**: 관련 기업이 있다면 금융감독원 전자공시 시스템에서 공시 자료 확인\n\
         4. **공공데이터**: data.go.kr 등 정부·공공 통계에서 수치 확인\n\
         5. **기타**: 학술자료, 국회 의안정보, 해외 통신사 등 추가 소스\n\n\
         **보고서 형식:**\n\n\
         ## 교차검증 보고서\n\n\
         ### 검증 대상\n\
         > {claim}\n\n\
         ### 소스별 검증 결과\n\
         각 소스에서 발견한 내용을 구체적으로 기술 (URL, 날짜, 수치 포함)\n\n\
         ### 소스 간 일치/불일치\n\
         소스들이 일치하는 부분과 불일치하는 부분을 명확히 표시\n\n\
         ### 종합 판정\n\
         - ✅ 확인됨 / ⚠️ 부분 확인 / ❌ 반박됨 / ❓ 검증 불가\n\
         - 근거 요약\n\
         - 추가 검증이 필요한 부분\n\n\
         **중요**: 조회하지 못한 소스도 \"조회 실패\" 또는 \"해당 없음\"으로 명시하세요. \
         기자가 어떤 소스를 확인했고 어떤 소스를 확인하지 못했는지 아는 것이 중요합니다."
    ))
}

/// Handle the `/verify` command.
pub async fn handle_verify(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let claim = input.strip_prefix("/verify").unwrap_or("").trim();

    if claim == "list" {
        verify_list();
        return;
    }

    let prompt = match build_verify_prompt(claim) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /verify <주장 또는 사실>{RESET}");
            println!("{DIM}  예시: /verify 삼성전자가 2025년 반도체 매출 100조원을 달성했다{RESET}");
            println!("{DIM}  /verify list — 저장된 교차검증 보고서 목록{RESET}\n");
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save verification report to file
    if !response.trim().is_empty() {
        let path = verify_file_path(claim);
        match save_verify(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 교차검증 보고서 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  교차검증 보고서 저장 실패: {e}{RESET}\n");
            }
        }
    }
}


// ── /tip ─────────────────────────────────────────────────────────────────

/// Directory for tip entries.
pub const TIPS_DIR: &str = ".journalist/tips";

/// Tip status values.
pub const TIP_STATUSES: &[&str] = &["미확인", "취재중", "기사화", "보류", "폐기"];

/// A single tip (제보) entry stored as JSON.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct TipEntry {
    pub id: String,
    pub source: String,
    pub content: String,
    pub anonymous: bool,
    pub credibility: u8,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_story: Option<String>,
}

pub(crate) fn tips_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(TIPS_DIR)
}

/// Generate a short unique ID for a tip (YYYYMMDD-HHMM-XXXX).
pub(crate) fn generate_tip_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let date = format_unix_timestamp(now);
    let date_part = date.replace(['-', ':', ' '], "");
    let rand_part = now % 10000;
    format!("{}-{:04}", &date_part[..12], rand_part)
}

/// Return the file path for a tip: `.journalist/tips/<id>.json`
#[cfg(test)]
pub fn tip_file_path(id: &str) -> std::path::PathBuf {
    tips_dir().join(format!("{id}.json"))
}

/// Return the file path for a tip in a given directory (for testing).
#[cfg(test)]
pub fn tip_file_path_at(dir: &std::path::Path, id: &str) -> std::path::PathBuf {
    dir.join(format!("{id}.json"))
}

/// Save a tip entry to its JSON file.
pub fn save_tip(tip: &TipEntry, base_dir: &std::path::Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(base_dir)?;
    let path = base_dir.join(format!("{}.json", tip.id));
    let json = serde_json::to_string_pretty(tip).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, e)
    })?;
    std::fs::write(path, json)
}

/// Load a tip entry from a JSON file.
pub fn load_tip(path: &std::path::Path) -> Option<TipEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Load all tips from a directory, sorted by created_at descending (newest first).
pub fn load_all_tips(dir: &std::path::Path) -> Vec<TipEntry> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut tips: Vec<TipEntry> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| load_tip(&e.path()))
        .collect();
    tips.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    tips
}

/// Handle `/tip` command with subcommands: add, list, show, update, search.
pub fn handle_tip(input: &str) {
    let args = input.strip_prefix("/tip").unwrap_or("").trim();

    if args.is_empty() || args == "help" || args == "--help" {
        print_tip_usage();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    let dir = tips_dir();
    match sub {
        "add" => handle_tip_add(rest, &dir),
        "list" => handle_tip_list(&dir),
        "show" => handle_tip_show(rest, &dir),
        "update" => handle_tip_update(rest, &dir),
        "search" => handle_tip_search(rest, &dir),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_tip_usage();
        }
    }
}

fn print_tip_usage() {
    println!("{DIM}  사용법:");
    println!("    /tip add <내용> --source 출처 [--anon] [--cred 1-5] [--story 슬러그]   제보 등록");
    println!("    /tip list                                        전체 제보 목록");
    println!("    /tip show <ID>                                   제보 상세 보기");
    println!("    /tip update <ID> <상태>                          상태 변경 (미확인/취재중/기사화/보류/폐기)");
    println!("    /tip search <키워드>                             키워드 검색{RESET}\n");
}

/// Parse `/tip add` arguments.
pub(crate) fn parse_tip_add_args(args: &str) -> (String, String, bool, u8, Option<String>) {
    let mut source = String::new();
    let mut anonymous = false;
    let mut credibility: u8 = 3;
    let mut story: Option<String> = None;
    let mut remaining = args.to_string();

    // Extract --source
    if let Some(pos) = remaining.find("--source") {
        let before = remaining[..pos].to_string();
        let after = remaining[pos + 8..].trim_start().to_string();
        let (val, rest) = extract_flag_value(&after);
        source = val;
        remaining = format!("{before} {rest}").trim().to_string();
    }

    // Extract --anon flag
    if let Some(pos) = remaining.find("--anon") {
        let before = remaining[..pos].to_string();
        let after = remaining[pos + 6..].trim_start().to_string();
        anonymous = true;
        remaining = format!("{before} {after}").trim().to_string();
    }

    // Extract --cred
    if let Some(pos) = remaining.find("--cred") {
        let before = remaining[..pos].to_string();
        let after = remaining[pos + 6..].trim_start().to_string();
        let (val, rest) = extract_flag_value(&after);
        if let Ok(c) = val.parse::<u8>() {
            credibility = c.clamp(1, 5);
        }
        remaining = format!("{before} {rest}").trim().to_string();
    }

    // Extract --story
    if let Some(pos) = remaining.find("--story") {
        let before = remaining[..pos].to_string();
        let after = remaining[pos + 7..].trim_start().to_string();
        let (val, rest) = extract_flag_value(&after);
        story = if val.is_empty() { None } else { Some(val) };
        remaining = format!("{before} {rest}").trim().to_string();
    }

    let content = remaining
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();

    (content, source, anonymous, credibility, story)
}

fn handle_tip_add(args: &str, dir: &std::path::Path) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /tip add <내용> --source 출처 [--anon] [--cred 1-5]{RESET}\n");
        return;
    }

    let (content, source, anonymous, credibility, linked_story) = parse_tip_add_args(args);

    if content.is_empty() {
        eprintln!("{RED}  제보 내용을 입력하세요.{RESET}\n");
        return;
    }
    if source.is_empty() {
        eprintln!("{RED}  출처를 지정하세요 (--source 이름).{RESET}\n");
        return;
    }

    let now = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format_unix_timestamp(secs).replace(' ', "T") + ":00"
    };

    let tip = TipEntry {
        id: generate_tip_id(),
        source: source.clone(),
        content: content.clone(),
        anonymous,
        credibility,
        status: "미확인".to_string(),
        created_at: now.clone(),
        updated_at: now,
        linked_story,
    };

    match save_tip(&tip, dir) {
        Ok(()) => {
            let anon_label = if anonymous { " [익명]" } else { "" };
            println!(
                "{GREEN}  📋 제보 등록: {id} — {src}{anon} (신뢰도 {cred}/5){RESET}\n",
                id = tip.id,
                src = source,
                anon = anon_label,
                cred = credibility,
            );
        }
        Err(e) => eprintln!("{RED}  제보 저장 실패: {e}{RESET}\n"),
    }
}

fn handle_tip_list(dir: &std::path::Path) {
    let tips = load_all_tips(dir);
    if tips.is_empty() {
        println!("{DIM}  등록된 제보가 없습니다.{RESET}\n");
        return;
    }
    println!("{DIM}  제보 목록 ({count}건):{RESET}", count = tips.len());
    for tip in &tips {
        let anon = if tip.anonymous { " [익명]" } else { "" };
        let src_display = if tip.anonymous {
            "익명".to_string()
        } else {
            tip.source.clone()
        };
        let preview: String = tip.content.chars().take(40).collect();
        let ellipsis = if tip.content.chars().count() > 40 { "…" } else { "" };
        println!(
            "{DIM}  [{status}] {id}  {src}{anon}  {preview}{ellipsis}  (신뢰도 {cred}/5){RESET}",
            status = tip.status,
            id = tip.id,
            src = src_display,
            anon = anon,
            preview = preview,
            ellipsis = ellipsis,
            cred = tip.credibility,
        );
    }
    println!();
}

fn handle_tip_show(id: &str, dir: &std::path::Path) {
    if id.is_empty() {
        eprintln!("{RED}  사용법: /tip show <ID>{RESET}\n");
        return;
    }

    let path = dir.join(format!("{id}.json"));
    match load_tip(&path) {
        Some(tip) => {
            let anon_label = if tip.anonymous { " (익명 제보)" } else { "" };
            let src_display = if tip.anonymous {
                "익명".to_string()
            } else {
                tip.source.clone()
            };
            println!("{DIM}  ── 제보 상세 ──{RESET}");
            println!("{DIM}  ID:       {}{RESET}", tip.id);
            println!("{DIM}  출처:     {src_display}{anon_label}{RESET}");
            println!("{DIM}  신뢰도:   {}/5{RESET}", tip.credibility);
            println!("{DIM}  상태:     {}{RESET}", tip.status);
            println!("{DIM}  등록일:   {}{RESET}", tip.created_at);
            println!("{DIM}  수정일:   {}{RESET}", tip.updated_at);
            if let Some(ref story) = tip.linked_story {
                println!("{DIM}  연결 스토리: {story}{RESET}");
            }
            println!("{DIM}  ─────────────{RESET}");
            println!("{DIM}  {}{RESET}", tip.content);
            println!();
        }
        None => eprintln!("{RED}  제보를 찾을 수 없습니다: {id}{RESET}\n"),
    }
}

fn handle_tip_update(args: &str, dir: &std::path::Path) {
    let (id, new_status) = match args.split_once(char::is_whitespace) {
        Some((i, s)) => (i.trim(), s.trim()),
        None => {
            eprintln!("{RED}  사용법: /tip update <ID> <상태>{RESET}");
            eprintln!(
                "{DIM}  상태: {}{RESET}\n",
                TIP_STATUSES.join(", ")
            );
            return;
        }
    };

    if !TIP_STATUSES.contains(&new_status) {
        eprintln!("{RED}  유효하지 않은 상태: {new_status}{RESET}");
        eprintln!(
            "{DIM}  사용 가능한 상태: {}{RESET}\n",
            TIP_STATUSES.join(", ")
        );
        return;
    }

    let path = dir.join(format!("{id}.json"));
    let mut tip = match load_tip(&path) {
        Some(t) => t,
        None => {
            eprintln!("{RED}  제보를 찾을 수 없습니다: {id}{RESET}\n");
            return;
        }
    };

    let old_status = tip.status.clone();
    tip.status = new_status.to_string();
    tip.updated_at = {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format_unix_timestamp(secs).replace(' ', "T") + ":00"
    };

    match save_tip(&tip, dir) {
        Ok(()) => println!(
            "{GREEN}  ✅ 제보 {id} 상태 변경: {old} → {new}{RESET}\n",
            old = old_status,
            new = new_status,
        ),
        Err(e) => eprintln!("{RED}  상태 변경 실패: {e}{RESET}\n"),
    }
}

fn handle_tip_search(keyword: &str, dir: &std::path::Path) {
    if keyword.is_empty() {
        eprintln!("{RED}  사용법: /tip search <키워드>{RESET}\n");
        return;
    }

    let tips = load_all_tips(dir);
    let kw_lower = keyword.to_lowercase();
    let matches: Vec<&TipEntry> = tips
        .iter()
        .filter(|t| {
            t.content.to_lowercase().contains(&kw_lower)
                || t.source.to_lowercase().contains(&kw_lower)
                || t.id.to_lowercase().contains(&kw_lower)
        })
        .collect();

    if matches.is_empty() {
        println!("{DIM}  '{keyword}'에 해당하는 제보가 없습니다.{RESET}\n");
        return;
    }

    println!(
        "{DIM}  검색 결과: {count}건 (키워드: {keyword}){RESET}",
        count = matches.len()
    );
    for tip in &matches {
        let preview: String = tip.content.chars().take(40).collect();
        let ellipsis = if tip.content.chars().count() > 40 { "…" } else { "" };
        println!(
            "{DIM}  [{status}] {id}  {src}  {preview}{ellipsis}{RESET}",
            status = tip.status,
            id = tip.id,
            src = if tip.anonymous { "익명" } else { &tip.source },
            preview = preview,
            ellipsis = ellipsis,
        );
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Constants ──────────────────────────────────────────────────────

    #[test]
    fn wire_subcommands_contains_save() {
        assert_eq!(WIRE_SUBCOMMANDS, &["save"]);
    }

    #[test]
    fn rss_subcommands_complete() {
        assert_eq!(RSS_SUBCOMMANDS, &["add", "list", "check", "search", "remove"]);
    }

    #[test]
    fn contact_subcommands_complete() {
        assert_eq!(CONTACT_SUBCOMMANDS, &["log", "history", "recent", "stale", "suggest"]);
    }

    #[test]
    fn tip_subcommands_complete() {
        assert_eq!(TIP_SUBCOMMANDS, &["add", "list", "show", "update", "search"]);
    }

    #[test]
    fn tip_statuses_complete() {
        assert_eq!(TIP_STATUSES, &["미확인", "취재중", "기사화", "보류", "폐기"]);
        assert_eq!(TIP_STATUSES.len(), 5);
    }

    // ── xml_extract_tag ───────────────────────────────────────────────

    #[test]
    fn xml_extract_tag_normal() {
        let xml = "<title>테스트 제목</title>";
        assert_eq!(xml_extract_tag(xml, "title"), Some("테스트 제목".to_string()));
    }

    #[test]
    fn xml_extract_tag_missing() {
        let xml = "<description>내용</description>";
        assert_eq!(xml_extract_tag(xml, "title"), None);
    }

    #[test]
    fn xml_extract_tag_cdata() {
        let xml = "<title><![CDATA[CDATA 제목]]></title>";
        assert_eq!(xml_extract_tag(xml, "title"), Some("CDATA 제목".to_string()));
    }

    #[test]
    fn xml_extract_tag_with_attributes() {
        let xml = r#"<link rel="alternate" href="x">링크텍스트</link>"#;
        assert_eq!(xml_extract_tag(xml, "link"), Some("링크텍스트".to_string()));
    }

    #[test]
    fn xml_extract_tag_nested() {
        let xml = "<desc><inner>중첩</inner></desc>";
        let result = xml_extract_tag(xml, "desc").unwrap();
        assert!(result.contains("중첩"));
    }

    #[test]
    fn xml_extract_tag_empty_content() {
        let xml = "<title></title>";
        assert_eq!(xml_extract_tag(xml, "title"), Some("".to_string()));
    }

    #[test]
    fn xml_extract_tag_whitespace() {
        let xml = "<title>  공백 있는 제목  </title>";
        assert_eq!(xml_extract_tag(xml, "title"), Some("공백 있는 제목".to_string()));
    }

    // ── parse_rss_items ───────────────────────────────────────────────

    #[test]
    fn parse_rss_items_normal() {
        let xml = r#"<?xml version="1.0"?>
<rss><channel>
<item>
  <title>제목1</title>
  <link>https://example.com/1</link>
  <description>설명1</description>
  <pubDate>Mon, 01 Apr 2026 09:00:00 +0900</pubDate>
</item>
<item>
  <title>제목2</title>
  <link>https://example.com/2</link>
  <description>설명2</description>
  <pubDate>Mon, 01 Apr 2026 10:00:00 +0900</pubDate>
</item>
</channel></rss>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "제목1");
        assert_eq!(items[0].link, "https://example.com/1");
        assert_eq!(items[0].description, "설명1");
        assert!(!items[0].pub_date.is_empty());
        assert_eq!(items[1].title, "제목2");
    }

    #[test]
    fn parse_rss_items_empty_xml() {
        let items = parse_rss_items("");
        assert!(items.is_empty());
    }

    #[test]
    fn parse_rss_items_no_items() {
        let xml = r#"<?xml version="1.0"?><rss><channel><title>Feed</title></channel></rss>"#;
        let items = parse_rss_items(xml);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_rss_items_cdata() {
        let xml = r#"<item>
  <title><![CDATA[CDATA 제목]]></title>
  <link>https://example.com/cdata</link>
  <description><![CDATA[<b>HTML</b> 내용]]></description>
  <pubDate>Tue, 02 Apr 2026 09:00:00 +0900</pubDate>
</item>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "CDATA 제목");
        assert_eq!(items[0].link, "https://example.com/cdata");
    }

    #[test]
    fn parse_rss_items_incomplete_item() {
        let xml = "<item><title>미완성";
        let items = parse_rss_items(xml);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_rss_items_missing_fields() {
        let xml = r#"<item><title>제목만</title></item>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "제목만");
        assert!(items[0].link.is_empty());
        assert!(items[0].description.is_empty());
    }

    #[test]
    fn parse_rss_items_with_attributes_on_item_tag() {
        let xml = r#"<item rdf:about="foo"><title>속성태그</title><link>http://x</link></item>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "속성태그");
    }

    #[test]
    fn parse_rss_items_html_in_description() {
        // Entity-encoded HTML: strip_html_tags strips real tags first, then decodes entities.
        // So &lt;b&gt; → passes tag stripping unchanged → decoded to <b>.
        let xml = r#"<item>
  <title>HTML 테스트</title>
  <link>http://test</link>
  <description><![CDATA[<b>굵게</b> 일반]]></description>
</item>"#;
        let items = parse_rss_items(xml);
        assert_eq!(items.len(), 1);
        // CDATA with real HTML tags: strip_html_tags removes <b></b>
        assert!(!items[0].description.contains("<b>"));
        assert!(items[0].description.contains("굵게"));
    }

    #[test]
    fn parse_rss_items_multiple_with_empty_title_and_link() {
        let xml = r#"<item><title></title><link></link></item>"#;
        let items = parse_rss_items(xml);
        assert!(items.is_empty());
    }

    // ── days_until ────────────────────────────────────────────────────

    #[test]
    fn days_until_future() {
        assert_eq!(days_until("2026-04-10", "2026-04-01"), Some(9));
    }

    #[test]
    fn days_until_past() {
        assert_eq!(days_until("2026-03-25", "2026-04-01"), Some(-7));
    }

    #[test]
    fn days_until_same_day() {
        assert_eq!(days_until("2026-04-01", "2026-04-01"), Some(0));
    }

    #[test]
    fn days_until_invalid_format() {
        assert_eq!(days_until("not-a-date", "2026-04-01"), None);
        assert_eq!(days_until("2026-04-01", "invalid"), None);
    }

    #[test]
    fn days_until_month_boundary() {
        assert_eq!(days_until("2026-04-01", "2026-03-31"), Some(1));
    }

    #[test]
    fn days_until_leap_year() {
        assert_eq!(days_until("2024-03-01", "2024-02-28"), Some(2));
    }

    #[test]
    fn days_until_year_boundary() {
        assert_eq!(days_until("2027-01-01", "2026-12-31"), Some(1));
    }

    #[test]
    fn days_until_invalid_month() {
        assert_eq!(days_until("2026-13-01", "2026-04-01"), None);
    }

    #[test]
    fn days_until_invalid_day() {
        assert_eq!(days_until("2026-04-32", "2026-04-01"), None);
    }

    // ── verify_file_path_with_date ────────────────────────────────────

    #[test]
    fn verify_file_path_with_date_normal() {
        let path = verify_file_path_with_date("삼성전자 주가 조작 의혹", "2026-04-01");
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("2026-04-01_"));
        assert!(name.ends_with(".md"));
        assert_eq!(path.parent().unwrap().to_str().unwrap(), VERIFY_DIR);
    }

    #[test]
    fn verify_file_path_with_date_empty_claim() {
        let path = verify_file_path_with_date("", "2026-04-01");
        let name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "2026-04-01_verify.md");
    }

    #[test]
    fn verify_file_path_with_date_different_dates() {
        let p1 = verify_file_path_with_date("test", "2026-01-01");
        let p2 = verify_file_path_with_date("test", "2026-12-31");
        assert_ne!(p1, p2);
        assert!(p1.to_str().unwrap().contains("2026-01-01"));
        assert!(p2.to_str().unwrap().contains("2026-12-31"));
    }

    // ── notes_file_for_date ───────────────────────────────────────────

    #[test]
    fn notes_file_for_date_normal() {
        let path = notes_file_for_date("2026-04-01");
        assert_eq!(path.to_str().unwrap(), ".journalist/notes/2026-04-01.jsonl");
    }

    #[test]
    fn notes_file_for_date_different_dates() {
        let p1 = notes_file_for_date("2026-01-15");
        let p2 = notes_file_for_date("2026-12-31");
        assert_ne!(p1, p2);
        assert!(p1.to_str().unwrap().ends_with("2026-01-15.jsonl"));
        assert!(p2.to_str().unwrap().ends_with("2026-12-31.jsonl"));
    }

    // ── tip_file_path_at ──────────────────────────────────────────────

    #[test]
    fn tip_file_path_at_normal() {
        let dir = std::path::Path::new("/tmp/tips");
        let path = tip_file_path_at(dir, "20260401-0930-1234");
        assert_eq!(path.to_str().unwrap(), "/tmp/tips/20260401-0930-1234.json");
    }

    #[test]
    fn tip_file_path_at_different_dir() {
        let dir = std::path::Path::new(".journalist/tips");
        let path = tip_file_path_at(dir, "test-id");
        assert_eq!(path.to_str().unwrap(), ".journalist/tips/test-id.json");
    }

    // ── detect_new_headlines ──────────────────────────────────────────

    #[test]
    fn detect_new_headlines_all_new() {
        let current = vec!["A".to_string(), "B".to_string()];
        let previous: Vec<String> = vec![];
        let result = detect_new_headlines(&current, &previous);
        assert_eq!(result, current);
    }

    #[test]
    fn detect_new_headlines_none_new() {
        let current = vec!["A".to_string(), "B".to_string()];
        let previous = vec!["A".to_string(), "B".to_string()];
        let result = detect_new_headlines(&current, &previous);
        assert!(result.is_empty());
    }

    #[test]
    fn detect_new_headlines_some_new() {
        let current = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let previous = vec!["A".to_string()];
        let result = detect_new_headlines(&current, &previous);
        assert_eq!(result, vec!["B".to_string(), "C".to_string()]);
    }

    #[test]
    fn detect_new_headlines_empty_current() {
        let current: Vec<String> = vec![];
        let previous = vec!["A".to_string()];
        let result = detect_new_headlines(&current, &previous);
        assert!(result.is_empty());
    }

    #[test]
    fn detect_new_headlines_both_empty() {
        let current: Vec<String> = vec![];
        let previous: Vec<String> = vec![];
        let result = detect_new_headlines(&current, &previous);
        assert!(result.is_empty());
    }

    #[test]
    fn detect_new_headlines_preserves_order() {
        let current = vec!["C".to_string(), "A".to_string(), "B".to_string()];
        let previous = vec!["A".to_string()];
        let result = detect_new_headlines(&current, &previous);
        assert_eq!(result, vec!["C".to_string(), "B".to_string()]);
    }

    // ── extract_naver_news_headlines (파싱만) ─────────────────────────

    #[test]
    fn extract_naver_headlines_normal() {
        let html = r#"<div class="news_tit" title="첫번째 뉴스">text</div>
<div class="news_tit" title="두번째 뉴스">text</div>"#;
        let result = extract_naver_news_headlines(html, 10);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "첫번째 뉴스");
        assert_eq!(result[1], "두번째 뉴스");
    }

    #[test]
    fn extract_naver_headlines_empty_html() {
        let result = extract_naver_news_headlines("", 10);
        assert!(result.is_empty());
    }

    #[test]
    fn extract_naver_headlines_no_news() {
        let html = "<html><body>No news here</body></html>";
        let result = extract_naver_news_headlines(html, 10);
        assert!(result.is_empty());
    }

    #[test]
    fn extract_naver_headlines_respects_max() {
        let html = r#"<a class="news_tit" title="뉴스1">x</a>
<a class="news_tit" title="뉴스2">x</a>
<a class="news_tit" title="뉴스3">x</a>"#;
        let result = extract_naver_news_headlines(html, 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn extract_naver_headlines_html_entities() {
        let html = r#"<a class="news_tit" title="A &amp; B &lt;C&gt;">x</a>"#;
        let result = extract_naver_news_headlines(html, 10);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "A & B <C>");
    }

    #[test]
    fn extract_naver_headlines_empty_title_skipped() {
        let html = r#"<a class="news_tit" title="">x</a>
<a class="news_tit" title="실제뉴스">x</a>"#;
        let result = extract_naver_news_headlines(html, 10);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "실제뉴스");
    }

    // ── is_valid_date ─────────────────────────────────────────────────

    #[test]
    fn is_valid_date_normal() {
        assert!(is_valid_date("2026-04-01"));
        assert!(is_valid_date("2024-02-29"));
    }

    #[test]
    fn is_valid_date_invalid() {
        assert!(!is_valid_date(""));
        assert!(!is_valid_date("not-date"));
        assert!(!is_valid_date("2026-13-01"));
        assert!(!is_valid_date("2026-00-01"));
        assert!(!is_valid_date("2026-04-32"));
        assert!(!is_valid_date("2026-04-00"));
        assert!(!is_valid_date("20260401"));
    }

    // ── RSS feed save/load roundtrip ──────────────────────────────────

    #[test]
    fn rss_feeds_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        let feeds = vec![
            RssFeed {
                url: "https://example.com/rss".to_string(),
                name: "테스트피드".to_string(),
                added: "2026-04-01 09:00:00".to_string(),
            },
        ];
        save_rss_feeds_to(&feeds, &path);
        let loaded = load_rss_feeds_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, "https://example.com/rss");
        assert_eq!(loaded[0].name, "테스트피드");
    }

    #[test]
    fn rss_feeds_load_nonexistent() {
        let loaded = load_rss_feeds_from(std::path::Path::new("/nonexistent/feeds.json"));
        assert!(loaded.is_empty());
    }

    // ── RSS cache save/load roundtrip ─────────────────────────────────

    #[test]
    fn rss_cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let items = vec![NewsItem {
            title: "캐시 테스트".to_string(),
            link: "https://example.com/1".to_string(),
            description: "설명".to_string(),
            pub_date: "2026-04-01".to_string(),
        }];
        save_rss_cache_to(&items, &path);
        let loaded = load_rss_cache_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "캐시 테스트");
        assert_eq!(loaded[0].link, "https://example.com/1");
    }

    #[test]
    fn rss_cache_load_nonexistent() {
        let loaded = load_rss_cache_from(std::path::Path::new("/nonexistent/cache.json"));
        assert!(loaded.is_empty());
    }

    // ── rss_cache_filename ────────────────────────────────────────────

    #[test]
    fn rss_cache_filename_strips_scheme() {
        let name = rss_cache_filename("https://www.yna.co.kr/rss/news.xml");
        assert!(!name.contains("https"));
        assert!(!name.is_empty());
    }

    #[test]
    fn rss_cache_filename_empty_url_fallback() {
        let name = rss_cache_filename("https://");
        assert!(!name.is_empty());
    }

    // ── Note roundtrip ────────────────────────────────────────────────

    #[test]
    fn note_append_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("2026-04-01.jsonl");
        let note = Note {
            content: "테스트 메모".to_string(),
            source: Some("테스트".to_string()),
            topic: None,
            timestamp: "2026-04-01 09:00:00".to_string(),
        };
        append_note_to(&note, &path);
        let loaded = load_notes_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "테스트 메모");
    }

    #[test]
    fn load_all_notes_from_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let note1 = Note {
            content: "Day1".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-04-01".to_string(),
        };
        let note2 = Note {
            content: "Day2".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-04-02".to_string(),
        };
        append_note_to(&note1, &dir.path().join("2026-04-01.jsonl"));
        append_note_to(&note2, &dir.path().join("2026-04-02.jsonl"));
        let all = load_all_notes_from(dir.path());
        assert_eq!(all.len(), 2);
    }

    // ── TipEntry save/load roundtrip ──────────────────────────────────

    #[test]
    fn tip_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let tip = TipEntry {
            id: "20260401-0930-0001".to_string(),
            source: "기자A".to_string(),
            content: "제보 내용".to_string(),
            anonymous: false,
            credibility: 3,
            status: "미확인".to_string(),
            created_at: "2026-04-01 09:30:00".to_string(),
            updated_at: "2026-04-01 09:30:00".to_string(),
            linked_story: None,
        };
        save_tip(&tip, dir.path()).unwrap();
        let loaded = load_tip(&dir.path().join("20260401-0930-0001.json")).unwrap();
        assert_eq!(loaded, tip);
    }

    #[test]
    fn load_all_tips_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let tips = load_all_tips(dir.path());
        assert!(tips.is_empty());
    }

    #[test]
    fn load_all_tips_multiple() {
        let dir = tempfile::tempdir().unwrap();
        for i in 1..=3 {
            let tip = TipEntry {
                id: format!("tip-{i}"),
                source: "src".to_string(),
                content: format!("content {i}"),
                anonymous: false,
                credibility: 3,
                status: "미확인".to_string(),
                created_at: format!("2026-04-0{i} 09:00:00"),
                updated_at: format!("2026-04-0{i} 09:00:00"),
                linked_story: None,
            };
            save_tip(&tip, dir.path()).unwrap();
        }
        let tips = load_all_tips(dir.path());
        assert_eq!(tips.len(), 3);
    }

    // ══════════════════════════════════════════════════════════════════════
    // Task 2: 데이터 I/O · CRUD 단위 테스트 (tempdir 기반)
    // ══════════════════════════════════════════════════════════════════════

    // ── RSS feed save/load edge cases ────────────────────────────────────

    #[test]
    fn rss_feeds_load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        std::fs::write(&path, "").unwrap();
        let loaded = load_rss_feeds_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_feeds_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        std::fs::write(&path, "NOT JSON AT ALL {{{").unwrap();
        let loaded = load_rss_feeds_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_feeds_save_and_load_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        let feeds = vec![
            RssFeed {
                url: "https://a.com/rss".to_string(),
                name: "피드A".to_string(),
                added: "2026-04-01".to_string(),
            },
            RssFeed {
                url: "https://b.com/rss".to_string(),
                name: "피드B".to_string(),
                added: "2026-04-02".to_string(),
            },
            RssFeed {
                url: "https://c.com/rss".to_string(),
                name: "".to_string(),
                added: "".to_string(),
            },
        ];
        save_rss_feeds_to(&feeds, &path);
        let loaded = load_rss_feeds_from(&path);
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[0].name, "피드A");
        assert_eq!(loaded[1].url, "https://b.com/rss");
        assert_eq!(loaded[2].name, "");
        assert_eq!(loaded[2].added, "");
    }

    #[test]
    fn rss_feeds_save_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        save_rss_feeds_to(&[], &path);
        let loaded = load_rss_feeds_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_feeds_load_array_with_missing_url_field() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        // url is missing → filter_map returns None → entry skipped
        std::fs::write(&path, r#"[{"name":"no-url","added":"2026-04-01"}]"#).unwrap();
        let loaded = load_rss_feeds_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_feeds_overwrite_preserves_latest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feeds.json");
        let v1 = vec![RssFeed {
            url: "https://old.com".to_string(),
            name: "Old".to_string(),
            added: "".to_string(),
        }];
        save_rss_feeds_to(&v1, &path);
        let v2 = vec![RssFeed {
            url: "https://new.com".to_string(),
            name: "New".to_string(),
            added: "".to_string(),
        }];
        save_rss_feeds_to(&v2, &path);
        let loaded = load_rss_feeds_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, "https://new.com");
    }

    // ── RSS cache edge cases ─────────────────────────────────────────────

    #[test]
    fn rss_cache_load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, "").unwrap();
        let loaded = load_rss_cache_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_cache_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, "broken json!").unwrap();
        let loaded = load_rss_cache_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_cache_save_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        save_rss_cache_to(&[], &path);
        let loaded = load_rss_cache_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn rss_cache_multiple_items_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        let items: Vec<NewsItem> = (1..=5)
            .map(|i| NewsItem {
                title: format!("뉴스{i}"),
                link: format!("https://example.com/{i}"),
                description: format!("설명{i}"),
                pub_date: format!("2026-04-0{i}"),
            })
            .collect();
        save_rss_cache_to(&items, &path);
        let loaded = load_rss_cache_from(&path);
        assert_eq!(loaded.len(), 5);
        assert_eq!(loaded[2].title, "뉴스3");
        assert_eq!(loaded[4].link, "https://example.com/5");
    }

    // ── load_notes_from edge cases ───────────────────────────────────────

    #[test]
    fn load_notes_from_nonexistent() {
        let loaded = load_notes_from(std::path::Path::new("/nonexistent/notes.jsonl"));
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_notes_from_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        std::fs::write(&path, "").unwrap();
        let loaded = load_notes_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_notes_from_invalid_json_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jsonl");
        std::fs::write(&path, "NOT JSON\nALSO NOT JSON\n").unwrap();
        let loaded = load_notes_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn load_notes_from_mixed_valid_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mixed.jsonl");
        let valid = r#"{"content":"유효","timestamp":"2026-04-01"}"#;
        std::fs::write(&path, format!("INVALID LINE\n{valid}\nANOTHER BAD\n")).unwrap();
        let loaded = load_notes_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "유효");
    }

    #[test]
    fn notes_append_multiple_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("multi.jsonl");
        for i in 1..=4 {
            let note = Note {
                content: format!("메모{i}"),
                source: None,
                topic: Some(format!("주제{i}")),
                timestamp: format!("2026-04-01 0{i}:00:00"),
            };
            append_note_to(&note, &path);
        }
        let loaded = load_notes_from(&path);
        assert_eq!(loaded.len(), 4);
        assert_eq!(loaded[0].content, "메모1");
        assert_eq!(loaded[3].topic, Some("주제4".to_string()));
    }

    #[test]
    fn load_all_notes_from_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let notes = load_all_notes_from(dir.path());
        assert!(notes.is_empty());
    }

    #[test]
    fn load_all_notes_from_nonexistent_dir() {
        let notes = load_all_notes_from(std::path::Path::new("/nonexistent/notes/"));
        assert!(notes.is_empty());
    }

    #[test]
    fn load_all_notes_ignores_non_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        // Write a .txt file — should be ignored
        std::fs::write(dir.path().join("stray.txt"), "not a note").unwrap();
        let note = Note {
            content: "진짜메모".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-04-01".to_string(),
        };
        append_note_to(&note, &dir.path().join("2026-04-01.jsonl"));
        let all = load_all_notes_from(dir.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].content, "진짜메모");
    }

    // ── append_contact_log / load_contact_logs_from ──────────────────────

    #[test]
    fn contact_log_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source_a.jsonl");
        let log = ContactLog {
            name: "김기자".to_string(),
            summary: "경제 브리핑 관련 전화".to_string(),
            timestamp: "2026-04-01T09:30:00".to_string(),
        };
        append_contact_log(&log, &path);
        let loaded = load_contact_logs_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "김기자");
        assert_eq!(loaded[0].summary, "경제 브리핑 관련 전화");
    }

    #[test]
    fn contact_log_append_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("contact.jsonl");
        for i in 1..=3 {
            let log = ContactLog {
                name: format!("취재원{i}"),
                summary: format!("대화{i}"),
                timestamp: format!("2026-04-0{i}T10:00:00"),
            };
            append_contact_log(&log, &path);
        }
        let loaded = load_contact_logs_from(&path);
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[2].name, "취재원3");
    }

    #[test]
    fn contact_log_load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        std::fs::write(&path, "").unwrap();
        let loaded = load_contact_logs_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn contact_log_load_nonexistent() {
        let loaded = load_contact_logs_from(std::path::Path::new("/no/such/file.jsonl"));
        assert!(loaded.is_empty());
    }

    #[test]
    fn contact_log_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jsonl");
        std::fs::write(&path, "NOT JSON\n").unwrap();
        let loaded = load_contact_logs_from(&path);
        assert!(loaded.is_empty());
    }

    // ── load_all_contact_logs path structure ─────────────────────────────
    // load_all_contact_logs() reads from the hardcoded contacts_dir(),
    // but we can verify the helper functions it delegates to.

    #[test]
    fn contact_file_for_sanitizes_name() {
        let path = contact_file_for("김 기자/특수문자!");
        let name = path.file_name().unwrap().to_str().unwrap();
        // Spaces and special chars replaced with _
        assert!(!name.contains(' '));
        assert!(!name.contains('/'));
        assert!(!name.contains('!'));
        assert!(name.ends_with(".jsonl"));
    }

    #[test]
    fn contact_file_for_preserves_alphanumeric() {
        let path = contact_file_for("abc_123-test");
        let name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "abc_123-test.jsonl");
    }

    // ── TipEntry CRUD: save/load/status change/search ────────────────────

    fn make_tip(id: &str, status: &str, content: &str) -> TipEntry {
        TipEntry {
            id: id.to_string(),
            source: "테스트출처".to_string(),
            content: content.to_string(),
            anonymous: false,
            credibility: 3,
            status: status.to_string(),
            created_at: "2026-04-01 09:00:00".to_string(),
            updated_at: "2026-04-01 09:00:00".to_string(),
            linked_story: None,
        }
    }

    #[test]
    fn tip_save_load_roundtrip_with_linked_story() {
        let dir = tempfile::tempdir().unwrap();
        let mut tip = make_tip("tip-linked", "미확인", "연결된 제보");
        tip.linked_story = Some("삼성전자 기사".to_string());
        tip.anonymous = true;
        tip.credibility = 5;
        save_tip(&tip, dir.path()).unwrap();
        let loaded = load_tip(&dir.path().join("tip-linked.json")).unwrap();
        assert_eq!(loaded.linked_story, Some("삼성전자 기사".to_string()));
        assert!(loaded.anonymous);
        assert_eq!(loaded.credibility, 5);
    }

    #[test]
    fn tip_load_nonexistent_returns_none() {
        let result = load_tip(std::path::Path::new("/nonexistent/tip.json"));
        assert!(result.is_none());
    }

    #[test]
    fn tip_load_invalid_json_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json").unwrap();
        assert!(load_tip(&path).is_none());
    }

    #[test]
    fn tip_status_change_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut tip = make_tip("tip-status", "미확인", "상태변경 테스트");
        save_tip(&tip, dir.path()).unwrap();

        // Update status
        tip.status = "취재중".to_string();
        tip.updated_at = "2026-04-01 12:00:00".to_string();
        save_tip(&tip, dir.path()).unwrap();

        let loaded = load_tip(&dir.path().join("tip-status.json")).unwrap();
        assert_eq!(loaded.status, "취재중");
        assert_eq!(loaded.updated_at, "2026-04-01 12:00:00");
    }

    #[test]
    fn tip_all_statuses_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        for (i, status) in TIP_STATUSES.iter().enumerate() {
            let tip = make_tip(&format!("tip-s{i}"), status, &format!("상태 {status}"));
            save_tip(&tip, dir.path()).unwrap();
        }
        let tips = load_all_tips(dir.path());
        assert_eq!(tips.len(), TIP_STATUSES.len());
        let statuses: Vec<&str> = tips.iter().map(|t| t.status.as_str()).collect();
        for s in TIP_STATUSES {
            assert!(statuses.contains(s), "missing status: {s}");
        }
    }

    #[test]
    fn load_all_tips_sorted_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let tip_old = TipEntry {
            created_at: "2026-03-01 09:00:00".to_string(),
            ..make_tip("tip-old", "미확인", "오래된")
        };
        let tip_new = TipEntry {
            created_at: "2026-04-01 09:00:00".to_string(),
            ..make_tip("tip-new", "미확인", "최신")
        };
        save_tip(&tip_old, dir.path()).unwrap();
        save_tip(&tip_new, dir.path()).unwrap();
        let tips = load_all_tips(dir.path());
        assert_eq!(tips[0].id, "tip-new");
        assert_eq!(tips[1].id, "tip-old");
    }

    #[test]
    fn load_all_tips_ignores_non_json() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "not a tip").unwrap();
        save_tip(&make_tip("real-tip", "미확인", "진짜"), dir.path()).unwrap();
        let tips = load_all_tips(dir.path());
        assert_eq!(tips.len(), 1);
        assert_eq!(tips[0].id, "real-tip");
    }

    #[test]
    fn tip_search_by_content_keyword() {
        let dir = tempfile::tempdir().unwrap();
        save_tip(&make_tip("t1", "미확인", "반도체 수출 규제"), dir.path()).unwrap();
        save_tip(&make_tip("t2", "미확인", "자동차 리콜 소식"), dir.path()).unwrap();
        save_tip(&make_tip("t3", "취재중", "반도체 시장 전망"), dir.path()).unwrap();

        let all = load_all_tips(dir.path());
        let filtered: Vec<&TipEntry> = all
            .iter()
            .filter(|t| t.content.contains("반도체"))
            .collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn tip_search_by_status() {
        let dir = tempfile::tempdir().unwrap();
        save_tip(&make_tip("t1", "미확인", "제보1"), dir.path()).unwrap();
        save_tip(&make_tip("t2", "취재중", "제보2"), dir.path()).unwrap();
        save_tip(&make_tip("t3", "기사화", "제보3"), dir.path()).unwrap();
        save_tip(&make_tip("t4", "취재중", "제보4"), dir.path()).unwrap();

        let all = load_all_tips(dir.path());
        let investigating: Vec<&TipEntry> = all
            .iter()
            .filter(|t| t.status == "취재중")
            .collect();
        assert_eq!(investigating.len(), 2);
    }

    // ── Followup roundtrip ───────────────────────────────────────────────

    #[test]
    fn followup_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followups.json");
        let followups = vec![
            Followup {
                topic: "국정감사 후속보도".to_string(),
                due: Some("2026-04-10".to_string()),
                done: false,
                created_at: "2026-04-01T09:00:00".to_string(),
            },
            Followup {
                topic: "재판 결과 확인".to_string(),
                due: None,
                done: true,
                created_at: "2026-03-25T14:00:00".to_string(),
            },
        ];
        save_followups_to(&followups, &path);
        let loaded = load_followups_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].topic, "국정감사 후속보도");
        assert_eq!(loaded[0].due, Some("2026-04-10".to_string()));
        assert!(!loaded[0].done);
        assert_eq!(loaded[1].topic, "재판 결과 확인");
        assert!(loaded[1].due.is_none());
        assert!(loaded[1].done);
    }

    #[test]
    fn followup_load_nonexistent() {
        let loaded = load_followups_from(std::path::Path::new("/nonexistent/followups.json"));
        assert!(loaded.is_empty());
    }

    #[test]
    fn followup_load_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followups.json");
        std::fs::write(&path, "").unwrap();
        let loaded = load_followups_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn followup_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followups.json");
        std::fs::write(&path, "NOT JSON").unwrap();
        let loaded = load_followups_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn followup_save_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followups.json");
        save_followups_to(&[], &path);
        let loaded = load_followups_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn followup_overwrite_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("followups.json");
        let v1 = vec![Followup {
            topic: "원본".to_string(),
            due: None,
            done: false,
            created_at: "2026-04-01T09:00:00".to_string(),
        }];
        save_followups_to(&v1, &path);
        // Overwrite with different data
        let v2 = vec![Followup {
            topic: "수정됨".to_string(),
            due: Some("2026-05-01".to_string()),
            done: true,
            created_at: "2026-04-01T10:00:00".to_string(),
        }];
        save_followups_to(&v2, &path);
        let loaded = load_followups_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].topic, "수정됨");
        assert!(loaded[0].done);
    }

    // ── monitor history roundtrip ────────────────────────────────────────

    #[test]
    fn monitor_history_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let keyword = "반도체";
        let history = vec![
            serde_json::json!({
                "date": "2026-04-01",
                "headlines": ["뉴스A", "뉴스B"]
            }),
            serde_json::json!({
                "date": "2026-03-31",
                "headlines": ["뉴스C"]
            }),
        ];
        save_monitor_history_to(keyword, &history, dir.path());
        let loaded = load_monitor_history_from(keyword, dir.path());
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["date"], "2026-04-01");
        let headlines = loaded[0]["headlines"].as_array().unwrap();
        assert_eq!(headlines.len(), 2);
    }

    #[test]
    fn monitor_history_load_nonexistent() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_monitor_history_from("없는키워드", dir.path());
        assert!(loaded.is_empty());
    }

    #[test]
    fn monitor_history_save_empty() {
        let dir = tempfile::tempdir().unwrap();
        save_monitor_history_to("빈히스토리", &[], dir.path());
        let loaded = load_monitor_history_from("빈히스토리", dir.path());
        assert!(loaded.is_empty());
    }

    #[test]
    fn monitor_history_different_keywords_independent() {
        let dir = tempfile::tempdir().unwrap();
        let h1 = vec![serde_json::json!({"k": "반도체"})];
        let h2 = vec![serde_json::json!({"k": "자동차"})];
        save_monitor_history_to("반도체", &h1, dir.path());
        save_monitor_history_to("자동차", &h2, dir.path());
        let l1 = load_monitor_history_from("반도체", dir.path());
        let l2 = load_monitor_history_from("자동차", dir.path());
        assert_eq!(l1.len(), 1);
        assert_eq!(l2.len(), 1);
        assert_eq!(l1[0]["k"], "반도체");
        assert_eq!(l2[0]["k"], "자동차");
    }

    #[test]
    fn monitor_history_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let v1 = vec![serde_json::json!({"ver": 1})];
        save_monitor_history_to("kw", &v1, dir.path());
        let v2 = vec![serde_json::json!({"ver": 2}), serde_json::json!({"ver": 3})];
        save_monitor_history_to("kw", &v2, dir.path());
        let loaded = load_monitor_history_from("kw", dir.path());
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["ver"], 2);
    }

    // ── build_verify_prompt ──────────────────────────────────────────────

    #[test]
    fn build_verify_prompt_empty_claim_returns_none() {
        assert!(build_verify_prompt("").is_none());
    }

    #[test]
    fn build_verify_prompt_normal_claim() {
        let prompt = build_verify_prompt("삼성전자 주가 상승").unwrap();
        assert!(prompt.contains("삼성전자 주가 상승"));
        assert!(prompt.contains("교차검증"));
        assert!(prompt.contains("BIG Kinds"));
        assert!(prompt.contains("DART"));
    }

    #[test]
    fn build_verify_prompt_contains_data_sources() {
        let prompt = build_verify_prompt("테스트 주장").unwrap();
        assert!(prompt.contains("뉴스 검색"));
        assert!(prompt.contains("BIG Kinds"));
        assert!(prompt.contains("DART"));
        assert!(prompt.contains("공공데이터"));
    }

    #[test]
    fn build_verify_prompt_contains_report_format() {
        let prompt = build_verify_prompt("테스트").unwrap();
        assert!(prompt.contains("교차검증 보고서"));
        assert!(prompt.contains("검증 대상"));
        assert!(prompt.contains("소스별 검증 결과"));
        assert!(prompt.contains("종합 판정"));
        assert!(prompt.contains("✅ 확인됨"));
    }

    #[test]
    fn build_verify_prompt_special_chars_in_claim() {
        let prompt = build_verify_prompt("100조원 & <특수문자> \"인용\"").unwrap();
        assert!(prompt.contains("100조원 & <특수문자> \"인용\""));
    }

    // ── parse_timestamp_secs ─────────────────────────────────────────────

    #[test]
    fn parse_timestamp_secs_iso_format() {
        let secs = parse_timestamp_secs("2026-04-01T09:30:00");
        assert!(secs.is_some());
        let val = secs.unwrap();
        assert!(val > 0);
    }

    #[test]
    fn parse_timestamp_secs_space_format() {
        let secs = parse_timestamp_secs("2026-04-01 09:30:00");
        assert!(secs.is_some());
    }

    #[test]
    fn parse_timestamp_secs_date_only() {
        let secs = parse_timestamp_secs("2026-04-01");
        assert!(secs.is_some());
        // hour/min/sec default to 0
        let with_time = parse_timestamp_secs("2026-04-01T00:00:00").unwrap();
        assert_eq!(secs.unwrap(), with_time);
    }

    #[test]
    fn parse_timestamp_secs_invalid() {
        assert!(parse_timestamp_secs("").is_none());
        assert!(parse_timestamp_secs("not-a-time").is_none());
    }

    #[test]
    fn parse_timestamp_secs_ordering() {
        let earlier = parse_timestamp_secs("2026-04-01T09:00:00").unwrap();
        let later = parse_timestamp_secs("2026-04-01T10:00:00").unwrap();
        assert!(later > earlier);
    }

    // ── parse_follow_add_args ────────────────────────────────────────────

    #[test]
    fn parse_follow_add_args_topic_only() {
        let (topic, due) = parse_follow_add_args("국정감사 후속");
        assert_eq!(topic, "국정감사 후속");
        assert!(due.is_none());
    }

    #[test]
    fn parse_follow_add_args_with_due() {
        let (topic, due) = parse_follow_add_args("재판 결과 --due 2026-04-15");
        assert_eq!(topic, "재판 결과");
        assert_eq!(due, Some("2026-04-15".to_string()));
    }

    #[test]
    fn parse_follow_add_args_empty_due() {
        let (topic, due) = parse_follow_add_args("주제 --due");
        assert_eq!(topic, "주제");
        assert!(due.is_none());
    }

    #[test]
    fn parse_follow_add_args_empty_input() {
        let (topic, due) = parse_follow_add_args("");
        assert_eq!(topic, "");
        assert!(due.is_none());
    }

    #[test]
    fn append_note_to_unwritable_path_does_not_panic() {
        let note = Note {
            content: "test".to_string(),
            source: None,
            topic: None,
            timestamp: "2026-04-01T12:00:00".to_string(),
        };
        // /proc/nonexistent/file is unwritable on Linux; should not panic
        let bad_path = std::path::Path::new("/proc/nonexistent/notes.jsonl");
        append_note_to(&note, bad_path);
        // If we reach here, no panic occurred — test passes
    }
}
