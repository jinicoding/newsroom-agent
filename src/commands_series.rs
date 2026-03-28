//! Series (연재기사) management command handlers.
//! Commands: /series (new, list, add, status, recap, link)

use crate::commands_project::{today_str, topic_to_slug};
use crate::format::*;

// ── /series — 연재기사 관리 ──────────────────────────────────────────────

/// Base directory for series workspaces.
pub const SERIES_DIR: &str = ".journalist/series";

/// Subcommand names for `/series <Tab>` completion.
pub const SERIES_SUBCOMMANDS: &[&str] = &["new", "list", "add", "status", "recap", "link"];

/// Valid series statuses.
#[cfg(test)]
pub const SERIES_STATUSES: &[&str] = &["진행중", "휴재", "완결"];

/// Metadata for a series installment.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SeriesInstallment {
    pub number: u32,
    pub title: String,
    pub added_date: String,
    #[serde(default)]
    pub story_slug: Option<String>,
}

/// Metadata for a series.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SeriesMeta {
    pub title: String,
    pub slug: String,
    pub status: String,
    pub created: String,
    #[serde(default)]
    pub installments: Vec<SeriesInstallment>,
}

/// Generate a slug for series names.
pub fn series_slug(title: &str) -> String {
    topic_to_slug(title, 50)
}

/// Return the series base directory path.
fn series_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(SERIES_DIR)
}

/// Return the path to a series metadata file.
fn series_meta_path(base: &std::path::Path, slug: &str) -> std::path::PathBuf {
    base.join(slug).join("series.json")
}

/// Load series metadata from a series.json file.
pub fn load_series_meta(path: &std::path::Path) -> Option<SeriesMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save series metadata to a series.json file.
pub fn save_series_meta(meta: &SeriesMeta, path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("디렉토리 생성 실패: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(meta).map_err(|e| format!("직렬화 실패: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("저장 실패: {e}"))?;
    Ok(())
}

/// Create a new series.
pub fn create_series(
    title: &str,
    base: &std::path::Path,
    date: &str,
) -> Result<SeriesMeta, String> {
    if title.trim().is_empty() {
        return Err("시리즈 제목을 입력하세요".to_string());
    }
    let slug = series_slug(title);
    if slug.is_empty() {
        return Err("유효한 제목을 입력하세요".to_string());
    }
    let meta_path = series_meta_path(base, &slug);
    if meta_path.exists() {
        return Err(format!("이미 존재하는 시리즈입니다: {slug}"));
    }

    let meta = SeriesMeta {
        title: title.to_string(),
        slug: slug.clone(),
        status: "진행중".to_string(),
        created: date.to_string(),
        installments: Vec::new(),
    };

    save_series_meta(&meta, &meta_path)?;
    Ok(meta)
}

/// Add an installment to a series. Returns the assigned number.
pub fn add_series_installment(
    slug: &str,
    inst_title: &str,
    base: &std::path::Path,
    date: &str,
) -> Result<u32, String> {
    if inst_title.trim().is_empty() {
        return Err("회차 제목을 입력하세요".to_string());
    }
    let path = series_meta_path(base, slug);
    let mut meta =
        load_series_meta(&path).ok_or_else(|| format!("시리즈를 찾을 수 없습니다: {slug}"))?;

    let next_number = meta.installments.iter().map(|i| i.number).max().unwrap_or(0) + 1;
    meta.installments.push(SeriesInstallment {
        number: next_number,
        title: inst_title.to_string(),
        added_date: date.to_string(),
        story_slug: None,
    });
    save_series_meta(&meta, &path)?;
    Ok(next_number)
}

/// Link a story slug to a series installment (latest if no number specified).
pub fn link_story_to_series(
    series_slug_str: &str,
    story_slug_str: &str,
    base: &std::path::Path,
) -> Result<u32, String> {
    let path = series_meta_path(base, series_slug_str);
    let mut meta = load_series_meta(&path)
        .ok_or_else(|| format!("시리즈를 찾을 수 없습니다: {series_slug_str}"))?;

    if meta.installments.is_empty() {
        return Err("회차가 없습니다. /series add로 먼저 추가하세요".to_string());
    }

    // Link to the latest installment that has no story yet, or the last one
    let idx = meta
        .installments
        .iter()
        .rposition(|i| i.story_slug.is_none())
        .unwrap_or(meta.installments.len() - 1);

    meta.installments[idx].story_slug = Some(story_slug_str.to_string());
    let number = meta.installments[idx].number;
    save_series_meta(&meta, &path)?;
    Ok(number)
}

