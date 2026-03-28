//! Story project management command handlers (취재 프로젝트 도메인)
//! Commands: /story (new, add, list, show, status, review)

use crate::commands_project::*;
use crate::format::*;

// ── /story ──────────────────────────────────────────────────────────────

/// Base directory for story project workspaces.
pub const STORIES_DIR: &str = ".journalist/stories";

/// Subcommand names for `/story <Tab>` completion.
pub const STORY_SUBCOMMANDS: &[&str] = &["new", "add", "list", "show", "status", "review"];

/// Valid story statuses.
const STORY_STATUSES: &[&str] = &["취재중", "초고", "검증", "완료"];

/// Metadata for a story project.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoryMeta {
    pub title: String,
    pub slug: String,
    pub status: String,
    pub created: String,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Return the stories base directory path (configurable for testing).
fn stories_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(STORIES_DIR)
}

/// Build the path to a story's metadata file under a given base dir.
#[cfg(test)]
pub fn story_meta_path_at(base: &std::path::Path, slug: &str) -> std::path::PathBuf {
    base.join(slug).join("story.md")
}

/// Load story metadata from a story.md file.
pub fn load_story_meta(path: &std::path::Path) -> Option<StoryMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    parse_story_meta(&content)
}

/// Parse story metadata from story.md content.
pub fn parse_story_meta(content: &str) -> Option<StoryMeta> {
    // Format: YAML-like frontmatter between --- lines
    let content = content.trim();
    if !content.starts_with("---") {
        return None;
    }
    let after_first = &content[3..];
    let end_idx = after_first.find("---")?;
    let frontmatter = &after_first[..end_idx];

    let mut title = String::new();
    let mut slug = String::new();
    let mut status = String::from("취재중");
    let mut created = String::new();
    let mut notes = Vec::new();
    let mut in_notes = false;

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.starts_with("title:") {
            title = line.strip_prefix("title:")?.trim().to_string();
            in_notes = false;
        } else if line.starts_with("slug:") {
            slug = line.strip_prefix("slug:")?.trim().to_string();
            in_notes = false;
        } else if line.starts_with("status:") {
            status = line.strip_prefix("status:")?.trim().to_string();
            in_notes = false;
        } else if line.starts_with("created:") {
            created = line.strip_prefix("created:")?.trim().to_string();
            in_notes = false;
        } else if line.starts_with("notes:") {
            in_notes = true;
        } else if in_notes && line.starts_with("- ") {
            notes.push(line[2..].to_string());
        }
    }

    if title.is_empty() || slug.is_empty() {
        return None;
    }

    Some(StoryMeta {
        title,
        slug,
        status,
        created,
        notes,
    })
}

/// Serialize story metadata to story.md content.
pub fn serialize_story_meta(meta: &StoryMeta) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("title: {}\n", meta.title));
    out.push_str(&format!("slug: {}\n", meta.slug));
    out.push_str(&format!("status: {}\n", meta.status));
    out.push_str(&format!("created: {}\n", meta.created));
    if !meta.notes.is_empty() {
        out.push_str("notes:\n");
        for note in &meta.notes {
            out.push_str(&format!("- {note}\n"));
        }
    }
    out.push_str("---\n");
    out
}

/// Create a new story project workspace.
pub fn create_story(title: &str, base: &std::path::Path, date: &str) -> Result<StoryMeta, String> {
    if title.trim().is_empty() {
        return Err("스토리 제목을 입력하세요".to_string());
    }
    let slug = topic_to_slug(title, 50);
    if slug.is_empty() {
        return Err("유효한 제목을 입력하세요".to_string());
    }
    let story_dir = base.join(&slug);
    if story_dir.exists() {
        return Err(format!("이미 존재하는 스토리입니다: {slug}"));
    }
    std::fs::create_dir_all(&story_dir).map_err(|e| format!("디렉토리 생성 실패: {e}"))?;

    let meta = StoryMeta {
        title: title.to_string(),
        slug: slug.clone(),
        status: "취재중".to_string(),
        created: date.to_string(),
        notes: Vec::new(),
    };

    let meta_path = story_dir.join("story.md");
    let content = serialize_story_meta(&meta);
    std::fs::write(&meta_path, content).map_err(|e| format!("메타파일 저장 실패: {e}"))?;

    Ok(meta)
}

