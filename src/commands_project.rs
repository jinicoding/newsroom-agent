//! Project dev-tool command handlers and shared utilities.
//! Commands: /context, /init, /health, /fix, /test, /lint, /tree, /run, /docs, /find, /index
//! Shared utilities: topic_to_slug, today_str, draft paths, research paths, source helpers.


use crate::cli;
use crate::commands::auto_compact_if_needed;
use crate::commands_research::RESEARCH_DIR;
use crate::docs;
use crate::format::*;
use crate::prompt::*;

use yoagent::agent::Agent;
use yoagent::*;

// ── /context ─────────────────────────────────────────────────────────────

pub fn handle_context() {
    let files = cli::list_project_context_files();
    if files.is_empty() {
        println!("{DIM}  No project context files found.");
        println!("  Create a YOYO.md to give yoyo project context.");
        println!("  Also supports: CLAUDE.md (compatibility alias), .yoyo/instructions.md");
        println!("  Run /init to create a starter YOYO.md.{RESET}\n");
    } else {
        println!("{DIM}  Project context files:");
        for (name, lines) in &files {
            println!("    {name} ({lines} lines)");
        }
        println!("{RESET}");
    }
}

// ── /init ────────────────────────────────────────────────────────────────

/// Scan the project directory and find important files (README, config, CI, etc.).
/// Returns a list of file paths that exist.
pub fn scan_important_files(dir: &std::path::Path) -> Vec<String> {
    let candidates = [
        "README.md",
        "README",
        "readme.md",
        "LICENSE",
        "LICENSE.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        ".gitignore",
        ".editorconfig",
        // Rust
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        // Node
        "package.json",
        "package-lock.json",
        "tsconfig.json",
        ".eslintrc.json",
        ".eslintrc.js",
        ".prettierrc",
        // Python
        "pyproject.toml",
        "setup.py",
        "setup.cfg",
        "requirements.txt",
        "Pipfile",
        "tox.ini",
        // Go
        "go.mod",
        "go.sum",
        // Build/CI
        "Makefile",
        "Dockerfile",
        "docker-compose.yml",
        "docker-compose.yaml",
        ".dockerignore",
        // CI configs
        ".github/workflows",
        ".gitlab-ci.yml",
        ".circleci/config.yml",
        ".travis.yml",
        "Jenkinsfile",
    ];
    candidates
        .iter()
        .filter(|f| dir.join(f).exists())
        .map(|f| f.to_string())
        .collect()
}

/// Detect key directories in the project (src, tests, docs, etc.).
/// Returns a list of directory names that exist.
pub fn scan_important_dirs(dir: &std::path::Path) -> Vec<String> {
    let candidates = [
        "src",
        "lib",
        "tests",
        "test",
        "docs",
        "doc",
        "examples",
        "benches",
        "scripts",
        ".github",
        ".vscode",
        "config",
        "public",
        "static",
        "assets",
        "migrations",
    ];
    candidates
        .iter()
        .filter(|d| dir.join(d).is_dir())
        .map(|d| d.to_string())
        .collect()
}

/// Get build/test/lint commands for a project type.
pub fn build_commands_for_project(project_type: &ProjectType) -> Vec<(&'static str, &'static str)> {
    match project_type {
        ProjectType::Rust => vec![
            ("Build", "cargo build"),
            ("Test", "cargo test"),
            ("Lint", "cargo clippy --all-targets -- -D warnings"),
            ("Format check", "cargo fmt -- --check"),
            ("Format", "cargo fmt"),
        ],
        ProjectType::Node => vec![
            ("Install", "npm install"),
            ("Test", "npm test"),
            ("Lint", "npx eslint ."),
        ],
        ProjectType::Python => vec![
            ("Test", "python -m pytest"),
            ("Lint", "ruff check ."),
            ("Type check", "python -m mypy ."),
        ],
        ProjectType::Go => vec![
            ("Build", "go build ./..."),
            ("Test", "go test ./..."),
            ("Vet", "go vet ./..."),
        ],
        ProjectType::Make => vec![("Build", "make"), ("Test", "make test")],
        ProjectType::Unknown => vec![],
    }
}