/// List all series under a base directory.
pub fn list_series(base: &std::path::Path) -> Vec<SeriesMeta> {
    let mut all = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return all,
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            let meta_path = entry.path().join("series.json");
            if let Some(meta) = load_series_meta(&meta_path) {
                all.push(meta);
            }
        }
    }
    all.sort_by(|a, b| a.created.cmp(&b.created));
    all
}

/// Format series status for display.
pub fn format_series_status(meta: &SeriesMeta) -> String {
    let total = meta.installments.len();
    let linked = meta.installments.iter().filter(|i| i.story_slug.is_some()).count();
    format!(
        "[{}] {} — {} ({}회, {}개 연결)",
        meta.status, meta.title, meta.created, total, linked
    )
}

/// Format detailed series info for display.
pub fn format_series_detail(meta: &SeriesMeta) -> String {
    let mut out = format!(
        "📚 {} ({})\n  상태: {} | 생성: {}\n",
        meta.title, meta.slug, meta.status, meta.created
    );
    if meta.installments.is_empty() {
        out.push_str("  회차 없음\n");
    } else {
        out.push_str("  회차:\n");
        for inst in &meta.installments {
            let story_info = match &inst.story_slug {
                Some(s) => format!(" → {s}"),
                None => String::new(),
            };
            out.push_str(&format!(
                "    {}회: {} ({}){}\n",
                inst.number, inst.title, inst.added_date, story_info
            ));
        }
    }
    out
}

/// Handle the `/series` command.
pub fn handle_series(input: &str) {
    let args = input.strip_prefix("/series").unwrap_or("").trim();

    if args.is_empty() {
        handle_series_list_cmd(&series_dir());
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "new" => handle_series_new_cmd(rest),
        "list" => handle_series_list_cmd(&series_dir()),
        "add" => handle_series_add_cmd(rest),
        "status" => handle_series_status_cmd(rest),
        "recap" => handle_series_recap_cmd(rest),
        "link" => handle_series_link_cmd(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_series_usage();
        }
    }
}

fn print_series_usage() {
    println!("{DIM}  사용법:");
    println!("    /series new <제목>              새 시리즈 생성");
    println!("    /series list                    시리즈 목록");
    println!("    /series add <시리즈> <회차제목>  새 회차 추가");
    println!("    /series status <시리즈>         시리즈 현황");
    println!("    /series recap <시리즈>          AI 요약 생성 (대화형)");
    println!("    /series link <시리즈> <story>   story 프로젝트 연결");
    println!("    /series                         (list와 동일){RESET}\n");
}

fn handle_series_new_cmd(title: &str) {
    let base = series_dir();
    let date = today_str();
    match create_series(title, &base, &date) {
        Ok(meta) => {
            println!(
                "{GREEN}  ✓ 시리즈 생성: {}{RESET}",
                meta.title
            );
            println!(
                "{DIM}    경로: {}/{}{RESET}\n",
                SERIES_DIR, meta.slug
            );
        }
        Err(e) => eprintln!("{RED}  {e}{RESET}\n"),
    }
}

fn handle_series_list_cmd(base: &std::path::Path) {
    let all = list_series(base);
    if all.is_empty() {
        println!("{DIM}  진행 중인 시리즈가 없습니다.");
        println!("  /series new <제목>으로 시작하세요{RESET}\n");
        return;
    }

    // Group by status
    let mut by_status: std::collections::BTreeMap<String, Vec<&SeriesMeta>> =
        std::collections::BTreeMap::new();
    for s in &all {
        by_status.entry(s.status.clone()).or_default().push(s);
    }

    println!("{BOLD}  📚 연재 시리즈{RESET}");
    for (status, items) in &by_status {
        println!("  {BOLD}{status}{RESET} ({}):", items.len());
        for s in items {
            println!("    • {}", format_series_status(s));
        }
    }
    println!();
}

fn handle_series_add_cmd(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /series add <시리즈slug> <회차제목>{RESET}\n");
        return;
    }

    let (slug, title) = match args.split_once(char::is_whitespace) {
        Some((s, t)) => (s.trim(), t.trim()),
        None => {
            eprintln!("{RED}  사용법: /series add <시리즈slug> <회차제목>{RESET}\n");
            return;
        }
    };

    let base = series_dir();
    let date = today_str();
    match add_series_installment(slug, title, &base, &date) {
        Ok(num) => {
            println!("{GREEN}  ✓ {slug} 시리즈에 {num}회 추가: {title}{RESET}\n");
        }
        Err(e) => eprintln!("{RED}  {e}{RESET}\n"),
    }
}