/// Add a note to a story.
pub fn add_story_note(slug: &str, note: &str, base: &std::path::Path) -> Result<(), String> {
    let meta_path = base.join(slug).join("story.md");
    let mut meta =
        load_story_meta(&meta_path).ok_or_else(|| format!("스토리를 찾을 수 없습니다: {slug}"))?;
    meta.notes.push(note.to_string());
    let content = serialize_story_meta(&meta);
    std::fs::write(&meta_path, content).map_err(|e| format!("저장 실패: {e}"))?;
    Ok(())
}

/// Change a story's status.
pub fn change_story_status(
    slug: &str,
    new_status: &str,
    base: &std::path::Path,
) -> Result<String, String> {
    if !STORY_STATUSES.contains(&new_status) {
        return Err(format!(
            "유효하지 않은 상태: {new_status}\n  사용 가능: {}",
            STORY_STATUSES.join(", ")
        ));
    }
    let meta_path = base.join(slug).join("story.md");
    let mut meta =
        load_story_meta(&meta_path).ok_or_else(|| format!("스토리를 찾을 수 없습니다: {slug}"))?;
    let old_status = meta.status.clone();
    meta.status = new_status.to_string();
    let content = serialize_story_meta(&meta);
    std::fs::write(&meta_path, content).map_err(|e| format!("저장 실패: {e}"))?;
    Ok(old_status)
}

/// List all stories under a base directory, grouped by status.
pub fn list_stories(base: &std::path::Path) -> Vec<StoryMeta> {
    let mut stories = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(e) => e,
        Err(_) => return stories,
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            let meta_path = entry.path().join("story.md");
            if let Some(meta) = load_story_meta(&meta_path) {
                stories.push(meta);
            }
        }
    }
    stories.sort_by(|a, b| a.created.cmp(&b.created));
    stories
}

/// Extract `--story <slug>` from an argument string.
/// Returns `(Option<slug>, remaining_args)`.
pub fn extract_story_arg(args: &str) -> (Option<String>, String) {
    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut story_slug: Option<String> = None;
    let mut remaining: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "--story" {
            if i + 1 < tokens.len() {
                story_slug = Some(tokens[i + 1].to_string());
                i += 2;
            } else {
                i += 1;
            }
        } else {
            remaining.push(tokens[i].to_string());
            i += 1;
        }
    }
    (story_slug, remaining.join(" "))
}

/// Link a file to a story project workspace.
/// Copies the file into `.journalist/stories/<slug>/` and adds a note to story.md.
pub fn link_file_to_story(
    slug: &str,
    source_path: &std::path::Path,
    label: &str,
    base: &std::path::Path,
) -> Result<(), String> {
    let story_dir = base.join(slug);
    let meta_path = story_dir.join("story.md");
    if !meta_path.exists() {
        return Err(format!("스토리를 찾을 수 없습니다: {slug}"));
    }
    let file_name = source_path
        .file_name()
        .ok_or_else(|| "파일명을 알 수 없습니다".to_string())?
        .to_string_lossy()
        .to_string();
    let dest = story_dir.join(&file_name);
    std::fs::copy(source_path, &dest).map_err(|e| format!("파일 복사 실패: {e}"))?;
    let note = format!("[{label}] {file_name}");
    add_story_note(slug, &note, base)?;
    Ok(())
}

/// Handle the `/story` command: journalist project workspace management.
pub fn handle_story(input: &str) {
    let args = input.strip_prefix("/story").unwrap_or("").trim();

    if args.is_empty() {
        handle_story_list_cmd(&stories_dir());
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "new" => handle_story_new_cmd(rest),
        "add" => handle_story_add_cmd(rest),
        "list" => handle_story_list_cmd(&stories_dir()),
        "show" => handle_story_show_cmd(rest),
        "status" => handle_story_status_cmd(rest),
        "review" => handle_story_review_cmd(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_story_usage();
        }
    }
}