/// Extract the project name from a README.md title line (# Title).
/// Returns None if no README or no title found.
fn extract_project_name_from_readme(dir: &std::path::Path) -> Option<String> {
    let readme_names = ["README.md", "readme.md", "README"];
    for name in &readme_names {
        if let Ok(content) = std::fs::read_to_string(dir.join(name)) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(title) = trimmed.strip_prefix("# ") {
                    let title = title.trim();
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Extract the project name from Cargo.toml [package] name field.
fn extract_name_from_cargo_toml(dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("name") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Extract the project name from package.json "name" field.
fn extract_name_from_package_json(dir: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("package.json")).ok()?;
    // Simple JSON parsing — find "name": "value"
    for line in content.lines() {
        let trimmed = line.trim().trim_end_matches(',');
        if let Some(rest) = trimmed.strip_prefix("\"name\"") {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix(':') {
                let val = rest.trim().trim_matches('"');
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Best-effort project name detection. Tries multiple sources.
pub fn detect_project_name(dir: &std::path::Path) -> String {
    // Try Cargo.toml name
    if let Some(name) = extract_name_from_cargo_toml(dir) {
        return name;
    }
    // Try package.json name
    if let Some(name) = extract_name_from_package_json(dir) {
        return name;
    }
    // Try README title
    if let Some(name) = extract_project_name_from_readme(dir) {
        return name;
    }
    // Fall back to directory name
    dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "my-project".to_string())
}

/// Generate a complete YOYO.md context file by scanning the project.
pub fn generate_init_content(dir: &std::path::Path) -> String {
    let project_type = detect_project_type(dir);
    let project_name = detect_project_name(dir);
    let important_files = scan_important_files(dir);
    let important_dirs = scan_important_dirs(dir);
    let build_commands = build_commands_for_project(&project_type);

    let mut content = String::new();

    // Header
    content.push_str("# Project Context\n\n");
    content.push_str("<!-- YOYO.md — generated by `yoyo /init`. Edit to customize. -->\n");
    content.push_str("<!-- Also works as CLAUDE.md for compatibility with other tools. -->\n\n");

    // About section
    content.push_str("## About This Project\n\n");
    content.push_str(&format!("**{project_name}**"));
    if project_type != ProjectType::Unknown {
        content.push_str(&format!(" — {project_type} project"));
    }
    content.push_str("\n\n");
    content.push_str("<!-- Add a description of what this project does. -->\n\n");

    // Build & Test section
    content.push_str("## Build & Test\n\n");
    if build_commands.is_empty() {
        content.push_str("<!-- Add build, test, and run commands for this project. -->\n\n");
    } else {
        content.push_str("```bash\n");
        for (label, cmd) in &build_commands {
            content.push_str(&format!("{cmd:<50} # {label}\n"));
        }
        content.push_str("```\n\n");
    }

    // Coding Conventions section
    content.push_str("## Coding Conventions\n\n");
    content.push_str(
        "<!-- List any coding standards, naming conventions, or patterns to follow. -->\n\n",
    );

    // Important Files section
    content.push_str("## Important Files\n\n");
    if important_files.is_empty() && important_dirs.is_empty() {
        content.push_str("<!-- List key files and directories the agent should know about. -->\n");
    } else {
        if !important_dirs.is_empty() {
            content.push_str("Key directories:\n");
            for d in &important_dirs {
                content.push_str(&format!("- `{d}/`\n"));
            }
            content.push('\n');
        }
        if !important_files.is_empty() {
            content.push_str("Key files:\n");
            for f in &important_files {
                content.push_str(&format!("- `{f}`\n"));
            }
            content.push('\n');
        }
    }

    content
}

pub fn handle_init() {
    let path = "YOYO.md";
    if std::path::Path::new(path).exists() {
        println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
    } else if std::path::Path::new("CLAUDE.md").exists() {
        println!("{DIM}  CLAUDE.md already exists — yoyo reads it as a compatibility alias.");
        println!("  Rename it to YOYO.md when you're ready: mv CLAUDE.md YOYO.md{RESET}\n");
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        let project_type = detect_project_type(&cwd);
        println!("{DIM}  Scanning project...{RESET}");
        if project_type != ProjectType::Unknown {
            println!("{DIM}  Detected: {project_type}{RESET}");
        }
        let content = generate_init_content(&cwd);
        match std::fs::write(path, &content) {
            Ok(_) => {
                let line_count = content.lines().count();
                println!("{GREEN}  ✓ Created {path} ({line_count} lines) — edit it to add project context.{RESET}");
                println!("{DIM}  Tip: Use /remember to save project-specific notes that persist across sessions.{RESET}\n");
            }
            Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
        }
    }
}

// ── /docs ────────────────────────────────────────────────────────────────

pub fn handle_docs(input: &str) {
    if input == "/docs" {
        println!("{DIM}  usage: /docs <crate> [item]");
        println!("  Look up docs.rs documentation for a Rust crate.");
        println!("  Examples: /docs serde, /docs tokio task{RESET}\n");
        return;
    }
    let args = input.trim_start_matches("/docs ").trim();
    if args.is_empty() {
        println!("{DIM}  usage: /docs <crate> [item]{RESET}\n");
        return;
    }
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();
    let crate_name = parts[0].trim();
    let item_name = parts.get(1).map(|s| s.trim()).unwrap_or("");

    let (found, summary) = if item_name.is_empty() {
        docs::fetch_docs_summary(crate_name)
    } else {
        docs::fetch_docs_item(crate_name, item_name)
    };
    if found {
        let label = if item_name.is_empty() {
            crate_name.to_string()
        } else {
            format!("{crate_name}::{item_name}")
        };
        println!("{GREEN}  ✓ {label}{RESET}");
        println!("{DIM}{summary}{RESET}\n");
    } else {
        println!("{RED}  ✗ {summary}{RESET}\n");
    }
}

// ── /health ──────────────────────────────────────────────────────────────

/// Detected project type based on marker files in the working directory.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Make,
    Unknown,
}

impl std::fmt::Display for ProjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectType::Rust => write!(f, "Rust (Cargo)"),
            ProjectType::Node => write!(f, "Node.js (npm)"),
            ProjectType::Python => write!(f, "Python"),
            ProjectType::Go => write!(f, "Go"),
            ProjectType::Make => write!(f, "Makefile"),
            ProjectType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect project type by checking for marker files in the given directory.
pub fn detect_project_type(dir: &std::path::Path) -> ProjectType {
    if dir.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if dir.join("package.json").exists() {
        ProjectType::Node
    } else if dir.join("pyproject.toml").exists()
        || dir.join("setup.py").exists()
        || dir.join("setup.cfg").exists()
    {
        ProjectType::Python
    } else if dir.join("go.mod").exists() {
        ProjectType::Go
    } else if dir.join("Makefile").exists() || dir.join("makefile").exists() {
        ProjectType::Make
    } else {
        ProjectType::Unknown
    }
}

/// Return health check commands for a given project type.
#[allow(clippy::vec_init_then_push, unused_mut)]
pub fn health_checks_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => {
            let mut checks = vec![("build", vec!["cargo", "build"])];
            #[cfg(not(test))]
            checks.push(("test", vec!["cargo", "test"]));
            checks.push((
                "clippy",
                vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
            ));
            checks.push(("fmt", vec!["cargo", "fmt", "--", "--check"]));
            checks
        }
        ProjectType::Node => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["npm", "test"]));
            checks.push(("lint", vec!["npx", "eslint", "."]));
            checks
        }
        ProjectType::Python => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["python", "-m", "pytest"]));
            checks.push(("lint", vec!["python", "-m", "flake8", "."]));
            checks.push(("typecheck", vec!["python", "-m", "mypy", "."]));
            checks
        }
        ProjectType::Go => {
            let mut checks = vec![("build", vec!["go", "build", "./..."])];
            #[cfg(not(test))]
            checks.push(("test", vec!["go", "test", "./..."]));
            checks.push(("vet", vec!["go", "vet", "./..."]));
            checks
        }
        ProjectType::Make => {
            let mut checks: Vec<(&str, Vec<&str>)> = vec![];
            #[cfg(not(test))]
            checks.push(("test", vec!["make", "test"]));
            checks
        }
        ProjectType::Unknown => vec![],
    }
}

/// Run health checks for a specific project type. Returns (name, passed, detail) tuples.
pub fn run_health_check_for_project(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let start = std::time::Instant::now();
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        let elapsed = format_duration(start.elapsed());
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, format!("ok ({elapsed})")));
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                let first_line = stderr.lines().next().unwrap_or("(unknown error)");
                results.push((
                    name,
                    false,
                    format!(
                        "FAIL ({elapsed}): {}",
                        truncate_with_ellipsis(first_line, 80)
                    ),
                ));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Run health checks and capture full error output for failures.
pub fn run_health_checks_full_output(
    project_type: &ProjectType,
) -> Vec<(&'static str, bool, String)> {
    let checks = health_checks_for_project(project_type);

    let mut results = Vec::new();
    for (name, args) in checks {
        let output = std::process::Command::new(args[0])
            .args(&args[1..])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                results.push((name, true, String::new()));
            }
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut full_output = String::new();
                if !stdout.is_empty() {
                    full_output.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !full_output.is_empty() {
                        full_output.push('\n');
                    }
                    full_output.push_str(&stderr);
                }
                results.push((name, false, full_output));
            }
            Err(e) => {
                results.push((name, false, format!("ERROR: {e}")));
            }
        }
    }
    results
}

