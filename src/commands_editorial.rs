//! Editorial management command handlers (에디토리얼 관리 도메인)
//! Commands: /desk, /collaborate, /coverage
//! Extracted from commands_workflow.rs — organizational commands distinct from content-production workflow.

use crate::commands_writing::format_unix_timestamp;
use crate::format::*;

// ── /desk ────────────────────────────────────────────────────────────────

const DESK_ASSIGNMENTS_FILE: &str = ".journalist/desk/assignments.json";

/// Status of a desk assignment.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeskStatus {
    Pending,
    Done,
}

/// A single desk assignment (데스크 → 기자 업무 지시).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DeskAssignment {
    pub reporter: String,
    pub content: String,
    pub deadline: Option<String>,
    pub status: DeskStatus,
    pub feedback: Vec<String>,
    /// true if this was a reporter pitch rather than a desk assignment
    #[serde(default)]
    pub is_pitch: bool,
    pub created_at: String,
}

pub fn desk_path() -> std::path::PathBuf {
    std::path::PathBuf::from(DESK_ASSIGNMENTS_FILE)
}

pub fn load_desk_from(path: &std::path::Path) -> Vec<DeskAssignment> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("⚠ 데스크 배정 데이터 파싱 실패 ({}): {e}", path.display());
                Vec::new()
            }
        },
        _ => Vec::new(),
    }
}

pub fn save_desk_to(assignments: &[DeskAssignment], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(assignments) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                eprintln!("⚠ 데스크 배정 데이터 저장 실패 ({}): {e}", path.display());
            }
        }
        Err(e) => eprintln!("⚠ 데스크 배정 데이터 직렬화 실패: {e}"),
    }
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
pub fn is_valid_time(s: &str) -> bool {
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

pub const COLLABORATE_DIR: &str = ".journalist/collaborate";

/// A collaborative reporting project.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CollabProject {
    pub name: String,
    pub reporters: Vec<String>,
    pub notes: Vec<CollabNote>,
    pub status: CollabStatus,
    pub created_at: String,
}

/// A single note within a collaborative project.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CollabNote {
    pub reporter: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CollabStatus {
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

pub fn load_collab_project_from(path: &std::path::Path) -> Option<CollabProject> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).ok(),
        _ => None,
    }
}

pub fn save_collab_project_to(project: &CollabProject, path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(project) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                eprintln!("⚠ 협업 프로젝트 저장 실패 ({}): {e}", path.display());
            }
        }
        Err(e) => eprintln!("⚠ 협업 프로젝트 직렬화 실패: {e}"),
    }
}

pub fn list_collab_projects_in(dir: &std::path::Path) -> Vec<CollabProject> {
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

pub fn now_timestamp() -> String {
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
pub struct CoverageClaim {
    pub topic: String,
    pub reporter: String,
    /// Optional expiry time in "HH:MM" format.
    pub until: Option<String>,
    pub active: bool,
    pub created_at: String,
}

pub fn coverage_path() -> std::path::PathBuf {
    std::path::PathBuf::from(COVERAGE_FILE)
}

pub fn load_coverage_from(path: &std::path::Path) -> Vec<CoverageClaim> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("⚠ 취재 배정 데이터 파싱 실패 ({}): {e}", path.display());
                Vec::new()
            }
        },
        _ => Vec::new(),
    }
}

pub fn save_coverage_to(claims: &[CoverageClaim], path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(claims) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                eprintln!("⚠ 취재 배정 데이터 저장 실패 ({}): {e}", path.display());
            }
        }
        Err(e) => eprintln!("⚠ 취재 배정 데이터 직렬화 실패: {e}"),
    }
}

/// Check if a claim has expired based on its `until` time and current HH:MM.
pub fn is_claim_expired(claim: &CoverageClaim, now_hhmm: &str) -> bool {
    match &claim.until {
        Some(until) => now_hhmm >= until.as_str(),
        None => false,
    }
}