fn print_story_usage() {
    println!("{DIM}  사용법:");
    println!("    /story new <제목>          새 취재 프로젝트 생성");
    println!("    /story add <메모>          활성 스토리에 메모 추가");
    println!("    /story list                스토리 목록 (상태별)");
    println!("    /story show [제목]         스토리 상세 표시");
    println!("    /story status <상태>       상태 변경 (취재중/초고/검증/완료)");
    println!("    /story review [제목]       취재 프로젝트 종합 리뷰");
    println!("    /story                     (list와 동일){RESET}\n");
}

fn handle_story_new_cmd(title: &str) {
    let base = stories_dir();
    let date = today_str();
    match create_story(title, &base, &date) {
        Ok(meta) => {
            println!(
                "{GREEN}  ✓ 스토리 생성: {}{RESET}",
                meta.title
            );
            println!(
                "{DIM}    경로: {}/{}{RESET}\n",
                STORIES_DIR, meta.slug
            );
        }
        Err(e) => eprintln!("{RED}  {e}{RESET}\n"),
    }
}

fn handle_story_add_cmd(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /story add <메모>{RESET}\n");
        return;
    }

    let base = stories_dir();
    // Find the most recent 취재중 story
    let stories = list_stories(&base);
    let active = stories.iter().rev().find(|s| s.status == "취재중");

    match active {
        Some(story) => {
            let slug = story.slug.clone();
            let title = story.title.clone();
            match add_story_note(&slug, args, &base) {
                Ok(()) => {
                    println!(
                        "{GREEN}  ✓ [{title}]에 메모 추가됨{RESET}\n"
                    );
                }
                Err(e) => eprintln!("{RED}  {e}{RESET}\n"),
            }
        }
        None => {
            eprintln!("{RED}  활성 스토리(취재중)가 없습니다. /story new <제목>으로 생성하세요{RESET}\n");
        }
    }
}

fn handle_story_list_cmd(base: &std::path::Path) {
    let stories = list_stories(base);
    if stories.is_empty() {
        println!("{DIM}  진행 중인 스토리가 없습니다.");
        println!("  /story new <제목>으로 시작하세요{RESET}\n");
        return;
    }

    // Group by status
    let mut by_status: std::collections::BTreeMap<String, Vec<&StoryMeta>> =
        std::collections::BTreeMap::new();
    for story in &stories {
        by_status
            .entry(story.status.clone())
            .or_default()
            .push(story);
    }

    // Display in priority order
    for status in STORY_STATUSES {
        if let Some(items) = by_status.get(*status) {
            println!("  {BOLD}[{status}]{RESET}");
            for s in items {
                let note_count = s.notes.len();
                println!(
                    "    {CYAN}{}{RESET}  ({}, 메모 {}건)",
                    s.title, s.created, note_count
                );
            }
        }
    }
    println!();
}

fn handle_story_show_cmd(args: &str) {
    let base = stories_dir();
    let stories = list_stories(&base);

    if stories.is_empty() {
        println!("{DIM}  스토리가 없습니다{RESET}\n");
        return;
    }

    let story = if args.is_empty() {
        // Show the most recent active story
        stories
            .iter()
            .rev()
            .find(|s| s.status == "취재중")
            .or_else(|| stories.last())
    } else {
        // Find by title match
        let query = args.to_lowercase();
        stories
            .iter()
            .find(|s| s.title.to_lowercase().contains(&query) || s.slug.contains(&query))
    };

    match story {
        Some(s) => {
            println!("  {BOLD}{}{RESET}", s.title);
            println!("  상태: {}  |  생성: {}", s.status, s.created);
            println!("  경로: {}/{}", STORIES_DIR, s.slug);
            if s.notes.is_empty() {
                println!("  {DIM}메모 없음{RESET}");
            } else {
                println!("  {DIM}── 메모 ({}건) ──{RESET}", s.notes.len());
                for (i, note) in s.notes.iter().enumerate() {
                    println!("  {}. {note}", i + 1);
                }
            }
            println!();
        }
        None => {
            if args.is_empty() {
                println!("{DIM}  활성 스토리가 없습니다{RESET}\n");
            } else {
                eprintln!("{RED}  스토리를 찾을 수 없습니다: {args}{RESET}\n");
            }
        }
    }
}