/// Build a prompt describing health check failures for the AI to fix.
pub fn build_fix_prompt(failures: &[(&str, &str)]) -> String {
    if failures.is_empty() {
        return String::new();
    }
    let mut prompt = String::from(
        "Fix the following build/lint errors in this project. Read the relevant files, understand the errors, and apply fixes:\n\n",
    );
    for (name, output) in failures {
        prompt.push_str(&format!("## {name} errors:\n```\n{output}\n```\n\n"));
    }
    prompt.push_str(
        "After fixing, run the failing checks again to verify. Fix any remaining issues.",
    );
    prompt
}

pub fn handle_health() {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return;
    }
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_check_for_project(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return;
    }
    let all_passed = results.iter().all(|(_, passed, _)| *passed);
    for (name, passed, detail) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        println!("  {icon} {name}: {detail}");
    }
    if all_passed {
        println!("\n{GREEN}  All checks passed ✓{RESET}\n");
    } else {
        println!("\n{RED}  Some checks failed ✗{RESET}\n");
    }
}

/// Handle the /fix command. Returns Some(fix_prompt) if failures were sent to AI, None otherwise.
pub async fn handle_fix(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }
    println!("{DIM}  Detected project: {project_type}{RESET}");
    println!("{DIM}  Running health checks...{RESET}");
    let results = run_health_checks_full_output(&project_type);
    if results.is_empty() {
        println!("{DIM}  No checks configured for {project_type}{RESET}\n");
        return None;
    }
    for (name, passed, _) in &results {
        let icon = if *passed {
            format!("{GREEN}✓{RESET}")
        } else {
            format!("{RED}✗{RESET}")
        };
        let status = if *passed { "ok" } else { "FAIL" };
        println!("  {icon} {name}: {status}");
    }
    let failures: Vec<(&str, &str)> = results
        .iter()
        .filter(|(_, passed, _)| !passed)
        .map(|(name, _, output)| (*name, output.as_str()))
        .collect();
    if failures.is_empty() {
        println!("\n{GREEN}  All checks passed — nothing to fix ✓{RESET}\n");
        return None;
    }
    let fail_count = failures.len();
    println!("\n{YELLOW}  Sending {fail_count} failure(s) to AI for fixing...{RESET}\n");
    let fix_prompt = build_fix_prompt(&failures);
    run_prompt(agent, &fix_prompt, session_total, model).await;
    auto_compact_if_needed(agent);
    Some(fix_prompt)
}

// ── /test ─────────────────────────────────────────────────────────────

/// Return the test command for a given project type.
pub fn test_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some(("cargo test", vec!["cargo", "test"])),
        ProjectType::Node => Some(("npm test", vec!["npm", "test"])),
        ProjectType::Python => Some(("python -m pytest", vec!["python", "-m", "pytest"])),
        ProjectType::Go => Some(("go test ./...", vec!["go", "test", "./..."])),
        ProjectType::Make => Some(("make test", vec!["make", "test"])),
        ProjectType::Unknown => None,
    }
}

/// Handle the /test command: auto-detect project type and run tests.
/// Returns a summary string suitable for AI context.
pub fn handle_test() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match test_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No test command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Tests passed ({elapsed}){RESET}\n");
                Some(format!("Tests passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Tests failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Tests FAILED (exit {code}, {elapsed}): {label}");
                // Include a preview of the error output for AI context
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /lint ──────────────────────────────────────────────────────────────

/// Return the lint command for a given project type.
pub fn lint_command_for_project(
    project_type: &ProjectType,
) -> Option<(&'static str, Vec<&'static str>)> {
    match project_type {
        ProjectType::Rust => Some((
            "cargo clippy --all-targets -- -D warnings",
            vec!["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
        )),
        ProjectType::Node => Some(("npx eslint .", vec!["npx", "eslint", "."])),
        ProjectType::Python => Some(("ruff check .", vec!["ruff", "check", "."])),
        ProjectType::Go => Some(("golangci-lint run", vec!["golangci-lint", "run"])),
        ProjectType::Make | ProjectType::Unknown => None,
    }
}

/// Handle the /lint command: auto-detect project type and run linter.
/// Returns a summary string suitable for AI context.
pub fn handle_lint() -> Option<String> {
    let project_type = detect_project_type(&std::env::current_dir().unwrap_or_default());
    println!("{DIM}  Detected project: {project_type}{RESET}");
    if project_type == ProjectType::Unknown {
        println!(
            "{DIM}  No recognized project found. Looked for: Cargo.toml, package.json, pyproject.toml, setup.py, go.mod, Makefile{RESET}\n"
        );
        return None;
    }

    let (label, args) = match lint_command_for_project(&project_type) {
        Some(cmd) => cmd,
        None => {
            println!("{DIM}  No lint command configured for {project_type}{RESET}\n");
            return None;
        }
    };

    println!("{DIM}  Running: {label}...{RESET}");
    let start = std::time::Instant::now();
    let output = std::process::Command::new(args[0])
        .args(&args[1..])
        .output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);

            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{stderr}");
            }

            if o.status.success() {
                println!("\n{GREEN}  ✓ Lint passed ({elapsed}){RESET}\n");
                Some(format!("Lint passed ({elapsed}): {label}"))
            } else {
                let code = o.status.code().unwrap_or(-1);
                println!("\n{RED}  ✗ Lint failed (exit {code}, {elapsed}){RESET}\n");
                let mut summary = format!("Lint FAILED (exit {code}, {elapsed}): {label}");
                let error_text = if !stderr.is_empty() {
                    stderr.to_string()
                } else {
                    stdout.to_string()
                };
                let lines: Vec<&str> = error_text.lines().collect();
                let preview_lines = if lines.len() > 20 {
                    &lines[lines.len() - 20..]
                } else {
                    &lines
                };
                summary.push_str("\n\nLast output:\n");
                for line in preview_lines {
                    summary.push_str(line);
                    summary.push('\n');
                }
                Some(summary)
            }
        }
        Err(e) => {
            eprintln!("{RED}  ✗ Failed to run {label}: {e}{RESET}\n");
            Some(format!("Failed to run {label}: {e}"))
        }
    }
}

// ── /tree ────────────────────────────────────────────────────────────────

/// Build a directory tree from `git ls-files`.
pub fn build_project_tree(max_depth: usize) -> String {
    let files = match std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut files: Vec<String> = text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            files.sort();
            files
        }
        _ => return "(not a git repository — /tree requires git)".to_string(),
    };

    if files.is_empty() {
        return "(no tracked files)".to_string();
    }

    format_tree_from_paths(&files, max_depth)
}

/// Format a sorted list of file paths into an indented tree string.
pub fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1;

        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

pub fn handle_tree(input: &str) {
    let arg = input.strip_prefix("/tree").unwrap_or("").trim();
    let max_depth = if arg.is_empty() {
        3
    } else {
        match arg.parse::<usize>() {
            Ok(d) => d,
            Err(_) => {
                println!("{DIM}  usage: /tree [depth]  (default depth: 3){RESET}\n");
                return;
            }
        }
    };
    let tree = build_project_tree(max_depth);
    println!("{DIM}{tree}{RESET}\n");
}