/// Get current time as "HH:MM" (UTC).
pub fn current_hhmm() -> String {
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
pub fn expire_claims(claims: &mut [CoverageClaim], now_hhmm: &str) -> usize {
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
pub fn time_diff_minutes(target: &str, now: &str) -> Option<i32> {
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    // ── /desk tests ─────────────────────────────────────────────────────

    #[test]
    fn desk_save_and_load() {
        let (_dir, path) = temp_desk_path();
        let assignments = vec![
            DeskAssignment {
                reporter: "김기자".to_string(),
                content: "국회 취재".to_string(),
                deadline: Some("15:00".to_string()),
                status: DeskStatus::Pending,
                feedback: vec![],
                is_pitch: false,
                created_at: "2026-03-20T10:00:00".to_string(),
            },
            DeskAssignment {
                reporter: "이기자".to_string(),
                content: "반도체 취재".to_string(),
                deadline: None,
                status: DeskStatus::Done,
                feedback: vec!["잘했습니다".to_string()],
                is_pitch: false,
                created_at: "2026-03-20T09:00:00".to_string(),
            },
        ];
        save_desk_to(&assignments, &path);

        let loaded = load_desk_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].reporter, "김기자");
        assert_eq!(loaded[0].status, DeskStatus::Pending);
        assert_eq!(loaded[0].deadline, Some("15:00".to_string()));
        assert_eq!(loaded[1].status, DeskStatus::Done);
        assert_eq!(loaded[1].feedback.len(), 1);
    }

    #[test]
    fn desk_load_missing_file() {
        let path = std::path::PathBuf::from("/tmp/nonexistent_desk_test.json");
        let loaded = load_desk_from(&path);
        assert!(loaded.is_empty());
    }

    #[test]
    fn desk_done_status() {
        let (_dir, path) = temp_desk_path();

        let assignment = DeskAssignment {
            reporter: "박기자".to_string(),
            content: "경제 취재".to_string(),
            deadline: None,
            status: DeskStatus::Pending,
            feedback: vec![],
            is_pitch: false,
            created_at: "2026-03-20T10:00:00".to_string(),
        };
        save_desk_to(&[assignment], &path);

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
            reporter: "김기자".to_string(),
            content: "기사 수정".to_string(),
            deadline: None,
            status: DeskStatus::Pending,
            feedback: vec![],
            is_pitch: false,
            created_at: "2026-03-20T10:00:00".to_string(),
        };
        save_desk_to(&[assignment], &path);

        let mut loaded = load_desk_from(&path);
        loaded[0].feedback.push("리드 수정 필요".to_string());
        loaded[0].feedback.push("사진 추가해주세요".to_string());
        save_desk_to(&loaded, &path);

        let reloaded = load_desk_from(&path);
        assert_eq!(reloaded[0].feedback.len(), 2);
    }

    #[test]
    fn desk_pitch_flag() {
        let (_dir, path) = temp_desk_path();

        let pitch = DeskAssignment {
            reporter: "제안".to_string(),
            content: "[반도체] 삼성전자 실적 분석".to_string(),
            deadline: None,
            status: DeskStatus::Pending,
            feedback: vec![],
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
    fn parse_desk_assign_args_deadline_empty_value() {
        let result = parse_desk_assign_args("김기자 반도체 취재 --deadline");
        assert!(result.is_some());
        let (reporter, content, deadline) = result.unwrap();
        assert_eq!(reporter, "김기자");
        assert_eq!(content, "반도체 취재");
        assert!(deadline.is_none());
    }

    #[test]
    fn parse_desk_assign_args_empty() {
        let result = parse_desk_assign_args("");
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

    // ── /collaborate tests ──────────────────────────────────────────────

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

    // ── /coverage tests ─────────────────────────────────────────────────

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
        assert_eq!(expired, 1);
        assert!(!claims[0].active);
        assert!(claims[1].active);
        assert!(claims[2].active);
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
                active: false,
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
    fn time_diff_minutes_invalid_input() {
        assert!(time_diff_minutes("invalid", "14:00").is_none());
        assert!(time_diff_minutes("14:00", "bad").is_none());
        assert!(time_diff_minutes("", "").is_none());
    }

    #[test]
    fn time_diff_minutes_zero_diff() {
        assert_eq!(time_diff_minutes("14:00", "14:00"), Some(0));
    }
}