fn handle_story_status_cmd(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  사용법: /story status <상태>{RESET}");
        eprintln!(
            "{DIM}  사용 가능: {}{RESET}\n",
            STORY_STATUSES.join(", ")
        );
        return;
    }

    let base = stories_dir();
    let stories = list_stories(&base);
    let active = stories.iter().rev().find(|s| s.status != "완료");

    match active {
        Some(story) => {
            let slug = story.slug.clone();
            let title = story.title.clone();
            match change_story_status(&slug, args, &base) {
                Ok(old) => {
                    println!(
                        "{GREEN}  ✓ [{title}] {old} → {args}{RESET}\n"
                    );
                }
                Err(e) => eprintln!("{RED}  {e}{RESET}\n"),
            }
        }
        None => {
            eprintln!("{RED}  상태를 변경할 스토리가 없습니다{RESET}\n");
        }
    }
}

/// Artifact found in a story workspace directory.
#[derive(Debug, Clone)]
pub struct StoryArtifact {
    pub file_name: String,
    pub category: String,
    pub size_bytes: u64,
    pub preview: String,
}

/// Known reporting stage categories for completeness checking.
const REPORTING_STAGES: &[(&str, &str)] = &[
    ("research", "리서치/조사"),
    ("interview", "인터뷰/취재"),
    ("factcheck", "팩트체크/검증"),
    ("draft", "기사 초고"),
    ("source", "취재원 관리"),
    ("legal", "법적 검토"),
    ("rebuttal", "반론 취재"),
    ("data", "데이터/통계"),
    ("photo", "사진/미디어"),
    ("timeline", "타임라인"),
];

/// Categorize a file by its name into a reporting stage.
fn categorize_artifact(file_name: &str) -> String {
    let lower = file_name.to_lowercase();
    if lower == "story.md" {
        return "meta".to_string();
    }
    for &(key, label) in REPORTING_STAGES {
        if lower.contains(key) {
            return label.to_string();
        }
    }
    if lower.ends_with(".md") || lower.ends_with(".txt") {
        "메모/노트".to_string()
    } else if lower.ends_with(".json") || lower.ends_with(".csv") {
        "데이터/통계".to_string()
    } else if lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".gif")
    {
        "사진/미디어".to_string()
    } else {
        "기타".to_string()
    }
}

/// Collect all artifacts from a story workspace directory.
pub fn collect_story_artifacts(story_dir: &std::path::Path) -> Vec<StoryArtifact> {
    let mut artifacts = Vec::new();
    let entries = match std::fs::read_dir(story_dir) {
        Ok(e) => e,
        Err(_) => return artifacts,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let category = categorize_artifact(&file_name);
        if category == "meta" {
            continue;
        }
        let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let preview = if file_name.ends_with(".md") || file_name.ends_with(".txt") {
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect()
        } else {
            format!("[{category} 파일, {size_bytes} bytes]")
        };
        artifacts.push(StoryArtifact {
            file_name,
            category,
            size_bytes,
            preview,
        });
    }
    artifacts.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    artifacts
}