fn handle_series_status_cmd(slug: &str) {
    if slug.is_empty() {
        eprintln!("{RED}  사용법: /series status <시리즈slug>{RESET}\n");
        return;
    }

    let base = series_dir();
    let path = series_meta_path(&base, slug);
    match load_series_meta(&path) {
        Some(meta) => {
            println!("{}", format_series_detail(&meta));
        }
        None => eprintln!("{RED}  시리즈를 찾을 수 없습니다: {slug}{RESET}\n"),
    }
}

fn handle_series_recap_cmd(slug: &str) {
    if slug.is_empty() {
        eprintln!("{RED}  사용법: /series recap <시리즈slug>{RESET}\n");
        return;
    }

    let base = series_dir();
    let path = series_meta_path(&base, slug);
    match load_series_meta(&path) {
        Some(meta) => {
            if meta.installments.is_empty() {
                println!("{DIM}  회차가 없어 요약할 내용이 없습니다.{RESET}\n");
                return;
            }
            println!("{BOLD}  📖 시리즈 요약 생성을 위한 컨텍스트:{RESET}");
            println!("{DIM}  시리즈: {} ({}){RESET}", meta.title, meta.slug);
            for inst in &meta.installments {
                let story_info = match &inst.story_slug {
                    Some(s) => format!(" [story: {s}]"),
                    None => String::new(),
                };
                println!(
                    "{DIM}    {}회: {}{}{RESET}",
                    inst.number, inst.title, story_info
                );
            }
            println!(
                "\n{DIM}  위 회차 정보를 바탕으로 '이전 연재 요약'을 요청하세요.{RESET}\n"
            );
        }
        None => eprintln!("{RED}  시리즈를 찾을 수 없습니다: {slug}{RESET}\n"),
    }
}

fn handle_series_link_cmd(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /series link <시리즈slug> <story-slug>{RESET}\n");
        return;
    }

    let (series_s, story_s) = match args.split_once(char::is_whitespace) {
        Some((s, t)) => (s.trim(), t.trim()),
        None => {
            eprintln!("{RED}  사용법: /series link <시리즈slug> <story-slug>{RESET}\n");
            return;
        }
    };

    let base = series_dir();
    match link_story_to_series(series_s, story_s, &base) {
        Ok(num) => {
            println!(
                "{GREEN}  ✓ {series_s} {num}회에 story '{story_s}' 연결됨{RESET}\n"
            );
        }
        Err(e) => eprintln!("{RED}  {e}{RESET}\n"),
    }
}

#[cfg(test)]
mod series_tests {
    use super::*;

    #[test]
    fn series_slug_generation() {
        assert_eq!(series_slug("반도체 전쟁"), "반도체-전쟁");
        assert_eq!(series_slug("AI Revolution"), "ai-revolution");
        assert_eq!(series_slug(""), "");
    }

    #[test]
    fn series_subcommands_constant() {
        assert!(SERIES_SUBCOMMANDS.contains(&"new"));
        assert!(SERIES_SUBCOMMANDS.contains(&"list"));
        assert!(SERIES_SUBCOMMANDS.contains(&"add"));
        assert!(SERIES_SUBCOMMANDS.contains(&"status"));
        assert!(SERIES_SUBCOMMANDS.contains(&"recap"));
        assert!(SERIES_SUBCOMMANDS.contains(&"link"));
        assert_eq!(SERIES_SUBCOMMANDS.len(), 6);
    }

    #[test]
    fn create_series_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();

        let meta = create_series("반도체 전쟁 시리즈", base, "2026-03-28").unwrap();
        assert_eq!(meta.title, "반도체 전쟁 시리즈");
        assert_eq!(meta.slug, "반도체-전쟁-시리즈");
        assert_eq!(meta.status, "진행중");
        assert!(meta.installments.is_empty());

