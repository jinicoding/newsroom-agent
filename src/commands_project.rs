//! Project-related command handlers: /context, /init, /health, /fix, /test, /lint,
//! /tree, /run, /docs, /find, /index.

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