/// Build a comprehensive review prompt for a story project.
pub fn build_story_review_prompt(meta: &StoryMeta, artifacts: &[StoryArtifact]) -> String {
    let mut prompt = String::new();
    prompt.push_str("당신은 숙련된 데스크(편집자)입니다. 기자가 제출한 취재 프로젝트를 종합 리뷰해주세요.\n\n");

    prompt.push_str(&format!("## 취재 프로젝트: {}\n", meta.title));
    prompt.push_str(&format!("- 상태: {}\n", meta.status));
    prompt.push_str(&format!("- 생성일: {}\n", meta.created));
    if !meta.notes.is_empty() {
        prompt.push_str(&format!("- 메모: {}건\n", meta.notes.len()));
        for note in &meta.notes {
            prompt.push_str(&format!("  - {note}\n"));
        }
    }
    prompt.push('\n');

    prompt.push_str("## 워크스페이스 산출물\n\n");
    if artifacts.is_empty() {
        prompt.push_str("(산출물 없음 — 스토리 메타데이터만 존재)\n\n");
    } else {
        for artifact in artifacts {
            prompt.push_str(&format!(
                "### [{category}] {name} ({size} bytes)\n",
                category = artifact.category,
                name = artifact.file_name,
                size = artifact.size_bytes,
            ));
            prompt.push_str(&format!("{}\n\n", artifact.preview));
        }
    }

    // Checklist of reporting stages
    let found_categories: std::collections::HashSet<&str> =
        artifacts.iter().map(|a| a.category.as_str()).collect();
    prompt.push_str("## 취재 단계 체크리스트\n\n");
    for &(_key, label) in REPORTING_STAGES {
        let check = if found_categories.contains(label) {
            "✅"
        } else {
            "❌"
        };
        prompt.push_str(&format!("{check} {label}\n"));
    }
    prompt.push('\n');

    prompt.push_str(
        "## 리뷰 요청사항\n\n\
         위 취재 프로젝트를 종합적으로 검토하고 다음을 평가해주세요:\n\n\
         1. **완성도**: 기사 제출 전 빠진 취재 단계가 있는가?\n\
         2. **취재원 균형**: 다양한 시각이 반영되었는가? 반론 취재는 충분한가?\n\
         3. **팩트체크**: 주요 사실에 대한 검증이 이루어졌는가?\n\
         4. **법적 리스크**: 명예훼손, 저작권, 개인정보 등 법적 검토가 필요한 부분이 있는가?\n\
         5. **보완 제안**: 기사 품질을 높이기 위해 추가로 필요한 취재나 자료는?\n\n\
         한국어로 답변해주세요.\n",
    );

    prompt
}