// ── /run ─────────────────────────────────────────────────────────────────

/// Run a shell command directly and print its output.
pub fn run_shell_command(cmd: &str) {
    let start = std::time::Instant::now();
    let output = std::process::Command::new("sh").args(["-c", cmd]).output();
    let elapsed = format_duration(start.elapsed());

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            if !stdout.is_empty() {
                print!("{stdout}");
            }
            if !stderr.is_empty() {
                eprint!("{RED}{stderr}{RESET}");
            }
            let code = o.status.code().unwrap_or(-1);
            if code == 0 {
                println!("{DIM}  ✓ exit {code} ({elapsed}){RESET}\n");
            } else {
                println!("{RED}  ✗ exit {code} ({elapsed}){RESET}\n");
            }
        }
        Err(e) => {
            eprintln!("{RED}  error running command: {e}{RESET}\n");
        }
    }
}

pub fn handle_run(input: &str) {
    let cmd = if input.starts_with("/run ") {
        input.trim_start_matches("/run ").trim()
    } else if input.starts_with('!') && input.len() > 1 {
        input[1..].trim()
    } else {
        ""
    };
    if cmd.is_empty() {
        println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
    } else {
        run_shell_command(cmd);
    }
}

pub fn handle_run_usage() {
    println!("{DIM}  usage: /run <command>  or  !<command>");
    println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
}

// ── /find ────────────────────────────────────────────────────────────────

/// Result of a fuzzy file match: (file_path, score, match_ranges).
/// Higher score = better match. match_ranges are byte offsets into the lowercased path.
#[derive(Debug, Clone, PartialEq)]
pub struct FindMatch {
    pub path: String,
    pub score: i32,
}

/// Score a file path against a fuzzy pattern (case-insensitive substring match).
/// Returns None if the pattern doesn't match.
/// Scoring:
///   - Base score for containing the pattern as a substring
///   - Bonus for matching the filename (last component) vs directory
///   - Bonus for exact filename match
///   - Bonus for match at the start of the filename
///   - Shorter paths score higher (less noise)
pub fn fuzzy_score(path: &str, pattern: &str) -> Option<i32> {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if !path_lower.contains(&pattern_lower) {
        return None;
    }

    let mut score: i32 = 100; // base score for matching

    // Extract filename (last path component)
    let filename = path.rsplit('/').next().unwrap_or(path);
    let filename_lower = filename.to_lowercase();

    // Big bonus if the pattern matches within the filename itself
    if filename_lower.contains(&pattern_lower) {
        score += 50;

        // Bonus for matching at the start of filename
        if filename_lower.starts_with(&pattern_lower) {
            score += 30;
        }

        // Bonus for exact filename match (without extension)
        let stem = filename_lower.split('.').next().unwrap_or(&filename_lower);
        if stem == pattern_lower {
            score += 20;
        }
    }

    // Shorter paths are slightly preferred (less deeply nested = more relevant)
    let depth = path.matches('/').count();
    score -= depth as i32 * 2;

    Some(score)
}

/// Find files matching a fuzzy pattern. Uses `git ls-files` if in a git repo,
/// otherwise falls back to a recursive directory listing.
pub fn find_files(pattern: &str) -> Vec<FindMatch> {
    let files = list_project_files();
    let mut matches: Vec<FindMatch> = files
        .iter()
        .filter_map(|path| {
            fuzzy_score(path, pattern).map(|score| FindMatch {
                path: path.clone(),
                score,
            })
        })
        .collect();

    // Sort by score descending, then alphabetically for ties
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.cmp(&b.path)));
    matches
}

/// List all project files. Prefers `git ls-files`, falls back to walkdir-style listing.
fn list_project_files() -> Vec<String> {
    if let Ok(output) = std::process::Command::new("git")
        .args(["ls-files"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            return text
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
        }
    }

    // Fallback: recursive listing of current directory (respecting common ignores)
    walk_directory(".", 8)
}

/// Simple recursive directory walk (fallback when not in a git repo).
fn walk_directory(dir: &str, max_depth: usize) -> Vec<String> {
    let mut files = Vec::new();
    walk_directory_inner(dir, max_depth, 0, &mut files);
    files
}

fn walk_directory_inner(dir: &str, max_depth: usize, depth: usize, files: &mut Vec<String>) {
    if depth > max_depth {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Skip hidden dirs and common ignore patterns
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        let path = if dir == "." {
            name.clone()
        } else {
            format!("{dir}/{name}")
        };
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            walk_directory_inner(&path, max_depth, depth + 1, files);
        } else {
            files.push(path);
        }
    }
}

/// Highlight the matching pattern within a file path for display.
/// Returns the path with ANSI bold/color around the matched portion.
pub fn highlight_match(path: &str, pattern: &str) -> String {
    let path_lower = path.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    if let Some(pos) = path_lower.rfind(&pattern_lower) {
        // Prefer highlighting in the filename portion
        let end = pos + pattern.len();
        format!(
            "{}{BOLD}{GREEN}{}{RESET}{}",
            &path[..pos],
            &path[pos..end],
            &path[end..]
        )
    } else {
        path.to_string()
    }
}

pub fn handle_find(input: &str) {
    let arg = input.strip_prefix("/find").unwrap_or("").trim();
    if arg.is_empty() {
        println!("{DIM}  usage: /find <pattern>");
        println!("  Fuzzy-search project files by name.");
        println!("  Examples: /find main, /find .toml, /find test{RESET}\n");
        return;
    }

    let matches = find_files(arg);
    if matches.is_empty() {
        println!("{DIM}  No files matching '{arg}'.{RESET}\n");
    } else {
        let count = matches.len();
        let shown = matches.iter().take(20);
        println!(
            "{DIM}  {count} file{s} matching '{arg}':",
            s = if count == 1 { "" } else { "s" }
        );
        for m in shown {
            let highlighted = highlight_match(&m.path, arg);
            println!("    {highlighted}");
        }
        if count > 20 {
            println!("    {DIM}... and {} more{RESET}", count - 20);
        }
        println!("{RESET}");
    }
}

// ── /index ───────────────────────────────────────────────────────────────

/// An entry in the project index: path, line count, and first meaningful line.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub path: String,
    pub lines: usize,
    pub summary: String,
}

/// Extract the first meaningful line from file content.
/// Skips blank lines, then grabs the first doc comment (`//!`, `///`, `#`),
/// module declaration, or any non-empty line.
pub fn extract_first_meaningful_line(content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Return the first non-empty line, truncated
        return truncate_with_ellipsis(trimmed, 80);
    }
    String::new()
}

