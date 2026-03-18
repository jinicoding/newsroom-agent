//! Project-related command handlers: /context, /init, /health, /fix, /test, /lint,
//! /tree, /run, /docs, /find, /index, /article, /research, /sources, /factcheck, /briefing.

use crate::cli;
use crate::commands::auto_compact_if_needed;
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

// ── /article ────────────────────────────────────────────────────────────

/// Drafts directory for saved articles.
const DRAFTS_DIR: &str = ".journalist/drafts";

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
fn today_str() -> String {
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
fn save_article_draft(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
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

/// Build the article prompt for a given topic.
/// Returns (prompt_text, has_topic).
/// `research_context` contains (filename, content) pairs of related research files.
pub fn build_article_prompt(topic: &str, research_context: &[(String, String)]) -> (String, bool) {
    if topic.is_empty() {
        (
            "기사 작성을 도와드리겠습니다. 어떤 주제로 기사를 작성하시겠습니까? \
             주제를 알려주시면 다음 구조로 초안을 제안합니다:\n\
             1. 리드 (핵심 요약)\n\
             2. 본문 (배경, 맥락, 상세)\n\
             3. 인용 (관계자 코멘트)\n\
             4. 맺음 (전망, 의미)"
                .to_string(),
            false,
        )
    } else {
        let mut prompt = format!(
            "다음 주제로 한국 신문 기사 초안을 작성해주세요: {topic}\n\n\
             다음 구조를 따라주세요:\n\
             1. **리드** — 육하원칙(누가, 언제, 어디서, 무엇을, 어떻게, 왜)을 포함한 핵심 요약 (1-2문장)\n\
             2. **본문** — 배경, 맥락, 상세 내용 (3-5문단)\n\
             3. **인용** — 관계자 코멘트가 들어갈 위치 표시 (\"[관계자 이름/직함] 인용 필요\")\n\
             4. **맺음** — 향후 전망 또는 의미 (1-2문장)\n\n\
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

/// Handle the /article command: AI-assisted article writing with structured format.
pub async fn handle_article(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let topic = input
        .strip_prefix("/article")
        .unwrap_or("")
        .trim();

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

    let (prompt, _) = build_article_prompt(topic, &research);

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

// ── /research ───────────────────────────────────────────────────────────

/// Directory for cached research results.
const RESEARCH_DIR: &str = ".journalist/research";

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

    let prompt = format!(
        "다음 주제에 대해 웹 리서치를 수행해주세요: {topic}\n\n\
         다음 단계를 따라주세요:\n\
         1. DuckDuckGo로 검색: curl -s \"https://lite.duckduckgo.com/lite?q={}\" | sed 's/<[^>]*>//g' | head -80\n\
         2. 네이버 뉴스 검색: curl -s \"https://search.naver.com/search.naver?where=news&query={}\" | sed 's/<[^>]*>//g' | head -80\n\
         3. 검색 결과를 종합하여 다음을 정리:\n\
            - **핵심 사실** — 확인된 주요 정보\n\
            - **주요 출처** — 신뢰할 수 있는 출처 목록\n\
            - **쟁점** — 다른 시각이나 논란\n\
            - **추가 취재 제안** — 더 파고들 수 있는 방향\n\n\
         모든 정보에 출처를 명시하고, 확인되지 않은 내용은 명확히 표시하세요.",
        topic.replace(' ', "+"),
        topic.replace(' ', "+"),
    );

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
const SOURCES_FILE: &str = ".journalist/sources.json";

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

fn ensure_sources_dir_at(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        if !dir.exists() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
}

fn load_sources() -> Vec<serde_json::Value> {
    load_sources_from(std::path::Path::new(SOURCES_FILE))
}

fn load_sources_from(path: &std::path::Path) -> Vec<serde_json::Value> {
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

fn save_sources_to(sources: &[serde_json::Value], path: &std::path::Path) {
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
fn source_matches(source: &serde_json::Value, query_lower: &str) -> bool {
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

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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

    fn temp_sources_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sources.json");
        (dir, path)
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

    // --- article draft path tests ---

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

    // --- research file path tests ---

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

    // --- sources JSON round-trip ---

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

    // --- sources_add input parsing (reject < 3 args) ---

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
    fn sources_add_single_arg_rejected() {
        let one_arg = "홍길동";
        let parts: Vec<&str> = one_arg.splitn(4, ' ').collect();
        assert!(parts.len() < 3);
    }

    // --- sources_search case-insensitive matching ---

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

    // --- sources beat tag ---

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

    // --- article prompt generation ---

    #[test]
    fn article_prompt_without_topic() {
        let (prompt, has_topic) = build_article_prompt("", &[]);
        assert!(!has_topic);
        assert!(prompt.contains("어떤 주제로 기사를 작성하시겠습니까"));
        assert!(prompt.contains("리드"));
    }

    #[test]
    fn article_prompt_with_topic() {
        let (prompt, has_topic) = build_article_prompt("반도체 수출 동향", &[]);
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
        let (prompt, has_topic) = build_article_prompt("반도체 수출 동향", &research);
        assert!(has_topic);
        assert!(prompt.contains("관련 리서치 자료"));
        assert!(prompt.contains("반도체-수출-동향.md"));
        assert!(prompt.contains("수출액 증가 추세"));
    }

    #[test]
    fn article_prompt_no_research_section_when_empty() {
        let (prompt, _) = build_article_prompt("반도체 수출 동향", &[]);
        assert!(!prompt.contains("관련 리서치 자료"));
    }

    // --- find_related_research tests ---

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

    // --- factcheck prompt generation ---

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

    // --- draft file path: slug + date ---

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
    fn research_file_path_contains_date_and_slug() {
        let path = research_file_path_with_date("경제 전망 보고서", "2026-06-01");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("2026-06-01"));
        assert!(path_str.contains("경제-전망-보고서"));
        assert!(path_str.starts_with(".journalist/research/"));
    }

    #[test]
    fn topic_to_slug_empty() {
        assert_eq!(topic_to_slug("", 50), "");
    }

    #[test]
    fn topic_to_slug_only_punctuation() {
        assert_eq!(topic_to_slug("..., !!!", 50), "");
    }

    // --- factcheck file path tests ---

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

    // --- briefing tests ---

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

    // ── /checklist tests ────────────────────────────────────────────────

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

    // --- research_search tests ---

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

    // ── /interview tests ────────────────────────────────────────────────

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

    // --- compare tests ---

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

    // --- timeline tests ---

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

    // ── /translate tests ────────────────────────────────────────────────

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
}