fn handle_story_review_cmd(args: &str) {
    let base = stories_dir();
    let stories = list_stories(&base);

    if stories.is_empty() {
        println!("{DIM}  스토리가 없습니다{RESET}\n");
        return;
    }

    let story = if args.is_empty() {
        stories
            .iter()
            .rev()
            .find(|s| s.status != "완료")
            .or_else(|| stories.last())
    } else {
        let query = args.to_lowercase();
        stories
            .iter()
            .find(|s| s.title.to_lowercase().contains(&query) || s.slug.contains(&query))
    };

    match story {
        Some(s) => {
            let story_dir = base.join(&s.slug);
            let artifacts = collect_story_artifacts(&story_dir);

            println!("  {BOLD}📋 [{title}] 종합 리뷰{RESET}", title = s.title);
            println!(
                "  산출물: {}건 | 상태: {}",
                artifacts.len(),
                s.status
            );

            // Show checklist
            let found_categories: std::collections::HashSet<&str> =
                artifacts.iter().map(|a| a.category.as_str()).collect();
            let total = REPORTING_STAGES.len();
            let covered = REPORTING_STAGES
                .iter()
                .filter(|&&(_, label)| found_categories.contains(label))
                .count();
            println!("  취재 단계: {covered}/{total} 완료\n");

            let prompt = build_story_review_prompt(s, &artifacts);
            println!("{DIM}  [AI에게 리뷰 프롬프트 전송 — {len}자]{RESET}\n", len = prompt.len());
        }
        None => {
            if args.is_empty() {
                println!("{DIM}  리뷰할 스토리가 없습니다{RESET}\n");
            } else {
                eprintln!("{RED}  스토리를 찾을 수 없습니다: {args}{RESET}\n");
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_create_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        let meta = create_story("반도체 수출 동향", base, "2026-03-25").unwrap();
        assert_eq!(meta.title, "반도체 수출 동향");
        assert_eq!(meta.slug, "반도체-수출-동향");
        assert_eq!(meta.status, "취재중");
        assert_eq!(meta.created, "2026-03-25");
        assert!(meta.notes.is_empty());
        // Verify file exists
        let meta_path = base.join("반도체-수출-동향").join("story.md");
        assert!(meta_path.exists());
    }

    #[test]
    fn story_create_duplicate_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_story("테스트", base, "2026-03-25").unwrap();
        let result = create_story("테스트", base, "2026-03-25");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("이미 존재"));
    }

    #[test]
    fn story_create_empty_title_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = create_story("", dir.path(), "2026-03-25");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("제목"));
    }

    #[test]
    fn story_slug_generation() {
        let dir = tempfile::TempDir::new().unwrap();
        let meta = create_story("대통령실 인사 발표", dir.path(), "2026-03-25").unwrap();
        assert_eq!(meta.slug, "대통령실-인사-발표");
    }

    #[test]
    fn story_add_note() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        let meta = create_story("테스트", base, "2026-03-25").unwrap();
        add_story_note(&meta.slug, "첫 번째 메모", base).unwrap();
        let loaded = load_story_meta(&story_meta_path_at(base, &meta.slug)).unwrap();
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0], "첫 번째 메모");
    }

    #[test]
    fn story_add_note_nonexistent() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = add_story_note("없는스토리", "메모", dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn story_change_status() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        let meta = create_story("테스트", base, "2026-03-25").unwrap();
        let old = change_story_status(&meta.slug, "초고", base).unwrap();
        assert_eq!(old, "취재중");
        let loaded = load_story_meta(&story_meta_path_at(base, &meta.slug)).unwrap();
        assert_eq!(loaded.status, "초고");
    }

    #[test]
    fn story_change_status_invalid() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        let meta = create_story("테스트", base, "2026-03-25").unwrap();
        let result = change_story_status(&meta.slug, "잘못된상태", base);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("유효하지 않은"));
    }

    #[test]
    fn story_list_sorted() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        create_story("첫째", base, "2026-03-20").unwrap();
        create_story("둘째", base, "2026-03-21").unwrap();
        let stories = list_stories(base);
        assert_eq!(stories.len(), 2);
        assert_eq!(stories[0].title, "첫째");
    }

    #[test]
    fn story_list_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let stories = list_stories(dir.path());
        assert!(stories.is_empty());
    }

    #[test]
    fn story_meta_parse_roundtrip() {
        let meta = StoryMeta {
            title: "테스트 취재".to_string(),
            slug: "테스트-취재".to_string(),
            status: "취재중".to_string(),
            created: "2026-03-25".to_string(),
            notes: vec!["메모1".to_string(), "메모2".to_string()],
        };
        let serialized = serialize_story_meta(&meta);
        let parsed = parse_story_meta(&serialized).unwrap();
        assert_eq!(parsed.title, meta.title);
        assert_eq!(parsed.slug, meta.slug);
        assert_eq!(parsed.status, meta.status);
        assert_eq!(parsed.created, meta.created);
        assert_eq!(parsed.notes, meta.notes);
    }

    #[test]
    fn story_meta_parse_no_notes() {
        let content = "---\ntitle: 테스트\nslug: 테스트\nstatus: 취재중\ncreated: 2026-03-25\n---\n";
        let meta = parse_story_meta(content).unwrap();
        assert_eq!(meta.title, "테스트");
        assert!(meta.notes.is_empty());
    }

    #[test]
    fn story_meta_parse_invalid() {
        assert!(parse_story_meta("not frontmatter").is_none());
        assert!(parse_story_meta("---\n---\n").is_none()); // empty frontmatter
        assert!(parse_story_meta("---\nslug: only-slug\n---\n").is_none()); // no title
    }

    #[test]
    fn categorize_artifact_research_file() {
        assert_eq!(categorize_artifact("research-notes.md"), "리서치/조사");
        assert_eq!(categorize_artifact("Research_memo.txt"), "리서치/조사");
    }

    #[test]
    fn categorize_artifact_interview_file() {
        assert_eq!(categorize_artifact("interview-김철수.md"), "인터뷰/취재");
    }

    #[test]
    fn categorize_artifact_factcheck_file() {
        assert_eq!(categorize_artifact("factcheck-결과.md"), "팩트체크/검증");
    }

    #[test]
    fn categorize_artifact_draft_file() {
        assert_eq!(categorize_artifact("draft-v1.md"), "기사 초고");
    }

    #[test]
    fn categorize_artifact_legal_file() {
        assert_eq!(categorize_artifact("legal-review.md"), "법적 검토");
    }

    #[test]
    fn categorize_artifact_data_by_extension() {
        assert_eq!(categorize_artifact("stats.json"), "데이터/통계");
        assert_eq!(categorize_artifact("export.csv"), "데이터/통계");
    }

    #[test]
    fn categorize_artifact_image_files() {
        assert_eq!(categorize_artifact("photo.jpg"), "사진/미디어");
        assert_eq!(categorize_artifact("scene.png"), "사진/미디어");
    }

    #[test]
    fn categorize_artifact_generic_text() {
        assert_eq!(categorize_artifact("memo.md"), "메모/노트");
        assert_eq!(categorize_artifact("notes.txt"), "메모/노트");
    }

    #[test]
    fn categorize_artifact_unknown() {
        assert_eq!(categorize_artifact("binary.bin"), "기타");
    }

    #[test]
    fn categorize_artifact_story_md_is_meta() {
        assert_eq!(categorize_artifact("story.md"), "meta");
    }

    #[test]
    fn collect_story_artifacts_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let artifacts = collect_story_artifacts(dir.path());
        assert!(artifacts.is_empty());
    }

    #[test]
    fn collect_story_artifacts_skips_story_md() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("story.md"), "---\ntitle: X\n---").unwrap();
        let artifacts = collect_story_artifacts(dir.path());
        assert!(artifacts.is_empty());
    }

    #[test]
    fn collect_story_artifacts_preview_text_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("notes.md"), "hello world").unwrap();
        let artifacts = collect_story_artifacts(dir.path());
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].preview, "hello world");
    }

    #[test]
    fn collect_story_artifacts_sorted_by_name() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("b.md"), "b").unwrap();
        std::fs::write(dir.path().join("a.md"), "a").unwrap();
        let artifacts = collect_story_artifacts(dir.path());
        assert_eq!(artifacts[0].file_name, "a.md");
        assert_eq!(artifacts[1].file_name, "b.md");
    }

    #[test]
    fn collect_story_artifacts_nonexistent_dir() {
        let artifacts = collect_story_artifacts(std::path::Path::new("/nonexistent/path"));
        assert!(artifacts.is_empty());
    }

    #[test]
    fn build_story_review_prompt_contains_title() {
        let meta = StoryMeta {
            title: "반도체 수출 동향".to_string(),
            slug: "반도체-수출-동향".to_string(),
            status: "취재중".to_string(),
            created: "2026-03-25".to_string(),
            notes: vec!["메모1".to_string()],
        };
        let prompt = build_story_review_prompt(&meta, &[]);
        assert!(prompt.contains("반도체 수출 동향"));
        assert!(prompt.contains("취재중"));
        assert!(prompt.contains("2026-03-25"));
        assert!(prompt.contains("메모1"));
    }

    #[test]
    fn build_story_review_prompt_empty_artifacts() {
        let meta = StoryMeta {
            title: "테스트".to_string(),
            slug: "테스트".to_string(),
            status: "초고".to_string(),
            created: "2026-03-25".to_string(),
            notes: vec![],
        };
        let prompt = build_story_review_prompt(&meta, &[]);
        assert!(prompt.contains("산출물 없음"));
        // All stages should be unchecked
        for &(_, label) in REPORTING_STAGES {
            assert!(prompt.contains(&format!("❌ {label}")));
        }
    }

    #[test]
    fn build_story_review_prompt_with_artifacts() {
        let meta = StoryMeta {
            title: "테스트".to_string(),
            slug: "테스트".to_string(),
            status: "취재중".to_string(),
            created: "2026-03-25".to_string(),
            notes: vec![],
        };
        let artifacts = vec![
            StoryArtifact {
                file_name: "research-notes.md".to_string(),
                category: "리서치/조사".to_string(),
                size_bytes: 100,
                preview: "리서치 내용 미리보기".to_string(),
            },
            StoryArtifact {
                file_name: "interview-김철수.md".to_string(),
                category: "인터뷰/취재".to_string(),
                size_bytes: 200,
                preview: "인터뷰 내용".to_string(),
            },
        ];
        let prompt = build_story_review_prompt(&meta, &artifacts);
        assert!(prompt.contains("✅ 리서치/조사"));
        assert!(prompt.contains("✅ 인터뷰/취재"));
        assert!(prompt.contains("❌ 팩트체크/검증"));
        assert!(prompt.contains("❌ 반론 취재"));
        assert!(prompt.contains("리서치 내용 미리보기"));
        assert!(prompt.contains("인터뷰 내용"));
    }

    #[test]
    fn build_story_review_prompt_review_sections() {
        let meta = StoryMeta {
            title: "테스트".to_string(),
            slug: "테스트".to_string(),
            status: "검증".to_string(),
            created: "2026-03-25".to_string(),
            notes: vec![],
        };
        let prompt = build_story_review_prompt(&meta, &[]);
        assert!(prompt.contains("완성도"));
        assert!(prompt.contains("취재원 균형"));
        assert!(prompt.contains("팩트체크"));
        assert!(prompt.contains("법적 리스크"));
        assert!(prompt.contains("보완 제안"));
    }

    #[test]
    fn extract_story_arg_basic() {
        let (slug, rest) = extract_story_arg("--story my-slug 반도체 수출");
        assert_eq!(slug, Some("my-slug".to_string()));
        assert_eq!(rest, "반도체 수출");
    }

    #[test]
    fn extract_story_arg_at_end() {
        let (slug, rest) = extract_story_arg("반도체 수출 --story my-slug");
        assert_eq!(slug, Some("my-slug".to_string()));
        assert_eq!(rest, "반도체 수출");
    }

    #[test]
    fn extract_story_arg_missing() {
        let (slug, rest) = extract_story_arg("반도체 수출");
        assert!(slug.is_none());
        assert_eq!(rest, "반도체 수출");
    }

    #[test]
    fn extract_story_arg_no_value() {
        let (slug, rest) = extract_story_arg("--story");
        assert!(slug.is_none());
        assert!(rest.is_empty());
    }

    #[test]
    fn link_file_to_story_copies_and_notes() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        let meta = create_story("테스트 취재", base, "2026-03-25").unwrap();

        // Create a source file to link
        let src_dir = dir.path().join("research");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("2026-03-25_test.md");
        std::fs::write(&src_file, "# 리서치 내용").unwrap();

        link_file_to_story(&meta.slug, &src_file, "리서치", base).unwrap();

        // Verify file was copied
        let copied = base.join(&meta.slug).join("2026-03-25_test.md");
        assert!(copied.exists());
        assert_eq!(std::fs::read_to_string(&copied).unwrap(), "# 리서치 내용");

        // Verify note was added
        let loaded = load_story_meta(&story_meta_path_at(base, &meta.slug)).unwrap();
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.notes[0], "[리서치] 2026-03-25_test.md");
    }

    #[test]
    fn link_file_to_story_nonexistent_slug() {
        let dir = tempfile::TempDir::new().unwrap();
        let src_file = dir.path().join("test.md");
        std::fs::write(&src_file, "test").unwrap();
        let result = link_file_to_story("없는스토리", &src_file, "리서치", dir.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("찾을 수 없습니다"));
    }

    #[test]
    fn link_file_to_story_interview_label() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path();
        let meta = create_story("인터뷰 테스트", base, "2026-03-26").unwrap();

        let src_dir = dir.path().join("interview");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src_file = src_dir.join("2026-03-26_interview.md");
        std::fs::write(&src_file, "# 인터뷰 질문지").unwrap();

        link_file_to_story(&meta.slug, &src_file, "인터뷰", base).unwrap();

        let copied = base.join(&meta.slug).join("2026-03-26_interview.md");
        assert!(copied.exists());

        let loaded = load_story_meta(&story_meta_path_at(base, &meta.slug)).unwrap();
        assert_eq!(loaded.notes.len(), 1);
        assert!(loaded.notes[0].contains("[인터뷰]"));
    }
}