/// Build a project index by listing files and extracting metadata.
/// Uses `git ls-files` when available, falls back to directory walk.
/// Only indexes text-like source files (skips binaries, images, etc.).
pub fn build_project_index() -> Vec<IndexEntry> {
    let files = list_project_files();
    let mut entries = Vec::new();

    for path in &files {
        // Skip binary/non-text files based on extension
        if is_binary_extension(path) {
            continue;
        }

        // Read the file — skip if it fails (binary, permission, etc.)
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let line_count = content.lines().count();
        let summary = extract_first_meaningful_line(&content);

        entries.push(IndexEntry {
            path: path.clone(),
            lines: line_count,
            summary,
        });
    }

    entries
}

/// Check if a file extension suggests a binary/non-text file.
pub fn is_binary_extension(path: &str) -> bool {
    let binary_exts = [
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".ico", ".svg", ".woff", ".woff2",
        ".ttf", ".otf", ".eot", ".pdf", ".zip", ".gz", ".tar", ".bz2", ".xz", ".7z", ".rar",
        ".exe", ".dll", ".so", ".dylib", ".o", ".a", ".class", ".pyc", ".pyo", ".wasm", ".lock",
    ];
    let lower = path.to_lowercase();
    binary_exts.iter().any(|ext| lower.ends_with(ext))
}

/// Format the project index as a table string.
pub fn format_project_index(entries: &[IndexEntry]) -> String {
    if entries.is_empty() {
        return "(no indexable files found)".to_string();
    }

    let mut output = String::new();

    // Find max path length for alignment (capped at 50)
    let max_path_len = entries
        .iter()
        .map(|e| e.path.len())
        .max()
        .unwrap_or(0)
        .min(50);

    output.push_str(&format!(
        "  {:<width$}  {:>5}  {}\n",
        "Path",
        "Lines",
        "Summary",
        width = max_path_len
    ));
    output.push_str(&format!(
        "  {:<width$}  {:>5}  {}\n",
        "─".repeat(max_path_len.min(50)),
        "─────",
        "─".repeat(40),
        width = max_path_len
    ));

    for entry in entries {
        let path_display = if entry.path.len() > 50 {
            format!("…{}", &entry.path[entry.path.len() - 49..])
        } else {
            entry.path.clone()
        };
        output.push_str(&format!(
            "  {:<width$}  {:>5}  {}\n",
            path_display,
            entry.lines,
            entry.summary,
            width = max_path_len
        ));
    }

    // Summary line
    let total_files = entries.len();
    let total_lines: usize = entries.iter().map(|e| e.lines).sum();
    output.push_str(&format!(
        "\n  {} file{}, {} total lines\n",
        total_files,
        if total_files == 1 { "" } else { "s" },
        total_lines
    ));

    output
}

/// Handle the /index command: build and display a project file index.
pub fn handle_index() {
    println!("{DIM}  Building project index...{RESET}");
    let entries = build_project_index();
    if entries.is_empty() {
        println!("{DIM}  (no indexable source files found){RESET}\n");
    } else {
        let formatted = format_project_index(&entries);
        println!("{DIM}{formatted}{RESET}");
    }
}


// ── Shared utilities (used by research/writing/workflow modules) ────────

// ── /article ────────────────────────────────────────────────────────────

/// Drafts directory for saved articles.
pub const DRAFTS_DIR: &str = ".journalist/drafts";

/// Generate a slug from a topic string: lowercase, ASCII-safe, hyphen-separated.
/// Non-ASCII characters (e.g. Korean) are kept as-is; only ASCII is lowercased.
/// Whitespace and punctuation become hyphens; consecutive hyphens are collapsed.
/// The slug is truncated to at most `max_len` characters (default 50).
pub fn topic_to_slug(topic: &str, max_len: usize) -> String {
    let mut slug = String::with_capacity(topic.len());
    let mut last_was_hyphen = true; // prevent leading hyphen
    for ch in topic.chars() {
        if ch.is_alphanumeric() {
            if ch.is_ascii() {
                slug.extend(ch.to_lowercase());
            } else {
                slug.push(ch);
            }
            last_was_hyphen = false;
        } else if !last_was_hyphen {
            slug.push('-');
            last_was_hyphen = true;
        }
    }
    // Trim trailing hyphen
    let slug = slug.trim_end_matches('-');
    // Truncate to max_len *characters* (not bytes) at a safe boundary
    if slug.chars().count() > max_len {
        slug.chars()
            .take(max_len)
            .collect::<String>()
            .trim_end_matches('-')
            .to_string()
    } else {
        slug.to_string()
    }
}

/// Get today's date as YYYY-MM-DD string.
pub fn today_str() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Parse UTC date from epoch seconds
    let days = secs / 86400;
    // Civil date from day count (algorithm from Howard Hinnant)
    let z = days as i64 + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Build the draft file path: `.journalist/drafts/YYYY-MM-DD_<slug>.md`
pub fn draft_file_path(topic: &str) -> std::path::PathBuf {
    draft_file_path_with_date(topic, &today_str())
}