        // Verify file exists
        let path = base.join("반도체-전쟁-시리즈").join("series.json");
        assert!(path.exists());
    }

    #[test]
    fn create_series_empty_title() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = create_series("", dir.path(), "2026-03-28");
        assert!(result.is_err());
    }

    #[test]
    fn create_series_duplicate() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_series("테스트", base, "2026-03-28").unwrap();
        let result = create_series("테스트", base, "2026-03-28");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("이미 존재"));
    }

    #[test]
    fn add_installment_auto_numbering() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_series("연재물", base, "2026-03-28").unwrap();

        let n1 = add_series_installment("연재물", "첫 회", base, "2026-03-28").unwrap();
        assert_eq!(n1, 1);

        let n2 = add_series_installment("연재물", "둘째 회", base, "2026-03-29").unwrap();
        assert_eq!(n2, 2);

        let n3 = add_series_installment("연재물", "셋째 회", base, "2026-03-30").unwrap();
        assert_eq!(n3, 3);

        // Verify stored
        let path = base.join("연재물").join("series.json");
        let meta = load_series_meta(&path).unwrap();
        assert_eq!(meta.installments.len(), 3);
        assert_eq!(meta.installments[2].number, 3);
        assert_eq!(meta.installments[2].title, "셋째 회");
    }

    #[test]
    fn add_installment_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = add_series_installment("없는시리즈", "제목", dir.path(), "2026-03-28");
        assert!(result.is_err());
    }

    #[test]
    fn add_installment_empty_title() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_series("테스트", base, "2026-03-28").unwrap();
        let result = add_series_installment("테스트", "", base, "2026-03-28");
        assert!(result.is_err());
    }

    #[test]
    fn link_story_to_series_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_series("링크테스트", base, "2026-03-28").unwrap();
        add_series_installment("링크테스트", "1회", base, "2026-03-28").unwrap();
        add_series_installment("링크테스트", "2회", base, "2026-03-29").unwrap();

        // Link to first unlinked installment
        let num = link_story_to_series("링크테스트", "my-story", base).unwrap();
        assert_eq!(num, 2); // last unlinked (rposition finds 2nd)

        // Now first unlinked is installment 1
        let num2 = link_story_to_series("링크테스트", "other-story", base).unwrap();
        assert_eq!(num2, 1);
    }

    #[test]
    fn link_story_no_installments() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_series("빈시리즈", base, "2026-03-28").unwrap();
        let result = link_story_to_series("빈시리즈", "story", base);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("회차가 없습니다"));
    }

    #[test]
    fn list_series_empty_and_populated() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();

        assert!(list_series(base).is_empty());

        create_series("시리즈A", base, "2026-03-01").unwrap();
        create_series("시리즈B", base, "2026-03-15").unwrap();

        let all = list_series(base);
        assert_eq!(all.len(), 2);
        // sorted by created date
        assert_eq!(all[0].title, "시리즈A");
        assert_eq!(all[1].title, "시리즈B");
    }

    #[test]
    fn format_series_status_display() {
        let meta = SeriesMeta {
            title: "테스트 시리즈".to_string(),
            slug: "테스트-시리즈".to_string(),
            status: "진행중".to_string(),
            created: "2026-03-28".to_string(),
            installments: vec![
                SeriesInstallment {
                    number: 1,
                    title: "1회".to_string(),
                    added_date: "2026-03-28".to_string(),
                    story_slug: Some("story-1".to_string()),
                },
                SeriesInstallment {
                    number: 2,
                    title: "2회".to_string(),
                    added_date: "2026-03-29".to_string(),
                    story_slug: None,
                },
            ],
        };
        let status = format_series_status(&meta);
        assert!(status.contains("진행중"));
        assert!(status.contains("테스트 시리즈"));
        assert!(status.contains("2회"));
        assert!(status.contains("1개 연결"));
    }

    #[test]
    fn format_series_detail_display() {
        let meta = SeriesMeta {
            title: "AI 시대".to_string(),
            slug: "ai-시대".to_string(),
            status: "진행중".to_string(),
            created: "2026-03-28".to_string(),
            installments: vec![SeriesInstallment {
                number: 1,
                title: "서막".to_string(),
                added_date: "2026-03-28".to_string(),
                story_slug: Some("ai-story".to_string()),
            }],
        };
        let detail = format_series_detail(&meta);
        assert!(detail.contains("AI 시대"));
        assert!(detail.contains("1회: 서막"));
        assert!(detail.contains("→ ai-story"));
    }

    #[test]
    fn series_statuses_valid() {
        assert!(SERIES_STATUSES.contains(&"진행중"));
        assert!(SERIES_STATUSES.contains(&"휴재"));
        assert!(SERIES_STATUSES.contains(&"완결"));
        assert_eq!(SERIES_STATUSES.len(), 3);
    }

    #[test]
    fn series_json_roundtrip() {
        let meta = SeriesMeta {
            title: "테스트".to_string(),
            slug: "테스트".to_string(),
            status: "진행중".to_string(),
            created: "2026-03-28".to_string(),
            installments: vec![SeriesInstallment {
                number: 1,
                title: "1회".to_string(),
                added_date: "2026-03-28".to_string(),
                story_slug: None,
            }],
        };

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test").join("series.json");
        save_series_meta(&meta, &path).unwrap();

        let loaded = load_series_meta(&path).unwrap();
        assert_eq!(loaded.title, "테스트");
        assert_eq!(loaded.installments.len(), 1);
        assert_eq!(loaded.installments[0].number, 1);
    }
}