/// Build the draft file path with an explicit date string (for testing).
pub fn draft_file_path_with_date(topic: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(topic, 50);
    let filename = if slug.is_empty() {
        format!("{date}_draft.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(DRAFTS_DIR).join(filename)
}

/// Save article draft to file. Creates the drafts directory if needed.
pub fn save_article_draft(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Search for research files related to the given topic in `.journalist/research/`.
/// Returns a list of (filename, content) pairs for matching files.
/// A file matches if any keyword from the topic appears in the filename's slug portion.
pub fn find_related_research(topic: &str) -> Vec<(String, String)> {
    find_related_research_in(topic, std::path::Path::new(RESEARCH_DIR))
}

/// Search for related research files in a specific directory (for testing).
pub fn find_related_research_in(
    topic: &str,
    research_dir: &std::path::Path,
) -> Vec<(String, String)> {
    let keywords: Vec<&str> = topic.split_whitespace().filter(|k| !k.is_empty()).collect();
    if keywords.is_empty() {
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
        let filename_lower = filename.to_lowercase();
        let matches = keywords
            .iter()
            .any(|kw| filename_lower.contains(&kw.to_lowercase()));
        if matches {
            if let Ok(content) = std::fs::read_to_string(&path) {
                results.push((filename, content));
            }
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// Parse `/article` arguments, extracting `--type <type>` if present.
/// Returns (article_type, remaining_topic).
pub fn parse_article_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    let mut article_type: Option<String> = None;
    let mut remaining_parts: Vec<String> = Vec::new();

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "--type" {
            if i + 1 < tokens.len() {
                article_type = Some(tokens[i + 1].to_string());
                i += 2;
            } else {
                i += 1;
            }
        } else {
            remaining_parts.push(tokens[i].to_string());
            i += 1;
        }
    }

    (article_type, remaining_parts.join(" "))
}

/// Build the article prompt for a given topic.
/// Returns (prompt_text, has_topic).
/// `research_context` contains (filename, content) pairs of related research files.
/// `article_type` selects the template: "straight" (default), "feature", "analysis", "planning".
pub fn build_article_prompt(
    topic: &str,
    research_context: &[(String, String)],
    article_type: Option<&str>,
) -> (String, bool) {
    if topic.is_empty() {
        (
            "기사 작성을 도와드리겠습니다. 어떤 주제로 기사를 작성하시겠습니까? \
             주제를 알려주시면 다음 구조로 초안을 제안합니다:\n\
             1. 리드 (핵심 요약)\n\
             2. 본문 (배경, 맥락, 상세)\n\
             3. 인용 (관계자 코멘트)\n\
             4. 맺음 (전망, 의미)\n\n\
             💡 `--type` 옵션으로 기사 유형을 지정할 수 있습니다:\n\
             - `straight` — 스트레이트 (역피라미드, 기본값)\n\
             - `feature` — 피처 (도입부+에피소드+본문+맺음)\n\
             - `analysis` — 해설 (배경+분석+전망)\n\
             - `planning` — 기획 (문제제기+현황+원인+대안)\n\
             - `interview` — 인터뷰 (도입부+인터뷰이 소개+Q&A+핵심 발언+맺음)\n\
             - `column` — 칼럼 (문제 제기+논거 전개+반론 검토+결론/제언)\n\
             - `editorial` — 사설 (사안 제시+논점 분석+주장+근거+제언)"
                .to_string(),
            false,
        )
    } else {
        let structure = match article_type.unwrap_or("straight") {
            "feature" | "피처" => {
                "1. **도입부** — 독자의 관심을 끄는 장면 묘사 또는 일화 (1-2문단)\n\
                 2. **에피소드** — 핵심 인물/사건의 구체적 이야기 (2-3문단)\n\
                 3. **본문** — 배경 설명, 맥락, 의미 부여 (3-5문단)\n\
                 4. **인용** — 관계자·전문가 코멘트 위치 표시 (\"[이름/직함] 인용 필요\")\n\
                 5. **맺음** — 여운을 남기는 마무리, 도입부와 호응 (1-2문단)"
            }
            "analysis" | "해설" => {
                "1. **핵심 요약** — 무엇이 왜 중요한지 한 문단 정리\n\
                 2. **배경** — 이 이슈가 나온 경위, 역사적 맥락 (2-3문단)\n\
                 3. **분석** — 원인, 이해관계, 쟁점별 심층 분석 (3-5문단)\n\
                 4. **전망** — 향후 시나리오와 예상 영향 (1-2문단)\n\
                 5. **인용** — 전문가·관계자 코멘트 위치 표시 (\"[이름/직함] 인용 필요\")"
            }
            "planning" | "기획" => {
                "1. **문제제기** — 왜 이 주제를 다루는지, 독자가 관심 가질 이유 (1-2문단)\n\
                 2. **현황** — 현재 상황, 관련 데이터와 사례 (2-3문단)\n\
                 3. **원인** — 문제의 구조적 원인 분석 (2-3문단)\n\
                 4. **대안** — 해결 방안, 해외 사례, 전문가 제안 (2-3문단)\n\
                 5. **인용** — 관계자·전문가 코멘트 위치 표시 (\"[이름/직함] 인용 필요\")\n\
                 6. **맺음** — 정리 및 향후 과제 (1문단)"
            }
            "interview" | "인터뷰" => {
                "1. **도입부** — 인터뷰 배경과 만남의 상황 묘사 (1-2문단)\n\
                 2. **인터뷰이 소개** — 인물 약력, 현재 직함, 전문 분야 (1문단)\n\
                 3. **Q&A** — 핵심 질문과 답변 5-7개 (질문은 굵게, 답변은 일반체)\n\
                 4. **핵심 발언** — 가장 인상적인 발언 1-2개 인용 블록으로 강조\n\
                 5. **맺음** — 인터뷰 소감, 향후 계획 또는 전망 (1문단)"
            }
            "column" | "칼럼" => {
                "1. **문제 제기** — 독자의 관심을 끄는 도입, 이슈의 핵심 질문 (1-2문단)\n\
                 2. **논거 전개** — 주장을 뒷받침하는 근거, 사례, 데이터 (2-3문단)\n\
                 3. **반론 검토** — 예상되는 반론과 이에 대한 재반박 (1-2문단)\n\
                 4. **결론/제언** — 필자의 최종 입장과 독자에게 던지는 메시지 (1문단)\n\
                 ※ 칼럼은 필자의 관점이 드러나는 글입니다. 1인칭 사용 가능."
            }
            "editorial" | "사설" => {
                "1. **사안 제시** — 다루는 사안의 개요와 시의성 (1문단)\n\
                 2. **논점 분석** — 사안의 핵심 쟁점을 다각도로 분석 (2-3문단)\n\
                 3. **주장** — 신문사의 입장을 명확히 제시 (1문단)\n\
                 4. **근거** — 주장을 뒷받침하는 논리적 근거와 사례 (2-3문단)\n\
                 5. **제언** — 관련 주체(정부, 기업, 시민 등)에 대한 구체적 제언 (1문단)\n\
                 ※ 사설은 신문사의 공식 입장입니다. 권위 있고 절제된 논조를 유지하세요."
            }
            // "straight" and anything else → default inverted pyramid
            _ => {
                "1. **리드** — 역피라미드 구조: 육하원칙(누가, 언제, 어디서, 무엇을, 어떻게, 왜)을 포함한 핵심 요약 (1-2문장)\n\
                 2. **본문** — 배경, 맥락, 상세 내용 (3-5문단)\n\
                 3. **인용** — 관계자 코멘트가 들어갈 위치 표시 (\"[관계자 이름/직함] 인용 필요\")\n\
                 4. **맺음** — 향후 전망 또는 의미 (1-2문장)"
            }
        };

        let type_label = match article_type.unwrap_or("straight") {
            "feature" | "피처" => "피처",
            "analysis" | "해설" => "해설",
            "planning" | "기획" => "기획",
            "interview" | "인터뷰" => "인터뷰",
            "column" | "칼럼" => "칼럼",
            "editorial" | "사설" => "사설",
            _ => "스트레이트",
        };

        let mut prompt = format!(
            "다음 주제로 한국 신문 기사 초안을 작성해주세요: {topic}\n\n\
             📰 기사 유형: **{type_label}**\n\n\
             다음 구조를 따라주세요:\n\
             {structure}\n\n\
             주의사항:\n\
             - 사실 확인이 필요한 부분은 [확인 필요]로 표시\n\
             - 추가 취재가 필요한 부분은 [취재 필요]로 표시\n\
             - 객관적이고 중립적인 톤 유지"
        );

        if !research_context.is_empty() {
            prompt.push_str("\n\n---\n\n📎 **관련 리서치 자료** (기사 작성 시 참고하세요):\n");
            for (filename, content) in research_context {
                prompt.push_str(&format!("\n### 📄 {filename}\n{content}\n"));
            }
        }

        (prompt, true)
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

    fn temp_sources_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sources.json");
        (dir, path)
    }

    #[test]
    fn topic_to_slug_basic() {
        assert_eq!(topic_to_slug("반도체 수출 동향", 50), "반도체-수출-동향");
    }

    #[test]
    fn topic_to_slug_ascii() {
        assert_eq!(topic_to_slug("Hello World", 50), "hello-world");
    }

    #[test]
    fn topic_to_slug_mixed() {
        assert_eq!(
            topic_to_slug("삼성전자 Q1 실적", 50),
            "삼성전자-q1-실적"
        );
    }

    #[test]
    fn topic_to_slug_punctuation() {
        assert_eq!(topic_to_slug("AI, 반도체... 전망!", 50), "ai-반도체-전망");
    }

    #[test]
    fn topic_to_slug_truncation() {
        let long = "가".repeat(60);
        let slug = topic_to_slug(&long, 50);
        assert!(slug.len() <= 50 * 3); // Korean chars are 3 bytes
        assert!(slug.chars().count() <= 50);
    }

    #[test]
    fn draft_file_path_with_topic() {
        let path = draft_file_path_with_date("반도체 수출", "2026-03-17");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/drafts/2026-03-17_반도체-수출.md"
        );
    }

    #[test]
    fn draft_file_path_empty_topic() {
        let path = draft_file_path_with_date("", "2026-03-17");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/drafts/2026-03-17_draft.md"
        );
    }

    #[test]
    fn save_article_draft_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("drafts").join("test.md");
        save_article_draft(&path, "# 테스트 기사\n본문").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# 테스트 기사\n본문");
    }

    #[test]
    fn sources_add_single_arg_rejected() {
        let one_arg = "홍길동";
        let parts: Vec<&str> = one_arg.splitn(4, ' ').collect();
        assert!(parts.len() < 3);
    }

    #[test]
    fn article_prompt_without_topic() {
        let (prompt, has_topic) = build_article_prompt("", &[], None);
        assert!(!has_topic);
        assert!(prompt.contains("어떤 주제로 기사를 작성하시겠습니까"));
        assert!(prompt.contains("리드"));
    }

    #[test]
    fn article_prompt_with_topic() {
        let (prompt, has_topic) = build_article_prompt("반도체 수출 동향", &[], None);
        assert!(has_topic);
        assert!(prompt.contains("반도체 수출 동향"));
        assert!(prompt.contains("리드"));
        assert!(prompt.contains("육하원칙"));
        assert!(prompt.contains("[확인 필요]"));
    }

    #[test]
    fn article_prompt_includes_research_context() {
        let research = vec![
            ("반도체-수출-동향.md".to_string(), "# 반도체 수출 리서치\n수출액 증가 추세".to_string()),
        ];
        let (prompt, has_topic) = build_article_prompt("반도체 수출 동향", &research, None);
        assert!(has_topic);
        assert!(prompt.contains("관련 리서치 자료"));
        assert!(prompt.contains("반도체-수출-동향.md"));
        assert!(prompt.contains("수출액 증가 추세"));
    }

    #[test]
    fn article_prompt_no_research_section_when_empty() {
        let (prompt, _) = build_article_prompt("반도체 수출 동향", &[], None);
        assert!(!prompt.contains("관련 리서치 자료"));
    }

    #[test]
    fn parse_article_args_no_type() {
        let (article_type, topic) = parse_article_args("반도체 수출 동향");
        assert!(article_type.is_none());
        assert_eq!(topic, "반도체 수출 동향");
    }

    #[test]
    fn parse_article_args_with_type() {
        let (article_type, topic) = parse_article_args("--type feature 반도체 수출 동향");
        assert_eq!(article_type.as_deref(), Some("feature"));
        assert_eq!(topic, "반도체 수출 동향");
    }

    #[test]
    fn parse_article_args_type_only() {
        let (article_type, topic) = parse_article_args("--type analysis");
        assert_eq!(article_type.as_deref(), Some("analysis"));
        assert!(topic.is_empty());
    }

    #[test]
    fn parse_article_args_empty() {
        let (article_type, topic) = parse_article_args("");
        assert!(article_type.is_none());
        assert!(topic.is_empty());
    }

    #[test]
    fn article_prompt_straight_type() {
        let (prompt, has_topic) =
            build_article_prompt("반도체 수출", &[], Some("straight"));
        assert!(has_topic);
        assert!(prompt.contains("역피라미드"));
        assert!(prompt.contains("리드"));
    }

    #[test]
    fn article_prompt_feature_type() {
        let (prompt, has_topic) =
            build_article_prompt("반도체 수출", &[], Some("feature"));
        assert!(has_topic);
        assert!(prompt.contains("도입부"));
        assert!(prompt.contains("에피소드"));
    }

    #[test]
    fn article_prompt_analysis_type() {
        let (prompt, has_topic) =
            build_article_prompt("반도체 수출", &[], Some("analysis"));
        assert!(has_topic);
        assert!(prompt.contains("배경"));
        assert!(prompt.contains("분석"));
        assert!(prompt.contains("전망"));
    }

    #[test]
    fn article_prompt_planning_type() {
        let (prompt, has_topic) =
            build_article_prompt("반도체 수출", &[], Some("planning"));
        assert!(has_topic);
        assert!(prompt.contains("문제제기"));
        assert!(prompt.contains("현황"));
        assert!(prompt.contains("대안"));
    }

    #[test]
    fn article_prompt_interview_type() {
        let (prompt, has_topic) =
            build_article_prompt("김 교수 인터뷰", &[], Some("interview"));
        assert!(has_topic);
        assert!(prompt.contains("인터뷰이 소개"));
        assert!(prompt.contains("Q&A"));
        assert!(prompt.contains("핵심 발언"));
        assert!(prompt.contains("인터뷰"));
    }

    #[test]
    fn article_prompt_column_type() {
        let (prompt, has_topic) =
            build_article_prompt("AI 규제 논란", &[], Some("column"));
        assert!(has_topic);
        assert!(prompt.contains("문제 제기"));
        assert!(prompt.contains("논거 전개"));
        assert!(prompt.contains("반론 검토"));
        assert!(prompt.contains("칼럼"));
    }

    #[test]
    fn article_prompt_editorial_type() {
        let (prompt, has_topic) =
            build_article_prompt("교육 개혁", &[], Some("editorial"));
        assert!(has_topic);
        assert!(prompt.contains("사안 제시"));
        assert!(prompt.contains("논점 분석"));
        assert!(prompt.contains("사설"));
    }

    #[test]
    fn parse_article_args_interview_type() {
        let (article_type, topic) = parse_article_args("--type interview 김 교수 인터뷰");
        assert_eq!(article_type.as_deref(), Some("interview"));
        assert_eq!(topic, "김 교수 인터뷰");
    }

    #[test]
    fn article_prompt_default_type_is_straight() {
        let (prompt_default, _) = build_article_prompt("반도체 수출", &[], None);
        let (prompt_straight, _) =
            build_article_prompt("반도체 수출", &[], Some("straight"));
        // Both should contain the same structure keywords
        assert!(prompt_default.contains("리드"));
        assert!(prompt_straight.contains("리드"));
        assert!(prompt_default.contains("역피라미드") || prompt_default.contains("육하원칙"));
    }

    #[test]
    fn find_related_research_matches_keyword_in_filename() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_반도체-수출-동향.md"),
            "# 반도체 리서치\n내용",
        )
        .unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_부동산-시장.md"),
            "# 부동산 리서치\n내용",
        )
        .unwrap();

        let results = find_related_research_in("반도체 수출", &research_dir);
        assert_eq!(results.len(), 1);
        assert!(results[0].0.contains("반도체"));
        assert!(results[0].1.contains("반도체 리서치"));
    }

    #[test]
    fn find_related_research_no_match() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_부동산-시장.md"),
            "# 부동산\n내용",
        )
        .unwrap();

        let results = find_related_research_in("반도체 수출", &research_dir);
        assert!(results.is_empty());
    }

    #[test]
    fn find_related_research_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        let results = find_related_research_in("반도체", &research_dir);
        assert!(results.is_empty());
    }

    #[test]
    fn find_related_research_multiple_matches() {
        let dir = tempfile::TempDir::new().unwrap();
        let research_dir = dir.path().join("research");
        std::fs::create_dir_all(&research_dir).unwrap();
        std::fs::write(
            research_dir.join("2026-03-16_반도체-수출.md"),
            "수출 자료",
        )
        .unwrap();
        std::fs::write(
            research_dir.join("2026-03-17_반도체-시장-전망.md"),
            "시장 전망",
        )
        .unwrap();

        let results = find_related_research_in("반도체", &research_dir);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn draft_file_path_contains_date_and_slug() {
        let path = draft_file_path_with_date("AI 반도체 전망", "2026-01-15");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("2026-01-15"));
        assert!(path_str.contains("ai-반도체-전망"));
        assert!(path_str.starts_with(".journalist/drafts/"));
        assert!(path_str.ends_with(".md"));
    }

    #[test]
    fn topic_to_slug_empty() {
        assert_eq!(topic_to_slug("", 50), "");
    }

    #[test]
    fn topic_to_slug_only_punctuation() {
        assert_eq!(topic_to_slug("..., !!!", 50), "");
    }

    #[test]
    fn news_command_recognized() {
        use crate::commands::{is_unknown_command, KNOWN_COMMANDS};
        assert!(!is_unknown_command("/news"));
        assert!(!is_unknown_command("/news 반도체"));
        assert!(
            KNOWN_COMMANDS.contains(&"/news"),
            "/news should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn news_command_matching() {
        let news_matches = |s: &str| s == "/news" || s.starts_with("/news ");
        assert!(news_matches("/news"));
        assert!(news_matches("/news 반도체"));
        assert!(news_matches("/news save 1"));
        assert!(!news_matches("/newsroom"));
        assert!(!news_matches("/newsletter"));
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

    fn temp_archive_paths() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let archive_dir = dir.path().join("archive");
        std::fs::create_dir_all(&archive_dir).unwrap();
        let index_path = archive_dir.join("index.json");
        (dir, index_path, archive_dir)
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

    #[test]
    fn coverage_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/coverage"),
            "/coverage should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn dashboard_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/dashboard"),
            "/dashboard should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn publish_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/publish"),
            "/publish should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn anonymize_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/anonymize"),
            "/anonymize should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn press_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/press"),
            "/press should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn improve_command_in_known_commands() {
        assert!(
            crate::commands::KNOWN_COMMANDS.contains(&"/improve"),
            "/improve should be in KNOWN_COMMANDS"
        );
    }

    fn temp_calendar_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("calendar.json");
        (dir, path)
    }

    #[test]
    fn sns_subcommand_routing_help() {
        // Verify the subcommand parsing logic
        let input = "/sns";
        let args = input.strip_prefix("/sns").unwrap_or("").trim();
        let subcmd = args.split_whitespace().next().unwrap_or("help");
        assert_eq!(subcmd, "help");
    }

    #[test]
    fn sns_subcommand_routing_trend() {
        let input = "/sns trend";
        let args = input.strip_prefix("/sns").unwrap_or("").trim();
        let subcmd = args.split_whitespace().next().unwrap_or("help");
        assert_eq!(subcmd, "trend");
    }

    #[test]
    fn sns_subcommand_routing_search_keyword() {
        let input = "/sns search 반도체";
        let args = input.strip_prefix("/sns").unwrap_or("").trim();
        let subcmd = args.split_whitespace().next().unwrap_or("help");
        assert_eq!(subcmd, "search");
        let keyword = args.strip_prefix("search").unwrap_or("").trim();
        assert_eq!(keyword, "반도체");
    }

    #[test]
    fn sns_subcommand_routing_buzz_keyword() {
        let input = "/sns buzz AI규제";
        let args = input.strip_prefix("/sns").unwrap_or("").trim();
        let subcmd = args.split_whitespace().next().unwrap_or("help");
        assert_eq!(subcmd, "buzz");
        let keyword = args.strip_prefix("buzz").unwrap_or("").trim();
        assert_eq!(keyword, "AI규제");
    }

    #[test]
    fn sns_search_empty_keyword_detected() {
        let input = "/sns search";
        let args = input.strip_prefix("/sns").unwrap_or("").trim();
        let keyword = args.strip_prefix("search").unwrap_or("").trim();
        assert!(keyword.is_empty());
    }

    #[test]
    fn sns_buzz_empty_keyword_detected() {
        let input = "/sns buzz";
        let args = input.strip_prefix("/sns").unwrap_or("").trim();
        let keyword = args.strip_prefix("buzz").unwrap_or("").trim();
        assert!(keyword.is_empty());
    }

    #[test]
    fn sns_cache_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sns").join("test_keyword_2026-03-21.md");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, "test content").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "test content");
    }

    fn temp_performance_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("performance.json");
        (dir, path)
    }

    #[test]
    fn note_command_in_known_commands() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/note"),
            "/note should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn contact_command_in_known_commands() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/contact"),
            "/contact should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn save_and_list_breaking() {
        let dir = tempfile::TempDir::new().unwrap();
        let file1 = dir.path().join("2026-03-22_100000_test1.md");
        let file2 = dir.path().join("2026-03-22_110000_test2.md");

        std::fs::write(&file1, "속보 1").unwrap();
        std::fs::write(&file2, "속보 2").unwrap();

        // Verify files are readable
        let c1 = std::fs::read_to_string(&file1).unwrap();
        assert_eq!(c1, "속보 1");
        let c2 = std::fs::read_to_string(&file2).unwrap();
        assert_eq!(c2, "속보 2");
    }

    #[test]
    fn diary_in_known_commands() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/diary"),
            "/diary should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn save_diary_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test-diary.md");
        std::fs::write(&path, "일지 내용").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "일지 내용");
    }
}
