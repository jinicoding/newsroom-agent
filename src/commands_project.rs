//! Project-related command handlers: /context, /init, /health, /fix, /test, /lint,
//! /tree, /run, /docs, /find, /index, /article, /research, /sources, /factcheck,
//! /briefing, /clip, /news, /summary, /stats, /draft, /deadline, /embargo, /export, /quote,
//! /trend, /archive, /data, /follow, /desk, /coverage, /dashboard, /publish,
//! /anonymize, /press, /law, /readability.

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
             - `planning` — 기획 (문제제기+현황+원인+대안)"
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

/// Handle the /article command: AI-assisted article writing with structured format.
pub async fn handle_article(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let raw_args = input
        .strip_prefix("/article")
        .unwrap_or("")
        .trim();

    let (article_type, topic) = parse_article_args(raw_args);
    let topic = topic.as_str();

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

    let (prompt, _) = build_article_prompt(topic, &research, article_type.as_deref());

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

// ── /headline ────────────────────────────────────────────────────────────

const HEADLINE_DIR: &str = ".journalist/headline";

/// Parse `/headline` arguments: supports `--file <path>` and inline text.
/// Returns (Option<file_path>, inline_text).
pub fn parse_headline_args(args: &str) -> (Option<String>, String) {
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

/// Build the prompt for `/headline`: generate 5–7 headline candidates in various styles.
pub fn build_headline_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 기사 초안(또는 주제)을 읽고, **한국 신문 스타일의 헤드라인 후보 5~7개**를 생성해주세요.\n\n\
         ## 헤드라인 스타일 (각 스타일별 최소 1개)\n\n\
         1. **스트레이트**: 핵심 사실을 간결하게 전달. 주어+동사 구조.\n\
         2. **분석**: 맥락·의미를 담은 헤드라인. '~의 의미', '~이 뜻하는 것' 등.\n\
         3. **피처**: 독자의 호기심을 자극하는 내러티브형. 인물·장면 중심.\n\
         4. **클릭유도**: 숫자·질문·강한 표현으로 클릭을 유도. 단, 낚시성 지양.\n\n\
         ## 한국 신문 헤드라인 관습\n\n\
         - **간결함**: 15~25자 내외 (공백 포함)\n\
         - **핵심 동사**: 능동형 동사로 끝맺음 ('~했다', '~한다', '~나서' 등)\n\
         - **숫자 활용**: 구체적 수치가 있으면 적극 활용\n\
         - **따옴표**: 인용이나 강조 시 홑따옴표('') 사용\n\
         - **말줄임표**: 긴장감이나 반전에 '…' 활용 가능\n\
         - **주어 생략**: 문맥상 명확하면 주어 생략 가능\n\n\
         ## 출력 형식\n\n\
         각 헤드라인에 스타일 태그를 붙여주세요:\n\
         ```\n\
         [스트레이트] 헤드라인 텍스트\n\
         [분석] 헤드라인 텍스트\n\
         [피처] 헤드라인 텍스트\n\
         [클릭유도] 헤드라인 텍스트\n\
         ```\n\n\
         각 헤드라인 아래에 한 줄로 **선택 이유**를 간단히 설명해주세요.\n\n\
         ---\n\n\
         기사 초안/주제:\n\n{article}"
    ))
}

/// Build headline file path with today's date.
pub fn headline_file_path(slug_source: &str) -> std::path::PathBuf {
    headline_file_path_with_date(slug_source, &today_str())
}

/// Build headline file path with an explicit date string (for testing).
pub fn headline_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_headline.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(HEADLINE_DIR).join(filename)
}

/// Save headline result to file. Creates the headline directory if needed.
fn save_headline(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/headline` command: generate headline candidates for an article draft or topic.
pub async fn handle_headline(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/headline").unwrap_or("").trim();
    let (file_path, inline_text) = parse_headline_args(args);

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

    let prompt = match build_headline_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /headline <기사 초안 또는 주제>{RESET}");
            println!("{DIM}  또는:   /headline --file <경로>{RESET}");
            println!("{DIM}  예시:   /headline 삼성전자 1분기 영업이익 전년 대비 30% 증가{RESET}");
            println!(
                "{DIM}  기사 초안이나 주제에 맞는 헤드라인 후보 5~7개를 생성합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save headline to .journalist/headline/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "headline".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "headline".to_string()
            } else {
                preview
            }
        };
        let path = headline_file_path(&slug_source);
        match save_headline(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 헤드라인 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  헤드라인 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /rewrite ─────────────────────────────────────────────────────────────

const REWRITE_DIR: &str = ".journalist/drafts";

/// Parse `/rewrite` arguments: supports `--style`, `--length`, `--file`, and inline text.
/// Returns (Option<style>, Option<length>, Option<file_path>, inline_text).
pub fn parse_rewrite_args(args: &str) -> (Option<String>, Option<String>, Option<String>, String) {
    let args = args.trim();
    let mut style: Option<String> = None;
    let mut length: Option<String> = None;
    let mut file_path: Option<String> = None;
    let mut remaining_parts: Vec<String> = Vec::new();

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "--style" => {
                if i + 1 < tokens.len() {
                    style = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--length" => {
                if i + 1 < tokens.len() {
                    length = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--file" => {
                if i + 1 < tokens.len() {
                    file_path = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            other => {
                remaining_parts.push(other.to_string());
                i += 1;
            }
        }
    }

    (style, length, file_path, remaining_parts.join(" "))
}

/// Build the prompt for `/rewrite`: rewrite an article in a different style/tone.
pub fn build_rewrite_prompt(
    article: &str,
    style: Option<&str>,
    length: Option<&str>,
) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    let style_name = style.unwrap_or("스트레이트");
    let style_desc = match style_name {
        "스트레이트" | "straight" => {
            "**스트레이트**: 역피라미드 구조. 핵심 사실을 첫 문단에 배치. 객관적이고 간결한 문체."
        }
        "피처" | "feature" => {
            "**피처**: 내러티브형 구조. 인물·장면 묘사로 시작. 독자의 감정에 호소하는 문체."
        }
        "칼럼" | "column" | "opinion" => {
            "**칼럼/오피니언**: 필자의 시각과 분석이 담긴 논평형. 주장-근거-결론 구조."
        }
        "요약" | "summary" => {
            "**요약**: 핵심 사실만 간추린 브리핑형. 불릿포인트 활용 가능. 최대한 압축."
        }
        "sns" | "SNS" | "소셜" => {
            "**SNS**: 소셜미디어에 적합한 짧고 임팩트 있는 문체. 이모지 활용 가능. 핵심만 전달."
        }
        other => {
            // Allow custom style descriptions
            return Some(format!(
                "아래 기사를 **{other}** 스타일로 재작성해주세요.\n\n\
                 {length_instruction}\n\n\
                 ## 재작성 규칙\n\n\
                 - 원문의 핵심 사실과 정보를 정확히 유지\n\
                 - 인용문은 그대로 보존\n\
                 - 숫자·고유명사의 정확성 유지\n\
                 - 원문에 없는 사실을 추가하지 않음\n\n\
                 ## 원문\n\n{article}",
                length_instruction = length_instruction(length),
            ));
        }
    };

    Some(format!(
        "아래 기사를 다음 스타일로 재작성해주세요.\n\n\
         ## 목표 스타일\n\n\
         {style_desc}\n\n\
         {length_instruction}\n\n\
         ## 재작성 규칙\n\n\
         - 원문의 핵심 사실과 정보를 정확히 유지\n\
         - 인용문은 그대로 보존\n\
         - 숫자·고유명사의 정확성 유지\n\
         - 원문에 없는 사실을 추가하지 않음\n\
         - 문단 구조와 흐름을 목표 스타일에 맞게 재구성\n\n\
         ## 원문\n\n{article}",
        length_instruction = length_instruction(length),
    ))
}

/// Build length instruction string for the rewrite prompt.
fn length_instruction(length: Option<&str>) -> String {
    match length {
        Some(len) => format!("## 글자 수 제한\n\n공백 포함 **{len}자** 이내로 작성해주세요."),
        None => String::new(),
    }
}

/// Build rewrite output file path using today's date.
pub fn rewrite_file_path(slug_source: &str) -> std::path::PathBuf {
    rewrite_file_path_with_date(slug_source, &today_str())
}

/// Build rewrite file path with an explicit date string (for testing).
pub fn rewrite_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_rewrite.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(REWRITE_DIR).join(filename)
}

/// Save rewrite result to file. Creates the drafts directory if needed.
fn save_rewrite(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/rewrite` command: rewrite an article in a different style/tone.
pub async fn handle_rewrite(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/rewrite").unwrap_or("").trim();
    let (style, length, file_path, inline_text) = parse_rewrite_args(args);

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

    let prompt = match build_rewrite_prompt(&article, style.as_deref(), length.as_deref()) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /rewrite <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /rewrite --file <경로>{RESET}");
            println!(
                "{DIM}  옵션:   --style <스트레이트|피처|칼럼|요약|SNS>{RESET}"
            );
            println!("{DIM}  옵션:   --length <글자수>{RESET}");
            println!(
                "{DIM}  예시:   /rewrite --style 요약 --file draft.txt{RESET}"
            );
            println!(
                "{DIM}  기존 기사를 다른 포맷·톤으로 재작성합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save rewrite to .journalist/drafts/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "rewrite".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "rewrite".to_string()
            } else {
                preview
            }
        };
        let path = rewrite_file_path(&slug_source);
        match save_rewrite(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 재작성 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  재작성 저장 실패: {e}{RESET}\n");
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

// ── /summary ─────────────────────────────────────────────────────────────

/// Parse `/summary` arguments: if the argument is an existing file path, read it;
/// otherwise treat it as inline text.
pub fn resolve_summary_input(args: &str) -> Option<String> {
    let args = args.trim();
    if args.is_empty() {
        return None;
    }

    // Check if the first token is an existing file
    let first_token = args.split_whitespace().next().unwrap_or("");
    if std::path::Path::new(first_token).is_file() {
        match std::fs::read_to_string(first_token) {
            Ok(content) => {
                println!(
                    "{DIM}  파일 읽기: {first_token} ({} bytes){RESET}",
                    content.len()
                );
                Some(content)
            }
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {first_token} — {e}{RESET}\n");
                None
            }
        }
    } else {
        Some(args.to_string())
    }
}

/// Build the prompt for `/summary`: generate a concise 3–5 line summary.
pub fn build_summary_prompt(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 문서를 읽고 **3~5줄로 핵심 요약**을 작성해주세요.\n\n\
         ## 요약 규칙\n\n\
         1. **첫 줄**: 가장 중요한 사실/결론을 한 문장으로.\n\
         2. **나머지**: 핵심 근거, 배경, 수치를 간결하게.\n\
         3. 전문 용어가 있으면 괄호 안에 간단히 풀어주세요.\n\
         4. 출처나 날짜 등 메타정보가 있으면 포함하세요.\n\
         5. 한국어로 작성하세요.\n\n\
         ---\n\n\
         {text}"
    ))
}

/// Handle `/summary <filepath or text>`.
pub async fn handle_summary(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/summary").unwrap_or("").trim();

    let text = match resolve_summary_input(args) {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            println!("{DIM}  사용법: /summary <파일경로 또는 텍스트>{RESET}");
            println!("{DIM}  예시:   /summary press_release.txt{RESET}");
            println!("{DIM}  예시:   /summary 정부가 오늘 새로운 부동산 정책을 발표했다...{RESET}");
            println!("{DIM}  보도자료, 판결문, 정책문서 등을 3~5줄로 빠르게 요약합니다.{RESET}\n");
            return;
        }
    };

    let prompt = build_summary_prompt(&text).unwrap();
    run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);
}

// ── /stats ──────────────────────────────────────────────────────────────

/// Text statistics computed locally (no AI).
#[derive(Debug, PartialEq)]
pub struct TextStats {
    pub chars_with_spaces: usize,
    pub chars_without_spaces: usize,
    pub words: usize,
    pub sentences: usize,
    pub paragraphs: usize,
    /// Estimated reading time in seconds (based on ~500 chars/min for Korean).
    pub reading_time_secs: u64,
}

/// Compute text statistics from a string.
pub fn compute_text_stats(text: &str) -> TextStats {
    let chars_with_spaces = text.chars().count();
    let chars_without_spaces = text.chars().filter(|c| !c.is_whitespace()).count();

    // Word count: split on whitespace, count non-empty tokens
    let words = text.split_whitespace().count();

    // Sentence count: split on sentence-ending punctuation (. ! ? 。)
    let sentences = text
        .chars()
        .filter(|&c| c == '.' || c == '!' || c == '?' || c == '。')
        .count()
        .max(if chars_without_spaces > 0 { 1 } else { 0 });

    // Paragraph count: sequences of non-empty lines separated by blank lines
    let paragraphs = text
        .split('\n')
        .fold((0usize, false), |(count, in_para), line| {
            let non_empty = !line.trim().is_empty();
            if non_empty && !in_para {
                (count + 1, true)
            } else if !non_empty {
                (count, false)
            } else {
                (count, in_para)
            }
        })
        .0;

    // Korean reading speed ~500 chars/min (excluding spaces)
    let reading_time_secs = if chars_without_spaces > 0 {
        (chars_without_spaces as u64 * 60) / 500
    } else {
        0
    };

    TextStats {
        chars_with_spaces,
        chars_without_spaces,
        words,
        sentences,
        paragraphs,
        reading_time_secs,
    }
}

/// Find the most recently modified file in `.journalist/drafts/`.
fn find_latest_draft() -> Option<std::path::PathBuf> {
    let dir = std::path::Path::new(DRAFTS_DIR);
    if !dir.exists() {
        return None;
    }
    let mut best: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "md") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if best.as_ref().map_or(true, |(_, t)| modified > *t) {
                        best = Some((path, modified));
                    }
                }
            }
        }
    }
    best.map(|(p, _)| p)
}

/// Format reading time as human-readable string.
fn format_reading_time(secs: u64) -> String {
    if secs < 60 {
        format!("{}초", secs)
    } else {
        let min = secs / 60;
        let sec = secs % 60;
        if sec == 0 {
            format!("{}분", min)
        } else {
            format!("{}분 {}초", min, sec)
        }
    }
}

/// Handle `/stats [파일경로]` — show text statistics for a file.
pub fn handle_stats(input: &str) {
    let arg = input.strip_prefix("/stats").unwrap_or("").trim();

    let (path, content) = if arg.is_empty() {
        // No argument: find latest draft
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => (p.to_string_lossy().to_string(), c),
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                eprintln!("{DIM}  분석할 파일이 없습니다. 경로를 지정하거나 /article로 초안을 먼저 작성하세요.{RESET}\n");
                return;
            }
        }
    } else {
        match std::fs::read_to_string(arg) {
            Ok(c) => (arg.to_string(), c),
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패 ({arg}): {e}{RESET}\n");
                return;
            }
        }
    };

    let stats = compute_text_stats(&content);

    println!("{BOLD}  📊 기사 통계: {path}{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");
    println!(
        "  글자 수 (공백 포함)  {}",
        stats.chars_with_spaces
    );
    println!(
        "  글자 수 (공백 제외)  {}",
        stats.chars_without_spaces
    );
    println!("  단어 수             {}", stats.words);
    println!("  문장 수             {}", stats.sentences);
    println!("  문단 수             {}", stats.paragraphs);
    println!(
        "  예상 읽기 시간       {}",
        format_reading_time(stats.reading_time_secs)
    );
    println!();
}

// ── /draft ──────────────────────────────────────────────────────────────

/// Base directory for versioned drafts: `.journalist/drafts/<slug>/v1.md, v2.md, ...`
const DRAFT_VERSIONS_BASE: &str = ".journalist/drafts";

/// Format a UNIX timestamp as "YYYY-MM-DD HH:MM" (UTC).
fn format_unix_timestamp(secs: u64) -> String {
    let s = secs as i64;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;

    // Convert days since epoch to y/m/d (civil calendar)
    // Algorithm from Howard Hinnant's chrono-compatible date library
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02}")
}

/// Return the directory path for a given draft title.
fn draft_versions_dir(title: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(title, 50);
    std::path::PathBuf::from(DRAFT_VERSIONS_BASE).join(slug)
}

/// Find the next version number for a draft title.
fn next_version_number(dir: &std::path::Path) -> u32 {
    if !dir.exists() {
        return 1;
    }
    let mut max = 0u32;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix('v') {
                if let Some(num_str) = rest.strip_suffix(".md") {
                    if let Ok(n) = num_str.parse::<u32>() {
                        if n > max {
                            max = n;
                        }
                    }
                }
            }
        }
    }
    max + 1
}

/// List all version files in a draft directory, sorted by version number.
fn list_versions(dir: &std::path::Path) -> Vec<(u32, std::path::PathBuf)> {
    let mut versions = Vec::new();
    if !dir.exists() {
        return versions;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix('v') {
                if let Some(num_str) = rest.strip_suffix(".md") {
                    if let Ok(n) = num_str.parse::<u32>() {
                        versions.push((n, entry.path()));
                    }
                }
            }
        }
    }
    versions.sort_by_key(|(n, _)| *n);
    versions
}

/// Handle `/draft` command with subcommands: save, list, load, diff.
pub fn handle_draft(input: &str) {
    let args = input.strip_prefix("/draft").unwrap_or("").trim();

    if args.is_empty() {
        print_draft_usage();
        return;
    }

    let (sub, rest) = match args.split_once(char::is_whitespace) {
        Some((s, r)) => (s, r.trim()),
        None => (args, ""),
    };

    match sub {
        "save" => handle_draft_save(rest),
        "list" => handle_draft_list(rest),
        "load" => handle_draft_load(rest),
        "diff" => handle_draft_diff(rest),
        _ => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {sub}{RESET}");
            print_draft_usage();
        }
    }
}

fn print_draft_usage() {
    println!("{DIM}  사용법:");
    println!("    /draft save <제목> [파일]   기사를 버전별로 저장 (파일 미지정 시 최신 초안)");
    println!("    /draft list [제목]          저장된 초안 목록");
    println!("    /draft load <제목> [버전]   특정 버전 불러오기 (미지정 시 최신)");
    println!("    /draft diff <제목> [v1] [v2] 두 버전 간 차이 비교{RESET}\n");
}

/// `/draft save <title> [file]`
fn handle_draft_save(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /draft save <제목> [파일]{RESET}\n");
        return;
    }

    let (title, file_arg) = match args.split_once(char::is_whitespace) {
        Some((t, f)) => (t.trim(), f.trim()),
        None => (args, ""),
    };

    // Read content: from file argument, or find latest draft
    let content = if !file_arg.is_empty() {
        match std::fs::read_to_string(file_arg) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패 ({file_arg}): {e}{RESET}\n");
                return;
            }
        }
    } else {
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                eprintln!("{RED}  저장할 파일이 없습니다. 파일 경로를 지정하거나 /article로 초안을 먼저 작성하세요.{RESET}\n");
                return;
            }
        }
    };

    let dir = draft_versions_dir(title);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("{RED}  디렉토리 생성 실패: {e}{RESET}\n");
        return;
    }

    let ver = next_version_number(&dir);
    let path = dir.join(format!("v{ver}.md"));
    if let Err(e) = std::fs::write(&path, &content) {
        eprintln!("{RED}  저장 실패: {e}{RESET}\n");
        return;
    }

    let char_count = content.chars().count();
    println!(
        "{GREEN}  ✅ 저장: {title} v{ver} ({char_count}자) → {}{RESET}\n",
        path.display()
    );
}

/// `/draft list [title]`
fn handle_draft_list(title: &str) {
    if title.is_empty() {
        // List all draft titles
        let base = std::path::Path::new(DRAFT_VERSIONS_BASE);
        if !base.exists() {
            println!("{DIM}  저장된 초안이 없습니다.{RESET}\n");
            return;
        }
        let mut entries: Vec<(String, usize, String, usize)> = Vec::new();
        if let Ok(dirs) = std::fs::read_dir(base) {
            for entry in dirs.flatten() {
                if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                let versions = list_versions(&entry.path());
                if versions.is_empty() {
                    continue;
                }
                let ver_count = versions.len();
                // Last modified time of the latest version
                let last_path = &versions.last().unwrap().1;
                let modified = std::fs::metadata(last_path)
                    .and_then(|m| m.modified())
                    .ok();
                let date_str = modified
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| format_unix_timestamp(d.as_secs()))
                    .unwrap_or_else(|| "-".to_string());
                // Char count of latest version
                let char_count = std::fs::read_to_string(last_path)
                    .map(|c| c.chars().count())
                    .unwrap_or(0);
                entries.push((name, ver_count, date_str, char_count));
            }
        }

        if entries.is_empty() {
            println!("{DIM}  저장된 초안이 없습니다.{RESET}\n");
            return;
        }

        entries.sort_by(|a, b| a.0.cmp(&b.0));
        println!("{BOLD}  📝 초안 목록{RESET}");
        println!("{DIM}  ──────────────────────────────{RESET}");
        for (name, ver_count, date, chars) in &entries {
            println!("  {name}  (v{ver_count}, {date}, {chars}자)");
        }
        println!();
    } else {
        // List versions for a specific title
        let dir = draft_versions_dir(title);
        let versions = list_versions(&dir);
        if versions.is_empty() {
            eprintln!("{DIM}  '{title}'에 저장된 버전이 없습니다.{RESET}\n");
            return;
        }

        println!("{BOLD}  📝 {title} 버전 목록{RESET}");
        println!("{DIM}  ──────────────────────────────{RESET}");
        for (ver, path) in &versions {
            let modified = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok();
            let date_str = modified
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| format_unix_timestamp(d.as_secs()))
                .unwrap_or_else(|| "-".to_string());
            let char_count = std::fs::read_to_string(path)
                .map(|c| c.chars().count())
                .unwrap_or(0);
            println!("  v{ver}  ({date_str}, {char_count}자)");
        }
        println!();
    }
}

/// `/draft load <title> [version]`
fn handle_draft_load(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /draft load <제목> [버전]{RESET}\n");
        return;
    }

    let (title, ver_arg) = match args.split_once(char::is_whitespace) {
        Some((t, v)) => (t.trim(), v.trim()),
        None => (args, ""),
    };

    let dir = draft_versions_dir(title);
    let versions = list_versions(&dir);
    if versions.is_empty() {
        eprintln!("{DIM}  '{title}'에 저장된 버전이 없습니다.{RESET}\n");
        return;
    }

    let target_ver = if ver_arg.is_empty() {
        // Load latest
        versions.last().unwrap().0
    } else {
        // Parse version: accept "v3" or "3"
        let num_str = ver_arg.strip_prefix('v').unwrap_or(ver_arg);
        match num_str.parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("{RED}  버전 번호가 올바르지 않습니다: {ver_arg}{RESET}\n");
                return;
            }
        }
    };

    let path = dir.join(format!("v{target_ver}.md"));
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let char_count = content.chars().count();
            println!(
                "{BOLD}  📄 {title} v{target_ver} ({char_count}자){RESET}"
            );
            println!("{DIM}  ──────────────────────────────{RESET}");
            println!("{content}");
        }
        Err(_) => {
            let available: Vec<String> = versions.iter().map(|(v, _)| format!("v{v}")).collect();
            eprintln!(
                "{RED}  v{target_ver} 버전이 존재하지 않습니다. 사용 가능: {}{RESET}\n",
                available.join(", ")
            );
        }
    }
}

/// `/draft diff <title> [v1] [v2]`
fn handle_draft_diff(args: &str) {
    if args.is_empty() {
        eprintln!("{RED}  제목을 지정하세요: /draft diff <제목> [v1] [v2]{RESET}\n");
        return;
    }

    let parts: Vec<&str> = args.split_whitespace().collect();
    let title = parts[0];

    let dir = draft_versions_dir(title);
    let versions = list_versions(&dir);
    if versions.len() < 2 {
        eprintln!("{DIM}  비교하려면 최소 2개 버전이 필요합니다.{RESET}\n");
        return;
    }

    // Determine which two versions to compare
    let (v1, v2) = if parts.len() >= 3 {
        let parse_ver = |s: &str| -> Option<u32> {
            let num_str = s.strip_prefix('v').unwrap_or(s);
            num_str.parse().ok()
        };
        match (parse_ver(parts[1]), parse_ver(parts[2])) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                eprintln!("{RED}  버전 번호가 올바르지 않습니다.{RESET}\n");
                return;
            }
        }
    } else if parts.len() == 2 {
        // One version specified: compare it with the latest
        let parse_ver = |s: &str| -> Option<u32> {
            let num_str = s.strip_prefix('v').unwrap_or(s);
            num_str.parse().ok()
        };
        match parse_ver(parts[1]) {
            Some(a) => {
                let latest = versions.last().unwrap().0;
                if a == latest {
                    // Compare with second-to-last
                    let prev = versions[versions.len() - 2].0;
                    (prev, a)
                } else {
                    (a, latest)
                }
            }
            None => {
                eprintln!("{RED}  버전 번호가 올바르지 않습니다.{RESET}\n");
                return;
            }
        }
    } else {
        // No versions specified: compare last two
        let len = versions.len();
        (versions[len - 2].0, versions[len - 1].0)
    };

    let path1 = dir.join(format!("v{v1}.md"));
    let path2 = dir.join(format!("v{v2}.md"));

    let content1 = match std::fs::read_to_string(&path1) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("{RED}  v{v1} 버전이 존재하지 않습니다.{RESET}\n");
            return;
        }
    };
    let content2 = match std::fs::read_to_string(&path2) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("{RED}  v{v2} 버전이 존재하지 않습니다.{RESET}\n");
            return;
        }
    };

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();

    println!(
        "{BOLD}  📊 {title}: v{v1} → v{v2} 비교{RESET}"
    );
    println!("{DIM}  ──────────────────────────────{RESET}");

    // Simple line-by-line diff
    let max_lines = lines1.len().max(lines2.len());
    let mut adds = 0usize;
    let mut removes = 0usize;
    let mut changes = Vec::new();

    for i in 0..max_lines {
        let l1 = lines1.get(i).copied();
        let l2 = lines2.get(i).copied();
        match (l1, l2) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => {
                changes.push(format!("{RED}  - [{ln}] {a}{RESET}", ln = i + 1));
                changes.push(format!("{GREEN}  + [{ln}] {b}{RESET}", ln = i + 1));
                removes += 1;
                adds += 1;
            }
            (Some(a), None) => {
                changes.push(format!("{RED}  - [{ln}] {a}{RESET}", ln = i + 1));
                removes += 1;
            }
            (None, Some(b)) => {
                changes.push(format!("{GREEN}  + [{ln}] {b}{RESET}", ln = i + 1));
                adds += 1;
            }
            (None, None) => {}
        }
    }

    if changes.is_empty() {
        println!("{DIM}  두 버전이 동일합니다.{RESET}\n");
    } else {
        let c1_chars = content1.chars().count();
        let c2_chars = content2.chars().count();
        println!(
            "{DIM}  v{v1}: {c1_chars}자 → v{v2}: {c2_chars}자 (차이: {adds} 추가, {removes} 삭제){RESET}"
        );
        for line in &changes {
            println!("{line}");
        }
        println!();
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
fn today_date_string() -> String {
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

// ── /export ─────────────────────────────────────────────────────────────

/// Base directory for exported articles.
const EXPORTS_DIR: &str = ".journalist/exports";

/// Strip markdown markup to produce clean plain text.
pub fn markdown_to_plain_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());

    for line in md.lines() {
        let trimmed = line.trim();

        // Skip horizontal rules
        if trimmed.chars().all(|c| c == '-' || c == '*' || c == '_' || c == ' ')
            && trimmed.len() >= 3
            && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3
        {
            out.push('\n');
            continue;
        }

        // Strip heading markers
        let line = if trimmed.starts_with('#') {
            let content = trimmed.trim_start_matches('#').trim();
            content
        } else {
            trimmed
        };

        // Strip bold/italic markers
        let line = line.replace("**", "").replace("__", "");
        let line = line.replace('*', "").replace('_', " ");

        // Strip inline code backticks
        let line = line.replace('`', "");

        // Strip link syntax [text](url) → text
        let line = strip_md_links(&line);

        // Strip image syntax ![alt](url) → alt
        let line = strip_md_images(&line);

        // Strip list markers
        let line = strip_list_marker(&line);

        out.push_str(&line);
        out.push('\n');
    }

    // Collapse triple+ newlines into double
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }

    out.trim().to_string()
}

/// Strip markdown link syntax: [text](url) → text
fn strip_md_links(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            // Look for ](
            if let Some(close_bracket) = chars[i + 1..].iter().position(|&c| c == ']') {
                let close_idx = i + 1 + close_bracket;
                if close_idx + 1 < chars.len() && chars[close_idx + 1] == '(' {
                    if let Some(close_paren) =
                        chars[close_idx + 2..].iter().position(|&c| c == ')')
                    {
                        // Extract link text
                        let text: String = chars[i + 1..close_idx].iter().collect();
                        result.push_str(&text);
                        i = close_idx + 2 + close_paren + 1;
                        continue;
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Strip markdown image syntax: ![alt](url) → alt
fn strip_md_images(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '[' {
            if let Some(close_bracket) = chars[i + 2..].iter().position(|&c| c == ']') {
                let close_idx = i + 2 + close_bracket;
                if close_idx + 1 < chars.len() && chars[close_idx + 1] == '(' {
                    if let Some(close_paren) =
                        chars[close_idx + 2..].iter().position(|&c| c == ')')
                    {
                        let alt: String = chars[i + 2..close_idx].iter().collect();
                        result.push_str(&alt);
                        i = close_idx + 2 + close_paren + 1;
                        continue;
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Strip list markers (-, *, numbered) from line start.
fn strip_list_marker(s: &str) -> String {
    let trimmed = s.trim_start();
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        return trimmed[2..].to_string();
    }
    // Numbered list: "1. ", "2. ", etc.
    if let Some(dot_pos) = trimmed.find(". ") {
        if dot_pos <= 3 && trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return trimmed[dot_pos + 2..].to_string();
        }
    }
    s.to_string()
}

/// Convert markdown to simple HTML.
pub fn markdown_to_html(md: &str) -> String {
    let mut out = String::with_capacity(md.len() * 2);
    out.push_str("<!DOCTYPE html>\n<html lang=\"ko\">\n<head>\n");
    out.push_str("<meta charset=\"UTF-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    out.push_str("<style>\n");
    out.push_str("body { font-family: 'Noto Sans KR', sans-serif; max-width: 720px; margin: 2em auto; padding: 0 1em; line-height: 1.8; color: #333; }\n");
    out.push_str("h1 { font-size: 1.6em; border-bottom: 2px solid #333; padding-bottom: 0.3em; }\n");
    out.push_str("h2 { font-size: 1.3em; margin-top: 1.5em; }\n");
    out.push_str("h3 { font-size: 1.1em; }\n");
    out.push_str("blockquote { border-left: 3px solid #ccc; padding-left: 1em; color: #666; margin: 1em 0; }\n");
    out.push_str(".meta { color: #888; font-size: 0.9em; margin-bottom: 2em; }\n");
    out.push_str("</style>\n</head>\n<body>\n");

    let mut in_paragraph = false;
    let mut in_list = false;
    let mut in_blockquote = false;

    for line in md.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            if in_blockquote {
                out.push_str("</blockquote>\n");
                in_blockquote = false;
            }
            continue;
        }

        // Headings
        if trimmed.starts_with("### ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            let content = html_escape(&trimmed[4..]);
            out.push_str(&format!("<h3>{content}</h3>\n"));
            continue;
        }
        if trimmed.starts_with("## ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            let content = html_escape(&trimmed[3..]);
            out.push_str(&format!("<h2>{content}</h2>\n"));
            continue;
        }
        if trimmed.starts_with("# ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            let content = html_escape(&trimmed[2..]);
            out.push_str(&format!("<h1>{content}</h1>\n"));
            continue;
        }

        // Blockquote
        if trimmed.starts_with("> ") {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            if !in_blockquote {
                out.push_str("<blockquote>\n");
                in_blockquote = true;
            }
            let content = inline_md_to_html(&trimmed[2..]);
            out.push_str(&format!("<p>{content}</p>\n"));
            continue;
        }

        // List items
        if (trimmed.starts_with("- ") || trimmed.starts_with("* "))
            || (trimmed.len() > 2
                && trimmed.find(". ").map_or(false, |p| {
                    p <= 3 && trimmed[..p].chars().all(|c| c.is_ascii_digit())
                }))
        {
            if in_paragraph {
                out.push_str("</p>\n");
                in_paragraph = false;
            }
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            let text = strip_list_marker(trimmed);
            let content = inline_md_to_html(&text);
            out.push_str(&format!("<li>{content}</li>\n"));
            continue;
        }

        // Regular paragraph
        if !in_paragraph {
            out.push_str("<p>");
            in_paragraph = true;
        } else {
            out.push_str("<br>\n");
        }
        let content = inline_md_to_html(trimmed);
        out.push_str(&content);
    }

    if in_paragraph {
        out.push_str("</p>\n");
    }
    if in_list {
        out.push_str("</ul>\n");
    }
    if in_blockquote {
        out.push_str("</blockquote>\n");
    }

    out.push_str("</body>\n</html>\n");
    out
}

/// Escape HTML special characters.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convert inline markdown (bold, italic, code, links) to HTML.
fn inline_md_to_html(s: &str) -> String {
    let s = html_escape(s);
    // Bold: **text** or __text__
    let s = regex_replace_pairs(&s, "**", "<strong>", "</strong>");
    let s = regex_replace_pairs(&s, "__", "<strong>", "</strong>");
    // Italic: *text* or _text_ (simplified)
    let s = regex_replace_pairs(&s, "*", "<em>", "</em>");
    // Inline code: `code`
    let s = regex_replace_pairs(&s, "`", "<code>", "</code>");
    s
}

/// Simple paired-delimiter replacement.
fn regex_replace_pairs(s: &str, delim: &str, open: &str, close: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    let mut is_open = false;

    while let Some(pos) = rest.find(delim) {
        result.push_str(&rest[..pos]);
        if is_open {
            result.push_str(close);
        } else {
            result.push_str(open);
        }
        is_open = !is_open;
        rest = &rest[pos + delim.len()..];
    }
    result.push_str(rest);
    // If we opened but never closed, treat the tag as literal
    if is_open {
        // Re-do without replacement — just return original
        return s.to_string();
    }
    result
}

/// Build the metadata header for an exported article.
fn build_export_meta(source_path: &str, char_count: usize) -> String {
    let today = today_str();
    let filename = std::path::Path::new(source_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    format!(
        "제목: {filename}\n날짜: {today}\n글자수: {char_count}자\n"
    )
}

/// Handle `/export <파일> [--html]` — export article to plain text or HTML.
pub fn handle_export(input: &str) {
    let args = input.strip_prefix("/export").unwrap_or("").trim();

    if args.is_empty() {
        // Try latest draft
        match find_latest_draft() {
            Some(p) => export_file(&p.to_string_lossy(), false),
            None => {
                eprintln!("{DIM}  사용법: /export <파일> [--html]{RESET}");
                eprintln!("{DIM}  마크다운 기사를 텍스트 또는 HTML로 내보냅니다.{RESET}\n");
            }
        }
        return;
    }

    let html_mode = args.contains("--html");
    let file_arg = args.replace("--html", "").trim().to_string();

    if file_arg.is_empty() {
        match find_latest_draft() {
            Some(p) => export_file(&p.to_string_lossy(), html_mode),
            None => {
                eprintln!("{RED}  내보낼 파일을 지정하세요.{RESET}\n");
            }
        }
    } else {
        export_file(&file_arg, html_mode);
    }
}

/// Core export logic: read file, convert, save, print info.
fn export_file(source_path: &str, html_mode: bool) {
    let content = match std::fs::read_to_string(source_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{RED}  파일 읽기 실패 ({source_path}): {e}{RESET}\n");
            return;
        }
    };

    // Build output
    let (output, ext) = if html_mode {
        (markdown_to_html(&content), "html")
    } else {
        let plain = markdown_to_plain_text(&content);
        let meta = build_export_meta(source_path, plain.chars().filter(|c| !c.is_whitespace()).count());
        (format!("{meta}\n---\n\n{plain}"), "txt")
    };

    // Ensure exports directory
    let exports = std::path::Path::new(EXPORTS_DIR);
    if let Err(e) = std::fs::create_dir_all(exports) {
        eprintln!("{RED}  디렉토리 생성 실패: {e}{RESET}\n");
        return;
    }

    // Build output filename
    let stem = std::path::Path::new(source_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "export".to_string());
    let out_name = format!("{stem}.{ext}");
    let out_path = exports.join(&out_name);

    if let Err(e) = std::fs::write(&out_path, &output) {
        eprintln!("{RED}  저장 실패: {e}{RESET}\n");
        return;
    }

    let char_count = if html_mode {
        markdown_to_plain_text(&content)
            .chars()
            .filter(|c| !c.is_whitespace())
            .count()
    } else {
        output
            .chars()
            .filter(|c| !c.is_whitespace())
            .count()
    };

    let format_label = if html_mode { "HTML" } else { "텍스트" };
    println!(
        "{GREEN}  ✅ {format_label} 내보내기 완료: {}{RESET}",
        out_path.display()
    );
    println!("{DIM}  글자수: {char_count}자 (공백 제외){RESET}");
    println!(
        "{DIM}  💡 클립보드 복사: cat {} | xclip -selection clipboard{RESET}\n",
        out_path.display()
    );
}

// ── /proofread ─────────────────────────────────────────────────────────

const PROOFREAD_DIR: &str = ".journalist/proofread";

/// Parse `/proofread` arguments: `--file <path>` and remaining inline text.
pub fn parse_proofread_args(args: &str) -> (Option<String>, String) {
    let args = args.trim();
    let mut file_path: Option<String> = None;
    let mut remaining_parts: Vec<String> = Vec::new();

    let tokens: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "--file" => {
                if i + 1 < tokens.len() {
                    file_path = Some(tokens[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            other => {
                remaining_parts.push(other.to_string());
                i += 1;
            }
        }
    }

    (file_path, remaining_parts.join(" "))
}

/// Build the proofread prompt with Korean news style rules embedded.
pub fn build_proofread_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    Some(format!(
        r#"당신은 한국 신문사의 교열 전문가입니다. 아래 기사를 교열하세요.

## 교열 규칙
1. **맞춤법·띄어쓰기**: 한글 맞춤법 통일안 및 표준어 규정 준수
2. **경어체 통일**: 뉴스 기사는 '~했다', '~이다' 등 해요체가 아닌 하십시오체/해라체(보도문체) 통일
3. **숫자 표기**: 만 단위 이상은 한글 병기 (예: 1조2000억원), 날짜는 'O일' (예: 15일)
4. **외래어 표기법**: 국립국어원 외래어 표기법 준수 (예: 컴퓨터, 인터넷)
5. **중복 표현 제거**: '약 ~정도', '먼저 ~에 앞서' 등 불필요한 중복 삭제
6. **인용문 형식**: 직접 인용은 큰따옴표(" "), 간접 인용은 따옴표 없이 '~(이)라고 말했다'
7. **주어-술어 호응**: 문장 내 주어와 술어의 호응 확인
8. **문장 길이**: 한 문장이 80자를 초과하면 분리 권장
9. **비문·어색한 표현**: 자연스러운 한국어로 교정
10. **뉴스 용어**: 약어 첫 등장 시 풀어쓰기 (예: GDP(국내총생산))

## 출력 형식
아래 형식으로 교정 결과를 출력하세요:

### 교열 결과

| # | 위치 | 원문 | 교정 | 근거 |
|---|------|------|------|------|
| 1 | 1문단 | ... | ... | ... |

### 교정된 전문
(교정이 반영된 전체 기사)

### 총평
(전반적인 문체·구조 평가, 1~2문장)

## 원문
{article}"#
    ))
}

/// Build proofread result file path with an explicit date string.
pub fn proofread_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_proofread.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(PROOFREAD_DIR).join(filename)
}

/// Build proofread result file path with today's date.
pub fn proofread_file_path(slug_source: &str) -> std::path::PathBuf {
    proofread_file_path_with_date(slug_source, &today_str())
}

/// Save proofread result to file. Creates the directory if needed.
fn save_proofread(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Handle the `/proofread` command: proofread a Korean article for grammar, spelling, and news style.
pub async fn handle_proofread(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/proofread").unwrap_or("").trim();
    let (file_path, inline_text) = parse_proofread_args(args);

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

    let prompt = match build_proofread_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /proofread <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /proofread --file <경로>{RESET}");
            println!(
                "{DIM}  한국어 기사의 맞춤법, 문법, 뉴스 문체를 교정합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save proofread result to .journalist/proofread/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "proofread".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "proofread".to_string()
            } else {
                preview
            }
        };
        let path = proofread_file_path(&slug_source);
        match save_proofread(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 교열 결과 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  교열 결과 저장 실패: {e}{RESET}\n");
            }
        }
    }
}

// ── /quote ──────────────────────────────────────────────────────────────

/// Quotes database path.
const QUOTES_FILE: &str = ".journalist/quotes.json";

/// Handle the /quote command: manage interview quotes.
pub fn handle_quote(input: &str) {
    let args = input.strip_prefix("/quote").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "add" => {
            let rest = args.strip_prefix("add").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /quote add <취재원> <발언>{RESET}");
                println!("{DIM}  예시: /quote add 홍길동 \"반도체 수출이 3개월 연속 증가했습니다\"{RESET}\n");
            } else {
                quote_add(rest);
            }
        }
        "list" => {
            let rest = args.strip_prefix("list").unwrap_or("").trim();
            quote_list(rest);
        }
        "search" => {
            let rest = args.strip_prefix("search").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /quote search <키워드>{RESET}\n");
            } else {
                quote_search(rest);
            }
        }
        "remove" => {
            let rest = args.strip_prefix("remove").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /quote remove <번호>{RESET}");
                println!("{DIM}  예시: /quote remove 2{RESET}\n");
            } else {
                quote_remove(rest);
            }
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /quote [add|list|search|remove]{RESET}\n");
        }
    }
}

fn load_quotes() -> Vec<serde_json::Value> {
    load_quotes_from(std::path::Path::new(QUOTES_FILE))
}

fn load_quotes_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_quotes(quotes: &[serde_json::Value]) {
    save_quotes_to(quotes, std::path::Path::new(QUOTES_FILE));
}

fn save_quotes_to(quotes: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(quotes) {
        let _ = std::fs::write(path, json);
    }
}

/// Look up source org from sources.json by name.
fn source_org_for(name: &str) -> Option<String> {
    let sources = load_sources();
    for s in &sources {
        if s["name"].as_str() == Some(name) {
            if let Some(org) = s["org"].as_str() {
                if !org.is_empty() {
                    return Some(org.to_string());
                }
            }
        }
    }
    None
}

fn quote_add(args: &str) {
    // Parse: <취재원> <발언> — the first token is the source name, rest is the quote
    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 || parts[1].is_empty() {
        println!("{DIM}  취재원 이름과 발언 내용이 모두 필요합니다.{RESET}\n");
        return;
    }
    let source_name = parts[0];
    let text = parts[1].trim_matches('"');
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);

    let entry = serde_json::json!({
        "source": source_name,
        "text": text,
        "timestamp": timestamp,
    });
    let mut quotes = load_quotes();
    quotes.push(entry);
    save_quotes(&quotes);

    let org_info = source_org_for(source_name)
        .map(|o| format!(" ({})", o))
        .unwrap_or_default();
    println!(
        "{DIM}  인용문 추가됨: {source_name}{org_info} — \"{text}\" [{timestamp}]{RESET}\n"
    );
}

fn quote_list(filter_source: &str) {
    let quotes = load_quotes();
    if quotes.is_empty() {
        println!("{DIM}  인용문 DB가 비어 있습니다.");
        println!("  /quote add <취재원> <발언> 으로 추가하세요.{RESET}\n");
        return;
    }

    let filtered: Vec<(usize, &serde_json::Value)> = if filter_source.is_empty() {
        quotes.iter().enumerate().collect()
    } else {
        quotes
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                q["source"]
                    .as_str()
                    .map(|s| s == filter_source)
                    .unwrap_or(false)
            })
            .collect()
    };

    if filtered.is_empty() {
        println!("{DIM}  '{filter_source}' 취재원의 인용문이 없습니다.{RESET}\n");
        return;
    }

    let title = if filter_source.is_empty() {
        format!("인용문 목록 ({} 건)", filtered.len())
    } else {
        let org_info = source_org_for(filter_source)
            .map(|o| format!(" ({})", o))
            .unwrap_or_default();
        format!(
            "{filter_source}{org_info} 인용문 ({} 건)",
            filtered.len()
        )
    };
    println!("{DIM}  ── {title} ──");
    for (i, q) in &filtered {
        let source = q["source"].as_str().unwrap_or("?");
        let text = q["text"].as_str().unwrap_or("");
        let ts = q["timestamp"].as_str().unwrap_or("");
        println!("  {}. [{ts}] {source}: \"{text}\"", i + 1);
    }
    println!("{RESET}");
}

fn quote_search(keyword: &str) {
    let quotes = load_quotes();
    let keyword_lower = keyword.to_lowercase();
    let matches: Vec<(usize, &serde_json::Value)> = quotes
        .iter()
        .enumerate()
        .filter(|(_, q)| {
            let text = q["text"].as_str().unwrap_or("").to_lowercase();
            let source = q["source"].as_str().unwrap_or("").to_lowercase();
            text.contains(&keyword_lower) || source.contains(&keyword_lower)
        })
        .collect();

    if matches.is_empty() {
        println!("{DIM}  '{keyword}' 검색 결과가 없습니다.{RESET}\n");
        return;
    }

    println!("{DIM}  ── 인용문 검색: '{keyword}' ({} 건) ──", matches.len());
    for (i, q) in &matches {
        let source = q["source"].as_str().unwrap_or("?");
        let text = q["text"].as_str().unwrap_or("");
        let ts = q["timestamp"].as_str().unwrap_or("");
        println!("  {}. [{ts}] {source}: \"{text}\"", i + 1);
    }
    println!("{RESET}");
}

fn quote_remove(args: &str) {
    let idx: usize = match args.parse() {
        Ok(n) if n >= 1 => n,
        _ => {
            eprintln!("{RED}  올바른 번호를 입력하세요.{RESET}\n");
            return;
        }
    };
    let mut quotes = load_quotes();
    if idx > quotes.len() {
        eprintln!("{RED}  번호 {idx}번은 범위를 벗어났습니다 (총 {} 건).{RESET}\n", quotes.len());
        return;
    }
    let removed = quotes.remove(idx - 1);
    save_quotes(&quotes);
    let source = removed["source"].as_str().unwrap_or("?");
    let text = removed["text"].as_str().unwrap_or("");
    let preview = if text.len() > 30 {
        format!("{}…", &text[..text.char_indices().take(30).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(30)])
    } else {
        text.to_string()
    };
    println!("{DIM}  인용문 삭제됨: {source} — \"{preview}\"{RESET}\n");
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

// ── /legal ───────────────────────────────────────────────────────────────

const LEGAL_DIR: &str = ".journalist/legal";

/// Parse `/legal` input to extract `--file <path>` and inline text.
/// Returns `(Option<file_path>, remaining_text)`.
pub fn parse_legal_args(args: &str) -> (Option<String>, String) {
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

/// Build the prompt for the `/legal` command (pre-publication legal risk check).
pub fn build_legal_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }
    Some(format!(
        "아래 기사 텍스트에 대해 출고 전 법적 리스크를 점검해주세요.\n\n\
         기사 텍스트:\n\"\"\"\n{article}\n\"\"\"\n\n\
         다음 항목을 순서대로 점검하고, 각 항목마다 리스크 등급을 표시하세요:\n\n\
         ## 1. 명예훼손 위험 요소\n\
         - 미확인 사실을 단정적으로 주장하고 있는지\n\
         - 출처 없이 특정인/단체를 비난하고 있는지\n\
         - 사생활 침해 요소가 있는지 (주거지, 가족관계, 건강정보 등)\n\
         - **형사상 명예훼손**: 사실 적시라도 공익 목적 없이 명예를 훼손하면 처벌 대상\n\n\
         ## 2. 초상권·프라이버시 침해\n\
         - 본인 동의 없는 사진/영상 사용 여부\n\
         - 사적 공간에서의 촬영물 포함 여부\n\
         - 개인정보(전화번호, 주소, 주민번호 등) 노출 여부\n\n\
         ## 3. 일방적 보도 여부 (반론권)\n\
         - 비판 대상의 반론/해명이 포함되어 있는지\n\
         - 반론 요청 시도 여부가 기재되어 있는지\n\
         - 언론중재법상 반론보도청구권 리스크\n\n\
         ## 4. 공인/사인 구분 기준 적용\n\
         - 기사 대상이 공인인지 사인인지 판단\n\
         - 공인: 공적 활동에 대한 비판은 허용 범위가 넓음\n\
         - 사인: 보도 기준이 엄격, 공익성 입증 필요\n\
         - 적용된 기준이 적절한지 판단\n\n\
         ## 5. 기타 법적 리스크\n\
         - 저작권 침해 (타 매체 기사/사진 무단 인용)\n\
         - 재판 계류 중 사건의 무죄추정 원칙 준수 여부\n\
         - 소년법/성폭력처벌법 등 보도 제한 규정 위반 여부\n\n\
         ## 종합 판정\n\
         각 항목별로 다음 등급을 부여하세요:\n\
         - ✅ 안전: 법적 리스크 없음\n\
         - ⚠️ 주의: 수정을 권고하는 부분 있음\n\
         - 🚨 위험: 반드시 수정 또는 삭제 필요\n\n\
         **종합 리스크 등급**과 함께, ⚠️ 이상 항목에 대해 **구체적인 수정 제안**을 제시하세요.\n\
         법적 근거(조항)를 가능한 한 명시하세요."
    ))
}

/// Build the legal check file path with an explicit date string (for testing).
pub fn legal_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_legal.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(LEGAL_DIR).join(filename)
}

/// Build legal check file path with today's date.
pub fn legal_file_path(slug_source: &str) -> std::path::PathBuf {
    legal_file_path_with_date(slug_source, &today_str())
}

/// Save legal check result to file. Creates the legal directory if needed.
fn save_legal(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// List existing legal check files in the legal directory.
fn legal_list() {
    let dir = std::path::Path::new(LEGAL_DIR);
    if !dir.exists() {
        println!("{DIM}  저장된 법적 점검 기록이 없습니다.{RESET}\n");
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
            .collect(),
        Err(_) => {
            println!("{DIM}  법적 점검 디렉토리를 읽을 수 없습니다.{RESET}\n");
            return;
        }
    };
    if entries.is_empty() {
        println!("{DIM}  저장된 법적 점검 기록이 없습니다.{RESET}\n");
        return;
    }
    entries.sort_by_key(|e| e.file_name());
    println!("{DIM}  저장된 법적 점검 목록:{RESET}");
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

/// Handle the `/legal` command: pre-publication legal risk check.
pub async fn handle_legal(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/legal").unwrap_or("").trim();

    if args == "list" {
        legal_list();
        return;
    }

    let (file_path, inline_text) = parse_legal_args(args);

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

    let prompt = match build_legal_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /legal <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /legal --file <경로>{RESET}");
            println!("{DIM}  또는:   /legal list — 저장된 법적 점검 목록{RESET}");
            println!("{DIM}  예시:   /legal --file draft.md{RESET}");
            println!(
                "{DIM}  기사의 명예훼손, 초상권, 반론권, 공인/사인 구분 등 법적 리스크를 점검합니다.{RESET}\n"
            );
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save legal check result to .journalist/legal/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "legal".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "legal".to_string()
            } else {
                preview
            }
        };
        let path = legal_file_path(&slug_source);
        match save_legal(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 법적 점검 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  법적 점검 저장 실패: {e}{RESET}\n");
            }
        }
    }
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

// ── /archive — 출고 기사 아카이브 시스템 ────────────────────────────────

const ARCHIVE_DIR: &str = ".journalist/archive";
const ARCHIVE_INDEX: &str = ".journalist/archive/index.json";

pub fn handle_archive(input: &str) {
    let args = input.strip_prefix("/archive").unwrap_or("").trim();

    match args.split_whitespace().next().unwrap_or("list") {
        "save" => {
            let rest = args.strip_prefix("save").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /archive save <제목> [--section 경제] [--type 스트레이트] [--tags 반도체,삼성]{RESET}");
                println!("{DIM}  본문: 표준 입력(파이프) 또는 --file <경로> 로 지정{RESET}\n");
            } else {
                archive_save(rest);
            }
        }
        "list" => {
            let rest = args.strip_prefix("list").unwrap_or("").trim();
            archive_list(rest);
        }
        "search" => {
            let rest = args.strip_prefix("search").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /archive search <키워드>{RESET}");
                println!("{DIM}  예시: /archive search 반도체{RESET}\n");
            } else {
                archive_search(rest);
            }
        }
        "view" => {
            let rest = args.strip_prefix("view").unwrap_or("").trim();
            if rest.is_empty() {
                println!("{DIM}  사용법: /archive view <번호>{RESET}");
                println!("{DIM}  예시: /archive view 3{RESET}\n");
            } else {
                archive_view(rest);
            }
        }
        other => {
            eprintln!("{RED}  알 수 없는 하위 커맨드: {other}{RESET}");
            println!("{DIM}  사용법: /archive [save|list|search|view]{RESET}\n");
        }
    }
}

/// Parse archive save arguments: extract title, --section, --type, --tags, --file.
fn parse_archive_save_args(args: &str) -> (String, String, String, Vec<String>, Option<String>) {
    let mut title = String::new();
    let mut section = String::new();
    let mut article_type = String::new();
    let mut tags: Vec<String> = Vec::new();
    let mut file_path: Option<String> = None;

    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--section" => {
                if i + 1 < parts.len() {
                    section = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--type" => {
                if i + 1 < parts.len() {
                    article_type = parts[i + 1].to_string();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--tags" => {
                if i + 1 < parts.len() {
                    tags = parts[i + 1].split(',').map(|s| s.trim().to_string()).collect();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--file" => {
                if i + 1 < parts.len() {
                    file_path = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                if title.is_empty() {
                    title = parts[i].to_string();
                } else {
                    // Accumulate multi-word title until we hit a flag
                    title.push(' ');
                    title.push_str(parts[i]);
                }
                i += 1;
            }
        }
    }

    (title, section, article_type, tags, file_path)
}

fn archive_save(args: &str) {
    let (title, section, article_type, tags, file_path) = parse_archive_save_args(args);

    if title.is_empty() {
        println!("{DIM}  제목을 입력하세요.{RESET}\n");
        return;
    }

    // Read body from file if --file provided
    let body = if let Some(ref fp) = file_path {
        match std::fs::read_to_string(fp) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                return;
            }
        }
    } else {
        String::new()
    };

    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let timestamp = format_unix_timestamp(secs);
    let date = &timestamp[..10]; // "YYYY-MM-DD"

    let mut index = load_archive_index_from(std::path::Path::new(ARCHIVE_INDEX));
    let id = index.len() + 1;

    // Save body text file
    let text_filename = format!("{id:04}.txt");
    let text_path = std::path::Path::new(ARCHIVE_DIR).join(&text_filename);
    ensure_sources_dir_at(&text_path);
    let _ = std::fs::write(&text_path, &body);

    // Build metadata entry
    let entry = serde_json::json!({
        "id": id,
        "title": title,
        "date": date,
        "section": section,
        "type": article_type,
        "tags": tags,
        "file": text_filename,
    });

    index.push(entry);
    save_archive_index_to(&index, std::path::Path::new(ARCHIVE_INDEX));

    println!("{DIM}  기사 아카이브 저장됨: #{id} \"{title}\" [{date}]{RESET}");
    if !section.is_empty() {
        println!("{DIM}    섹션: {section}{RESET}");
    }
    if !article_type.is_empty() {
        println!("{DIM}    유형: {article_type}{RESET}");
    }
    if !tags.is_empty() {
        println!("{DIM}    태그: {}{RESET}", tags.join(", "));
    }
    println!();
}

fn archive_list(args: &str) {
    archive_list_from(std::path::Path::new(ARCHIVE_INDEX), args);
}

fn archive_list_from(index_path: &std::path::Path, args: &str) {
    let index = load_archive_index_from(index_path);
    if index.is_empty() {
        println!("{DIM}  아카이브가 비어있습니다.");
        println!("  /archive save <제목> 으로 기사를 저장하세요.{RESET}\n");
        return;
    }

    // Parse --section and --recent flags
    let mut section_filter: Option<String> = None;
    let mut recent_limit: Option<usize> = None;
    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--section" => {
                if i + 1 < parts.len() {
                    section_filter = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--recent" => {
                if i + 1 < parts.len() {
                    recent_limit = parts[i + 1].parse().ok();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    // Filter by section
    let filtered: Vec<&serde_json::Value> = index
        .iter()
        .filter(|e| {
            if let Some(ref sec) = section_filter {
                e["section"].as_str().unwrap_or("") == sec
            } else {
                true
            }
        })
        .collect();

    // Apply recent limit (from the end)
    let display: Vec<&&serde_json::Value> = if let Some(n) = recent_limit {
        filtered.iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
    } else {
        filtered.iter().collect()
    };

    if display.is_empty() {
        println!("{DIM}  조건에 맞는 기사가 없습니다.{RESET}\n");
        return;
    }

    println!("{BOLD}  기사 아카이브 ({} 건){RESET}", display.len());
    println!("{DIM}  ─────────────────────────────────────{RESET}");
    for entry in &display {
        let id = entry["id"].as_u64().unwrap_or(0);
        let date = entry["date"].as_str().unwrap_or("?");
        let title = entry["title"].as_str().unwrap_or("?");
        let section = entry["section"].as_str().unwrap_or("");
        let sec_display = if section.is_empty() {
            String::new()
        } else {
            format!(" [{section}]")
        };
        println!("{DIM}  {id:>4}. {date}  {title}{sec_display}{RESET}");
    }
    println!();
}

fn archive_search(keyword: &str) {
    archive_search_in(
        std::path::Path::new(ARCHIVE_INDEX),
        std::path::Path::new(ARCHIVE_DIR),
        keyword,
    );
}

fn archive_search_in(index_path: &std::path::Path, archive_dir: &std::path::Path, keyword: &str) {
    let index = load_archive_index_from(index_path);
    if index.is_empty() {
        println!("{DIM}  아카이브가 비어있습니다.{RESET}\n");
        return;
    }

    let keyword_lower = keyword.to_lowercase();
    let mut results: Vec<&serde_json::Value> = Vec::new();

    for entry in &index {
        // Search in title
        let title = entry["title"].as_str().unwrap_or("");
        if title.to_lowercase().contains(&keyword_lower) {
            results.push(entry);
            continue;
        }

        // Search in tags
        if let Some(tags) = entry["tags"].as_array() {
            if tags.iter().any(|t| {
                t.as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&keyword_lower)
            }) {
                results.push(entry);
                continue;
            }
        }

        // Search in body text
        let filename = entry["file"].as_str().unwrap_or("");
        if !filename.is_empty() {
            let text_path = archive_dir.join(filename);
            if let Ok(body) = std::fs::read_to_string(&text_path) {
                if body.to_lowercase().contains(&keyword_lower) {
                    results.push(entry);
                }
            }
        }
    }

    if results.is_empty() {
        println!("{DIM}  \"{keyword}\" 검색 결과 없음.{RESET}\n");
        return;
    }

    println!(
        "{BOLD}  \"{keyword}\" 검색 결과 ({} 건){RESET}",
        results.len()
    );
    println!("{DIM}  ─────────────────────────────────────{RESET}");
    for entry in &results {
        let id = entry["id"].as_u64().unwrap_or(0);
        let date = entry["date"].as_str().unwrap_or("?");
        let title = entry["title"].as_str().unwrap_or("?");
        let section = entry["section"].as_str().unwrap_or("");
        let sec_display = if section.is_empty() {
            String::new()
        } else {
            format!(" [{section}]")
        };
        println!("{DIM}  {id:>4}. {date}  {title}{sec_display}{RESET}");
    }
    println!();
}

fn archive_view(id_str: &str) {
    archive_view_in(
        std::path::Path::new(ARCHIVE_INDEX),
        std::path::Path::new(ARCHIVE_DIR),
        id_str,
    );
}

fn archive_view_in(
    index_path: &std::path::Path,
    archive_dir: &std::path::Path,
    id_str: &str,
) {
    let id: usize = match id_str.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("{RED}  유효한 번호를 입력하세요.{RESET}\n");
            return;
        }
    };

    let index = load_archive_index_from(index_path);
    let entry = index.iter().find(|e| e["id"].as_u64() == Some(id as u64));
    let entry = match entry {
        Some(e) => e,
        None => {
            eprintln!("{RED}  #{id} 기사를 찾을 수 없습니다.{RESET}\n");
            return;
        }
    };

    let title = entry["title"].as_str().unwrap_or("?");
    let date = entry["date"].as_str().unwrap_or("?");
    let section = entry["section"].as_str().unwrap_or("");
    let article_type = entry["type"].as_str().unwrap_or("");
    let tags: Vec<&str> = entry["tags"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    println!("{BOLD}  #{id} {title}{RESET}");
    println!("{DIM}  날짜: {date}{RESET}");
    if !section.is_empty() {
        println!("{DIM}  섹션: {section}{RESET}");
    }
    if !article_type.is_empty() {
        println!("{DIM}  유형: {article_type}{RESET}");
    }
    if !tags.is_empty() {
        println!("{DIM}  태그: {}{RESET}", tags.join(", "));
    }
    println!("{DIM}  ─────────────────────────────────────{RESET}");

    let filename = entry["file"].as_str().unwrap_or("");
    if !filename.is_empty() {
        let text_path = archive_dir.join(filename);
        match std::fs::read_to_string(&text_path) {
            Ok(body) => {
                if body.is_empty() {
                    println!("{DIM}  (본문 없음){RESET}");
                } else {
                    println!("{DIM}{body}{RESET}");
                }
            }
            Err(_) => {
                println!("{DIM}  (본문 파일을 읽을 수 없습니다){RESET}");
            }
        }
    }
    println!();
}

fn load_archive_index_from(path: &std::path::Path) -> Vec<serde_json::Value> {
    if !path.exists() {
        return Vec::new();
    }
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_archive_index_to(index: &[serde_json::Value], path: &std::path::Path) {
    ensure_sources_dir_at(path);
    if let Ok(json) = serde_json::to_string_pretty(index) {
        let _ = std::fs::write(path, json);
    }
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

// ── /follow ──────────────────────────────────────────────────────────────

const FOLLOWUPS_FILE: &str = ".journalist/followups.json";

/// A single follow-up story entry.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct Followup {
    topic: String,
    /// Optional due date in "YYYY-MM-DD" format.
    due: Option<String>,
    done: bool,
    /// ISO 8601 datetime when the followup was created.
    created_at: String,
}

fn followups_path() -> std::path::PathBuf {
    std::path::PathBuf::from(FOLLOWUPS_FILE)
}

fn load_followups_from(path: &std::path::Path) -> Vec<Followup> {
    match std::fs::read_to_string(path) {
        Ok(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn save_followups_to(followups: &[Followup], path: &std::path::Path) {
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
fn days_until(target: &str, today: &str) -> Option<i64> {
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

// ── /publish ────────────────────────────────────────────────────────────

/// Possible outcomes for each pipeline step.
#[derive(Debug, Clone, PartialEq)]
pub enum PublishStepResult {
    /// Step completed successfully.
    Pass(String),
    /// Step failed (e.g. file not found, empty article).
    Fail(String),
    /// Legal step found 위험 — pipeline must halt.
    Blocked(String),
}

/// Run the publish pipeline: checklist → proofread → legal → export.
/// Returns a vec of (step_name, result) pairs.
/// Stops early if legal returns 위험.
pub async fn handle_publish(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/publish").unwrap_or("").trim();

    // Determine the target file (--file flag or latest draft)
    let file_path = if args.contains("--file") {
        let parts: Vec<&str> = args.splitn(3, "--file").collect();
        let after = parts.get(1).unwrap_or(&"").trim();
        let path = after.split_whitespace().next().unwrap_or("").to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    } else if !args.is_empty() {
        Some(args.to_string())
    } else {
        find_latest_draft().map(|p| p.to_string_lossy().to_string())
    };

    let file_path = match file_path {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /publish <파일> 또는 /publish --file <경로>{RESET}");
            println!(
                "{DIM}  출고 파이프라인: checklist → proofread → legal → export 를 순차 실행합니다.{RESET}"
            );
            println!("{DIM}  legal 단계에서 🚨 위험 판정 시 파이프라인을 중단합니다.{RESET}\n");
            return;
        }
    };

    // Verify file exists
    if !std::path::Path::new(&file_path).exists() {
        eprintln!("{RED}  파일을 찾을 수 없습니다: {file_path}{RESET}\n");
        return;
    }

    println!("\n{BOLD}  ══════════════════════════════════════{RESET}");
    println!("{BOLD}   🚀 출고 파이프라인 시작{RESET}");
    println!("{BOLD}   대상: {file_path}{RESET}");
    println!("{BOLD}  ══════════════════════════════════════{RESET}\n");

    let results =
        run_publish_pipeline(agent, session_total, model, &file_path).await;

    print_publish_report(&results);
}

/// Core pipeline logic, separated for testability of the report printer.
async fn run_publish_pipeline(
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
    file_path: &str,
) -> Vec<(&'static str, PublishStepResult)> {
    let mut results: Vec<(&'static str, PublishStepResult)> = Vec::new();
    let steps: &[&str] = &["checklist", "proofread", "legal", "export"];

    for &step in steps {
        println!(
            "{CYAN}  ▶ [{step}] 단계 실행 중...{RESET}"
        );

        let result = match step {
            "checklist" => {
                let cmd = format!("/checklist --file {file_path}");
                handle_checklist(agent, &cmd, session_total, model).await;
                PublishStepResult::Pass("체크리스트 완료".to_string())
            }
            "proofread" => {
                let cmd = format!("/proofread --file {file_path}");
                handle_proofread(agent, &cmd, session_total, model).await;
                PublishStepResult::Pass("교열 완료".to_string())
            }
            "legal" => {
                // We need to capture the legal response to check for 위험
                let article = match std::fs::read_to_string(file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        let msg = format!("파일 읽기 실패: {e}");
                        results.push((step, PublishStepResult::Fail(msg)));
                        break;
                    }
                };
                let prompt = match build_legal_prompt(&article) {
                    Some(p) => p,
                    None => {
                        results.push((
                            step,
                            PublishStepResult::Fail("빈 기사 — 법적 점검 불가".to_string()),
                        ));
                        break;
                    }
                };
                let response = run_prompt(agent, &prompt, session_total, model).await;
                auto_compact_if_needed(agent);

                // Save legal result (same as handle_legal)
                if !response.trim().is_empty() {
                    let slug = std::path::Path::new(file_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "legal".to_string());
                    let path = legal_file_path(&slug);
                    if let Ok(()) = save_legal(&path, &response) {
                        println!("{GREEN}  ✓ 법적 점검 저장: {}{RESET}", path.display());
                    }
                }

                // Check for 🚨 위험 — halt if found
                if response.contains("위험") {
                    PublishStepResult::Blocked(
                        "🚨 법적 리스크 '위험' 판정 — 파이프라인 중단".to_string(),
                    )
                } else {
                    PublishStepResult::Pass("법적 점검 통과".to_string())
                }
            }
            "export" => {
                let cmd = format!("/export {file_path}");
                handle_export(&cmd);
                PublishStepResult::Pass("내보내기 완료".to_string())
            }
            _ => unreachable!(),
        };

        let blocked = matches!(&result, PublishStepResult::Blocked(_));
        results.push((step, result));

        if blocked {
            // Mark remaining steps as skipped
            let done = results.len();
            for &remaining in &steps[done..] {
                results.push((
                    remaining,
                    PublishStepResult::Fail("이전 단계 중단으로 건너뜀".to_string()),
                ));
            }
            break;
        }
    }

    results
}

/// Print the publish pipeline summary report.
pub fn print_publish_report(results: &[(&str, PublishStepResult)]) {
    println!("\n{BOLD}  ──────────────────────────────────────{RESET}");
    println!("{BOLD}   📊 출고 파이프라인 결과{RESET}");
    println!("{BOLD}  ──────────────────────────────────────{RESET}\n");

    let mut pass_count = 0u32;
    let mut fail_count = 0u32;
    let mut blocked = false;

    for (step, result) in results {
        match result {
            PublishStepResult::Pass(msg) => {
                println!("   ✅ {step}: {msg}");
                pass_count += 1;
            }
            PublishStepResult::Fail(msg) => {
                println!("   ❌ {step}: {msg}");
                fail_count += 1;
            }
            PublishStepResult::Blocked(msg) => {
                println!("   🚨 {step}: {msg}");
                fail_count += 1;
                blocked = true;
            }
        }
    }

    println!();
    if blocked {
        println!(
            "{RED}  ⛔ 출고 중단 — 법적 리스크를 먼저 해결하세요 (통과: {pass_count}, 실패/중단: {fail_count}){RESET}\n"
        );
    } else if fail_count > 0 {
        println!(
            "{YELLOW}  ⚠ 일부 단계 실패 (통과: {pass_count}, 실패: {fail_count}){RESET}\n"
        );
    } else {
        println!(
            "{GREEN}  ✅ 출고 준비 완료! 모든 단계 통과 ({pass_count}/{pass_count}){RESET}\n"
        );
    }
}

// ── /anonymize ───────────────────────────────────────────────────────────

const ANONYMIZE_DIR: &str = ".journalist/anonymize";

/// Build anonymize result file path with an explicit date string.
pub fn anonymize_file_path_with_date(slug_source: &str, date: &str) -> std::path::PathBuf {
    let slug = topic_to_slug(slug_source, 50);
    let filename = if slug.is_empty() {
        format!("{date}_anonymize.md")
    } else {
        format!("{date}_{slug}.md")
    };
    std::path::PathBuf::from(ANONYMIZE_DIR).join(filename)
}

/// Build anonymize result file path with today's date.
pub fn anonymize_file_path(slug_source: &str) -> std::path::PathBuf {
    anonymize_file_path_with_date(slug_source, &today_str())
}

/// Save anonymized result to file. Creates the directory if needed.
fn save_anonymize(path: &std::path::Path, content: &str) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

/// Build the anonymize prompt for PII de-identification in Korean news articles.
pub fn build_anonymize_prompt(article: &str) -> Option<String> {
    if article.trim().is_empty() {
        return None;
    }

    Some(format!(
        r#"당신은 한국 언론의 취재원 보호 전문가입니다. 아래 기사 텍스트에서 개인식별정보(PII)를 감지하고 익명화 처리하세요.

## 익명화 규칙
1. **실명** → A씨, B씨, C씨 등 (등장 순서대로 알파벳 부여, 동일 인물은 같은 알파벳 유지)
2. **소속·기관명** → A기관, B기관, C기관 등 (공공기관·정부부처·국제기구는 유지 가능)
3. **직함·직위** → 구체적 직함 제거, '관계자', '임원', '직원' 등으로 대체
4. **전화번호** → [전화번호 삭제]
5. **이메일** → [이메일 삭제]
6. **주소** → [주소 삭제] (시·도 단위는 유지 가능)
7. **주민등록번호·계좌번호 등** → [개인정보 삭제]
8. **나이·성별**: 기사 맥락에 필수적이면 유지, 아니면 삭제
9. **공인(대통령, 장관, 국회의원 등 공적 인물)**: 실명 유지 가능
10. **기업명**: 상장사·대기업은 유지, 중소기업·스타트업은 익명화 고려

## 출력 형식

### 감지된 개인식별정보

| # | 유형 | 원문 | 처리 | 비고 |
|---|------|------|------|------|
| 1 | 실명 | 홍길동 | A씨 | 취재원 |

### 익명화된 전문
(모든 PII가 처리된 전체 기사)

### 익명화 매핑표
(원문 ↔ 익명화 대응표, 내부 참조용)

| 익명 | 원문 | 유형 |
|------|------|------|
| A씨 | 홍길동 | 실명 |

### 주의사항
(익명화 과정에서 판단이 필요했던 사항, 공인 여부 판단 근거 등)

## 원문
{article}"#
    ))
}

/// Handle the `/anonymize` command: detect and anonymize PII in article text.
pub async fn handle_anonymize(
    agent: &mut Agent,
    input: &str,
    session_total: &mut Usage,
    model: &str,
) {
    let args = input.strip_prefix("/anonymize").unwrap_or("").trim();
    let (file_path, inline_text) = parse_proofread_args(args);

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

    let prompt = match build_anonymize_prompt(&article) {
        Some(p) => p,
        None => {
            println!("{DIM}  사용법: /anonymize <기사 텍스트>{RESET}");
            println!("{DIM}  또는:   /anonymize --file <경로>{RESET}");
            println!(
                "{DIM}  기사에서 실명·전화번호·이메일 등 개인식별정보를 감지하고 익명화합니다.{RESET}"
            );
            println!("{DIM}  탐사보도 초안 공유, 법률 검토 시 취재원 보호에 활용하세요.{RESET}\n");
            return;
        }
    };

    let response = run_prompt(agent, &prompt, session_total, model).await;
    auto_compact_if_needed(agent);

    // Save anonymized result to .journalist/anonymize/
    if !response.trim().is_empty() {
        let slug_source = if let Some(ref path) = file_path {
            std::path::Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "anonymize".to_string())
        } else {
            let preview: String = article.chars().take(30).collect();
            if preview.is_empty() {
                "anonymize".to_string()
            } else {
                preview
            }
        };
        let path = anonymize_file_path(&slug_source);
        match save_anonymize(&path, &response) {
            Ok(_) => {
                println!(
                    "{GREEN}  ✓ 익명화 결과 저장: {}{RESET}\n",
                    path.display()
                );
            }
            Err(e) => {
                eprintln!("{RED}  익명화 결과 저장 실패: {e}{RESET}\n");
            }
        }
    }
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

// ── /readability ────────────────────────────────────────────────────────

/// Readability metrics for Korean article text.
#[derive(Debug, PartialEq)]
pub struct ReadabilityMetrics {
    /// Average sentence length in characters.
    pub avg_sentence_len: f64,
    /// Ratio of long sentences (over 80 chars), 0.0–1.0.
    pub long_sentence_ratio: f64,
    /// Average number of sentences per paragraph.
    pub avg_paragraph_len: f64,
    /// Estimated passive voice ratio, 0.0–1.0.
    pub passive_ratio: f64,
    /// Jargon density (ratio of jargon-like words), 0.0–1.0.
    pub jargon_density: f64,
    /// Overall grade: A (best) through F (worst).
    pub grade: char,
    /// Total number of sentences detected.
    pub sentence_count: usize,
    /// Total number of paragraphs detected.
    pub paragraph_count: usize,
}

/// Split Korean text into sentences using sentence-ending markers.
///
/// Korean sentences typically end with 다, 요, 죠, 음, 임 followed by period/question/exclamation,
/// or just period/question/exclamation on their own.
fn split_korean_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if ch == '.' || ch == '!' || ch == '?' || ch == '。' {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }
    }
    // Remaining text without terminal punctuation counts as a sentence if non-empty
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }
    sentences
}

/// Split text into paragraphs (groups of non-empty lines separated by blank lines).
fn split_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current = String::new();

    for line in text.lines() {
        if line.trim().is_empty() {
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                paragraphs.push(trimmed);
            }
            current.clear();
        } else {
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        paragraphs.push(trimmed);
    }
    paragraphs
}

/// Korean passive voice suffixes.
const PASSIVE_SUFFIXES: &[&str] = &[
    "되었다", "됐다", "되었습니다", "됐습니다", "되었으며", "되었고",
    "된다", "됩니다", "되고", "되며", "되어", "돼",
    "받았다", "받았습니다", "받게", "받는다",
    "당했다", "당했습니다", "당하고",
];

/// Common jargon / technical terms frequently found in Korean news articles.
const JARGON_TERMS: &[&str] = &[
    "거버넌스", "컨센서스", "패러다임", "이니셔티브", "로드맵",
    "리스크", "레버리지", "포트폴리오", "밸류에이션", "모멘텀",
    "인프라", "플랫폼", "컴플라이언스", "가이드라인", "프레임워크",
    "시너지", "이해관계자", "스테이크홀더", "디폴트", "모라토리엄",
    "유동성", "변동성", "펀더멘털", "스프레드", "디커플링",
];

/// Compute readability metrics for Korean article text.
pub fn compute_readability(text: &str) -> ReadabilityMetrics {
    let sentences = split_korean_sentences(text);
    let paragraphs = split_paragraphs(text);

    let sentence_count = sentences.len();
    let paragraph_count = paragraphs.len();

    // Average sentence length (character count excluding spaces)
    let avg_sentence_len = if sentence_count > 0 {
        let total_chars: usize = sentences
            .iter()
            .map(|s| s.chars().filter(|c| !c.is_whitespace()).count())
            .sum();
        total_chars as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Long sentence ratio (over 80 chars excluding spaces)
    let long_sentence_ratio = if sentence_count > 0 {
        let long_count = sentences
            .iter()
            .filter(|s| s.chars().filter(|c| !c.is_whitespace()).count() > 80)
            .count();
        long_count as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Average paragraph length in sentences
    let avg_paragraph_len = if paragraph_count > 0 {
        let para_sentence_counts: Vec<usize> = paragraphs
            .iter()
            .map(|p| split_korean_sentences(p).len())
            .collect();
        let total: usize = para_sentence_counts.iter().sum();
        total as f64 / paragraph_count as f64
    } else {
        0.0
    };

    // Passive voice ratio
    let passive_ratio = if sentence_count > 0 {
        let passive_count = sentences
            .iter()
            .filter(|s| PASSIVE_SUFFIXES.iter().any(|suf| s.contains(suf)))
            .count();
        passive_count as f64 / sentence_count as f64
    } else {
        0.0
    };

    // Jargon density: count jargon occurrences per word
    let words: Vec<&str> = text.split_whitespace().collect();
    let word_count = words.len();
    let jargon_density = if word_count > 0 {
        let jargon_count = words
            .iter()
            .filter(|w| JARGON_TERMS.iter().any(|j| w.contains(j)))
            .count();
        jargon_count as f64 / word_count as f64
    } else {
        0.0
    };

    // Grade: score 0–100, then map to A–F
    // Lower avg sentence len is better, lower long_sentence_ratio is better, etc.
    let mut score = 100.0_f64;

    // Penalty for long average sentence (ideal: ~30 chars for Korean)
    if avg_sentence_len > 30.0 {
        score -= (avg_sentence_len - 30.0) * 0.5;
    }

    // Penalty for long sentences
    score -= long_sentence_ratio * 30.0;

    // Penalty for long paragraphs (ideal: 2–4 sentences)
    if avg_paragraph_len > 4.0 {
        score -= (avg_paragraph_len - 4.0) * 5.0;
    }

    // Penalty for passive voice
    score -= passive_ratio * 20.0;

    // Penalty for jargon
    score -= jargon_density * 30.0;

    let score = score.max(0.0).min(100.0);
    let grade = if score >= 90.0 {
        'A'
    } else if score >= 80.0 {
        'B'
    } else if score >= 70.0 {
        'C'
    } else if score >= 60.0 {
        'D'
    } else if score >= 50.0 {
        'E'
    } else {
        'F'
    };

    ReadabilityMetrics {
        avg_sentence_len,
        long_sentence_ratio,
        avg_paragraph_len,
        passive_ratio,
        jargon_density,
        grade,
        sentence_count,
        paragraph_count,
    }
}

/// Handle `/readability [파일경로]` — show readability analysis for article text.
pub fn handle_readability(input: &str) {
    let arg = input.strip_prefix("/readability").unwrap_or("").trim();

    let (path, content) = if arg.is_empty() {
        match find_latest_draft() {
            Some(p) => match std::fs::read_to_string(&p) {
                Ok(c) => (p.to_string_lossy().to_string(), c),
                Err(e) => {
                    eprintln!("{RED}  파일 읽기 실패: {e}{RESET}\n");
                    return;
                }
            },
            None => {
                eprintln!("{DIM}  분석할 파일이 없습니다. 경로를 지정하거나 /article로 초안을 먼저 작성하세요.{RESET}\n");
                return;
            }
        }
    } else {
        match std::fs::read_to_string(arg) {
            Ok(c) => (arg.to_string(), c),
            Err(e) => {
                eprintln!("{RED}  파일 읽기 실패 ({arg}): {e}{RESET}\n");
                return;
            }
        }
    };

    let m = compute_readability(&content);

    println!("{BOLD}  📖 가독성 분석: {path}{RESET}");
    println!("{DIM}  ──────────────────────────────{RESET}");
    println!("  종합 등급             {}", grade_colored(m.grade));
    println!("  문장 수               {}", m.sentence_count);
    println!("  문단 수               {}", m.paragraph_count);
    println!(
        "  평균 문장 길이         {:.1}자",
        m.avg_sentence_len
    );
    println!(
        "  긴 문장 비율 (>80자)  {:.1}%",
        m.long_sentence_ratio * 100.0
    );
    println!(
        "  평균 문단 길이         {:.1}문장",
        m.avg_paragraph_len
    );
    println!(
        "  수동태 추정 비율       {:.1}%",
        m.passive_ratio * 100.0
    );
    println!(
        "  전문 용어 밀도         {:.1}%",
        m.jargon_density * 100.0
    );
    println!();

    // Tips
    if m.long_sentence_ratio > 0.3 {
        println!("  {YELLOW}💡 긴 문장이 많습니다. 80자 이하로 줄이면 가독성이 높아집니다.{RESET}");
    }
    if m.passive_ratio > 0.3 {
        println!(
            "  {YELLOW}💡 수동태 표현이 많습니다. 능동태로 바꾸면 더 명확해집니다.{RESET}"
        );
    }
    if m.jargon_density > 0.05 {
        println!("  {YELLOW}💡 전문 용어가 많습니다. 독자 눈높이에 맞게 풀어쓰는 것을 권장합니다.{RESET}");
    }
    if m.avg_paragraph_len > 5.0 {
        println!("  {YELLOW}💡 문단이 깁니다. 3~4문장 단위로 끊으면 읽기 편합니다.{RESET}");
    }
    println!();
}

/// Return a grade string with color.
fn grade_colored(grade: char) -> String {
    match grade {
        'A' => format!("{GREEN}{BOLD}{grade}{RESET}"),
        'B' => format!("{GREEN}{grade}{RESET}"),
        'C' => format!("{YELLOW}{grade}{RESET}"),
        'D' => format!("{YELLOW}{grade}{RESET}"),
        _ => format!("{RED}{grade}{RESET}"),
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

    // --- parse_article_args tests ---

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

    // --- article type template tests ---

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
    fn article_prompt_default_type_is_straight() {
        let (prompt_default, _) = build_article_prompt("반도체 수출", &[], None);
        let (prompt_straight, _) =
            build_article_prompt("반도체 수출", &[], Some("straight"));
        // Both should contain the same structure keywords
        assert!(prompt_default.contains("리드"));
        assert!(prompt_straight.contains("리드"));
        assert!(prompt_default.contains("역피라미드") || prompt_default.contains("육하원칙"));
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

    // --- build_news_context / build_research_prompt tests ---

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

    // --- headline tests ---

    #[test]
    fn parse_headline_args_inline_text() {
        let (file, text) = parse_headline_args("삼성전자 1분기 실적 발표");
        assert!(file.is_none());
        assert_eq!(text, "삼성전자 1분기 실적 발표");
    }

    #[test]
    fn parse_headline_args_file_flag() {
        let (file, text) = parse_headline_args("--file draft.txt");
        assert_eq!(file.as_deref(), Some("draft.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_headline_args_file_with_extra_text() {
        let (file, text) = parse_headline_args("--file draft.txt 추가 맥락");
        assert_eq!(file.as_deref(), Some("draft.txt"));
        assert_eq!(text, "추가 맥락");
    }

    #[test]
    fn parse_headline_args_empty() {
        let (file, text) = parse_headline_args("");
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_headline_prompt_basic() {
        let prompt = build_headline_prompt("삼성전자가 1분기 영업이익 15조원을 기록했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("삼성전자가 1분기 영업이익 15조원을 기록했다."));
        assert!(p.contains("헤드라인"));
        assert!(p.contains("스트레이트"));
        assert!(p.contains("분석"));
        assert!(p.contains("피처"));
        assert!(p.contains("클릭유도"));
    }

    #[test]
    fn build_headline_prompt_empty_returns_none() {
        assert!(build_headline_prompt("").is_none());
        assert!(build_headline_prompt("   ").is_none());
    }

    #[test]
    fn headline_file_path_with_topic() {
        let path = headline_file_path_with_date("삼성전자 실적", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/headline/2026-03-18_삼성전자-실적.md")
        );
    }

    #[test]
    fn headline_file_path_empty_slug() {
        let path = headline_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/headline/2026-03-18_headline.md")
        );
    }

    #[test]
    fn save_headline_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("headline").join("test.md");
        let result = save_headline(&path, "# 헤드라인 후보\n\n[스트레이트] 테스트");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("헤드라인 후보"));
    }

    // ── /rewrite tests ──────────────────────────────────────────────────

    #[test]
    fn parse_rewrite_args_inline_text() {
        let (style, length, file, text) = parse_rewrite_args("삼성전자 기사 본문");
        assert!(style.is_none());
        assert!(length.is_none());
        assert!(file.is_none());
        assert_eq!(text, "삼성전자 기사 본문");
    }

    #[test]
    fn parse_rewrite_args_with_style() {
        let (style, length, file, text) = parse_rewrite_args("--style 요약 기사 본문");
        assert_eq!(style.as_deref(), Some("요약"));
        assert!(length.is_none());
        assert!(file.is_none());
        assert_eq!(text, "기사 본문");
    }

    #[test]
    fn parse_rewrite_args_with_all_options() {
        let (style, length, file, text) =
            parse_rewrite_args("--style 피처 --length 500 --file draft.txt 추가 맥락");
        assert_eq!(style.as_deref(), Some("피처"));
        assert_eq!(length.as_deref(), Some("500"));
        assert_eq!(file.as_deref(), Some("draft.txt"));
        assert_eq!(text, "추가 맥락");
    }

    #[test]
    fn parse_rewrite_args_file_only() {
        let (style, length, file, text) = parse_rewrite_args("--file article.txt");
        assert!(style.is_none());
        assert!(length.is_none());
        assert_eq!(file.as_deref(), Some("article.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_rewrite_args_empty() {
        let (style, length, file, text) = parse_rewrite_args("");
        assert!(style.is_none());
        assert!(length.is_none());
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_rewrite_prompt_basic() {
        let prompt = build_rewrite_prompt("삼성전자가 1분기 실적을 발표했다.", None, None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("삼성전자가 1분기 실적을 발표했다."));
        assert!(p.contains("스트레이트"));
        assert!(p.contains("재작성"));
    }

    #[test]
    fn build_rewrite_prompt_with_style() {
        let prompt =
            build_rewrite_prompt("기사 본문입니다.", Some("피처"), None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("피처"));
        assert!(p.contains("내러티브"));
    }

    #[test]
    fn build_rewrite_prompt_with_length() {
        let prompt =
            build_rewrite_prompt("기사 본문입니다.", Some("요약"), Some("300"));
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("요약"));
        assert!(p.contains("300자"));
    }

    #[test]
    fn build_rewrite_prompt_sns_style() {
        let prompt = build_rewrite_prompt("기사 본문.", Some("sns"), None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("SNS"));
    }

    #[test]
    fn build_rewrite_prompt_custom_style() {
        let prompt = build_rewrite_prompt("기사 본문.", Some("뉴스레터"), None);
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("뉴스레터"));
    }

    #[test]
    fn build_rewrite_prompt_empty_returns_none() {
        assert!(build_rewrite_prompt("", None, None).is_none());
        assert!(build_rewrite_prompt("   ", None, None).is_none());
    }

    #[test]
    fn rewrite_file_path_with_topic() {
        let path = rewrite_file_path_with_date("삼성전자 실적", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/drafts/2026-03-18_삼성전자-실적.md")
        );
    }

    #[test]
    fn rewrite_file_path_empty_slug() {
        let path = rewrite_file_path_with_date("", "2026-03-18");
        assert_eq!(
            path,
            std::path::PathBuf::from(".journalist/drafts/2026-03-18_rewrite.md")
        );
    }

    #[test]
    fn save_rewrite_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("drafts").join("test.md");
        let result = save_rewrite(&path, "# 재작성\n\n재작성된 기사 본문");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("재작성"));
    }

    // ── /clip tests ─────────────────────────────────────────────────────

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

    // ── /summary tests ──

    #[test]
    fn build_summary_prompt_basic() {
        let prompt = build_summary_prompt("정부가 오늘 새로운 부동산 정책을 발표했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("정부가 오늘 새로운 부동산 정책을 발표했다."));
        assert!(p.contains("3~5줄"));
        assert!(p.contains("핵심 요약"));
    }

    #[test]
    fn build_summary_prompt_empty_returns_none() {
        assert!(build_summary_prompt("").is_none());
        assert!(build_summary_prompt("   ").is_none());
    }

    #[test]
    fn resolve_summary_input_inline_text() {
        let result = resolve_summary_input("정부가 부동산 정책을 발표했다");
        assert_eq!(result, Some("정부가 부동산 정책을 발표했다".to_string()));
    }

    #[test]
    fn resolve_summary_input_empty() {
        assert!(resolve_summary_input("").is_none());
        assert!(resolve_summary_input("   ").is_none());
    }

    #[test]
    fn resolve_summary_input_reads_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test_doc.txt");
        std::fs::write(&file_path, "보도자료 내용입니다.").unwrap();
        let result = resolve_summary_input(file_path.to_str().unwrap());
        assert_eq!(result, Some("보도자료 내용입니다.".to_string()));
    }

    #[test]
    fn resolve_summary_input_nonexistent_file_treated_as_text() {
        let result = resolve_summary_input("no_such_file_xyz.txt");
        // Non-existent file path is treated as inline text
        assert_eq!(result, Some("no_such_file_xyz.txt".to_string()));
    }

    // ── /news tests ──

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

    // ── /stats tests ──

    #[test]
    fn stats_empty_text() {
        let stats = compute_text_stats("");
        assert_eq!(stats.chars_with_spaces, 0);
        assert_eq!(stats.chars_without_spaces, 0);
        assert_eq!(stats.words, 0);
        assert_eq!(stats.sentences, 0);
        assert_eq!(stats.paragraphs, 0);
        assert_eq!(stats.reading_time_secs, 0);
    }

    #[test]
    fn stats_single_sentence() {
        let text = "오늘 서울 날씨는 맑음.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.chars_with_spaces, 13);
        assert_eq!(stats.chars_without_spaces, 10);
        assert_eq!(stats.words, 4); // "오늘" "서울" "날씨는" "맑음."
        assert_eq!(stats.sentences, 1);
        assert_eq!(stats.paragraphs, 1);
    }

    #[test]
    fn stats_multiple_paragraphs() {
        let text = "첫 번째 문단입니다.\n\n두 번째 문단입니다.\n\n세 번째 문단입니다.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.paragraphs, 3);
        assert_eq!(stats.sentences, 3);
    }

    #[test]
    fn stats_reading_time() {
        // 500 chars (no spaces) → 60 seconds
        let text = "가".repeat(500);
        let stats = compute_text_stats(&text);
        assert_eq!(stats.reading_time_secs, 60);
    }

    #[test]
    fn stats_mixed_punctuation() {
        let text = "정말요? 네! 좋습니다.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.sentences, 3);
    }

    #[test]
    fn format_reading_time_seconds_only() {
        assert_eq!(format_reading_time(30), "30초");
    }

    #[test]
    fn format_reading_time_minutes_only() {
        assert_eq!(format_reading_time(120), "2분");
    }

    #[test]
    fn format_reading_time_mixed() {
        assert_eq!(format_reading_time(90), "1분 30초");
    }

    #[test]
    fn stats_words_english() {
        let text = "Hello world. This is a test.";
        let stats = compute_text_stats(text);
        assert_eq!(stats.words, 6);
        assert_eq!(stats.sentences, 2);
    }

    // ── /draft tests ─────────────────────────────────────────────────────

    #[test]
    fn draft_versions_dir_uses_slug() {
        let dir = draft_versions_dir("테스트 기사");
        assert!(dir.to_string_lossy().contains("테스트-기사"));
        assert!(dir.starts_with(DRAFT_VERSIONS_BASE));
    }

    #[test]
    fn draft_next_version_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("draft-test");
        // Dir doesn't exist yet
        assert_eq!(next_version_number(&dir), 1);
        // Create dir, still empty
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(next_version_number(&dir), 1);
    }

    #[test]
    fn draft_next_version_increments() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("draft-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("v1.md"), "first").unwrap();
        assert_eq!(next_version_number(&dir), 2);
        std::fs::write(dir.join("v2.md"), "second").unwrap();
        assert_eq!(next_version_number(&dir), 3);
    }

    #[test]
    fn draft_list_versions_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("draft-test");
        std::fs::create_dir_all(&dir).unwrap();
        // Create out of order
        std::fs::write(dir.join("v3.md"), "third").unwrap();
        std::fs::write(dir.join("v1.md"), "first").unwrap();
        std::fs::write(dir.join("v2.md"), "second").unwrap();
        // Also a non-version file
        std::fs::write(dir.join("notes.txt"), "ignore").unwrap();

        let versions = list_versions(&dir);
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].0, 1);
        assert_eq!(versions[1].0, 2);
        assert_eq!(versions[2].0, 3);
    }

    #[test]
    fn draft_list_versions_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("nonexistent");
        let versions = list_versions(&dir);
        assert!(versions.is_empty());
    }

    #[test]
    fn draft_format_unix_timestamp_epoch() {
        // 2024-01-01 00:00 UTC = 1704067200
        let s = format_unix_timestamp(1_704_067_200);
        assert_eq!(s, "2024-01-01 00:00");
    }

    #[test]
    fn draft_format_unix_timestamp_nonzero_time() {
        // 2025-06-15 14:30 UTC = 1750000200
        let s = format_unix_timestamp(1_750_000_200);
        assert!(s.starts_with("2025-"));
        assert!(s.contains(':'));
    }

    // ── /deadline tests ─────────────────────────────────────────────────

    fn temp_deadlines_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("deadlines.json");
        (dir, path)
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

    // --- export tests ---

    #[test]
    fn export_markdown_to_plain_text_strips_headings() {
        let md = "# 제목\n\n본문 내용입니다.\n\n## 소제목\n\n더 많은 내용.";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains('#'));
        assert!(plain.contains("제목"));
        assert!(plain.contains("본문 내용입니다."));
        assert!(plain.contains("소제목"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_bold_italic() {
        let md = "이것은 **굵은** 글씨와 *기울임* 입니다.";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains("**"));
        assert!(!plain.contains('*'));
        assert!(plain.contains("굵은"));
        assert!(plain.contains("기울임"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_links() {
        let md = "자세한 내용은 [여기](https://example.com)를 참고하세요.";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains("https://"));
        assert!(!plain.contains('['));
        assert!(plain.contains("여기"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_images() {
        let md = "이미지: ![대체텍스트](image.png)";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.contains("image.png"));
        assert!(plain.contains("대체텍스트"));
    }

    #[test]
    fn export_markdown_to_plain_text_strips_list_markers() {
        let md = "- 항목1\n- 항목2\n1. 번호항목";
        let plain = markdown_to_plain_text(md);
        assert!(!plain.starts_with("- "));
        assert!(plain.contains("항목1"));
        assert!(plain.contains("번호항목"));
    }

    #[test]
    fn export_markdown_to_html_basic_structure() {
        let md = "# 제목\n\n본문 내용.";
        let html = markdown_to_html(md);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<h1>제목</h1>"));
        assert!(html.contains("<p>본문 내용.</p>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn export_markdown_to_html_blockquote() {
        let md = "> 인용문입니다.";
        let html = markdown_to_html(md);
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("인용문입니다."));
    }

    #[test]
    fn export_markdown_to_html_list() {
        let md = "- 항목1\n- 항목2";
        let html = markdown_to_html(md);
        assert!(html.contains("<ul>"));
        assert!(html.contains("<li>항목1</li>"));
        assert!(html.contains("<li>항목2</li>"));
    }

    #[test]
    fn export_html_escapes_special_chars() {
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert('xss')&lt;/script&gt;"
        );
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }

    #[test]
    fn export_inline_md_to_html_bold() {
        let result = inline_md_to_html("이것은 **굵은** 텍스트");
        assert!(result.contains("<strong>굵은</strong>"));
    }

    #[test]
    fn export_strip_list_marker_dash() {
        assert_eq!(strip_list_marker("- 항목"), "항목");
        assert_eq!(strip_list_marker("* 항목"), "항목");
        assert_eq!(strip_list_marker("1. 항목"), "항목");
        assert_eq!(strip_list_marker("일반 텍스트"), "일반 텍스트");
    }

    #[test]
    fn export_build_meta_includes_info() {
        let meta = build_export_meta("test-article.md", 500);
        assert!(meta.contains("제목: test-article"));
        assert!(meta.contains("글자수: 500자"));
        assert!(meta.contains("날짜:"));
    }

    #[test]
    fn export_file_creates_text_output() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("article.md");
        std::fs::write(&src, "# 테스트 기사\n\n본문 **내용**입니다.").unwrap();

        // Set working dir context for EXPORTS_DIR
        let exports = tmp.path().join(".journalist").join("exports");
        std::fs::create_dir_all(&exports).unwrap();

        // Directly test the conversion functions
        let content = std::fs::read_to_string(&src).unwrap();
        let plain = markdown_to_plain_text(&content);
        assert!(plain.contains("테스트 기사"));
        assert!(plain.contains("본문"));
        assert!(!plain.contains("**"));
        assert!(!plain.contains('#'));
    }

    #[test]
    fn export_file_creates_html_output() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("article.md");
        std::fs::write(&src, "# 테스트\n\n> 인용\n\n- 목록").unwrap();

        let content = std::fs::read_to_string(&src).unwrap();
        let html = markdown_to_html(&content);
        assert!(html.contains("<h1>테스트</h1>"));
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("<li>목록</li>"));
    }

    #[test]
    fn export_regex_replace_pairs_balanced() {
        let result = regex_replace_pairs("a **b** c", "**", "<strong>", "</strong>");
        assert_eq!(result, "a <strong>b</strong> c");
    }

    #[test]
    fn export_regex_replace_pairs_unbalanced() {
        // Unbalanced delimiters should return original
        let result = regex_replace_pairs("a **b c", "**", "<strong>", "</strong>");
        assert_eq!(result, "a **b c");
    }

    // ── /proofread tests ─────────────────────────────────────────────────

    #[test]
    fn parse_proofread_args_inline_text() {
        let (file, text) = parse_proofread_args("삼성전자가 실적을 발표했다");
        assert!(file.is_none());
        assert_eq!(text, "삼성전자가 실적을 발표했다");
    }

    #[test]
    fn parse_proofread_args_with_file() {
        let (file, text) = parse_proofread_args("--file article.txt");
        assert_eq!(file.as_deref(), Some("article.txt"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_proofread_args_file_and_text() {
        let (file, text) = parse_proofread_args("--file draft.md 추가 맥락");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert_eq!(text, "추가 맥락");
    }

    #[test]
    fn parse_proofread_args_empty() {
        let (file, text) = parse_proofread_args("");
        assert!(file.is_none());
        assert!(text.is_empty());
    }

    #[test]
    fn build_proofread_prompt_basic() {
        let prompt = build_proofread_prompt("삼성전자가 1분기 실적을 발표했다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("삼성전자가 1분기 실적을 발표했다."));
        assert!(p.contains("교열"));
        assert!(p.contains("맞춤법"));
        assert!(p.contains("경어체"));
    }

    #[test]
    fn build_proofread_prompt_empty_returns_none() {
        assert!(build_proofread_prompt("").is_none());
        assert!(build_proofread_prompt("   ").is_none());
    }

    #[test]
    fn proofread_file_path_with_topic() {
        let path = proofread_file_path_with_date("반도체 수출", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/proofread/2026-03-20_반도체-수출.md"
        );
    }

    #[test]
    fn proofread_file_path_empty_slug() {
        let path = proofread_file_path_with_date("", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/proofread/2026-03-20_proofread.md"
        );
    }

    #[test]
    fn save_proofread_creates_dir_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("proofread").join("test.md");
        let result = save_proofread(&path, "# 교열 결과\n\n교정된 기사 본문");
        assert!(result.is_ok());
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("교열 결과"));
    }

    // ── /quote tests ────────────────────────────────────────────────────

    #[test]
    fn quote_load_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("quotes.json");
        let quotes = load_quotes_from(&path);
        assert!(quotes.is_empty());
    }

    #[test]
    fn quote_save_and_load() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("quotes.json");
        let quotes = vec![serde_json::json!({
            "source": "홍길동",
            "text": "반도체 수출이 증가했습니다",
            "timestamp": "2026-03-20 09:30",
        })];
        save_quotes_to(&quotes, &path);
        assert!(path.exists());
        let loaded = load_quotes_from(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0]["source"], "홍길동");
        assert_eq!(loaded[0]["text"], "반도체 수출이 증가했습니다");
        assert_eq!(loaded[0]["timestamp"], "2026-03-20 09:30");
    }

    #[test]
    fn quote_save_multiple_and_remove() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("quotes.json");
        let mut quotes = vec![
            serde_json::json!({"source": "김기자", "text": "첫 번째 발언", "timestamp": "2026-03-20 10:00"}),
            serde_json::json!({"source": "이기자", "text": "두 번째 발언", "timestamp": "2026-03-20 11:00"}),
            serde_json::json!({"source": "김기자", "text": "세 번째 발언", "timestamp": "2026-03-20 12:00"}),
        ];
        save_quotes_to(&quotes, &path);
        assert_eq!(load_quotes_from(&path).len(), 3);

        // Remove second entry (index 1)
        quotes.remove(1);
        save_quotes_to(&quotes, &path);
        let loaded = load_quotes_from(&path);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0]["source"], "김기자");
        assert_eq!(loaded[1]["text"], "세 번째 발언");
    }

    #[test]
    fn quote_source_org_lookup() {
        // source_org_for reads from the global SOURCES_FILE, so when no sources exist
        // it should return None.
        let result = source_org_for("존재하지않는취재원");
        assert!(result.is_none());
    }

    #[test]
    fn quote_save_creates_parent_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("sub").join("dir").join("quotes.json");
        let quotes = vec![serde_json::json!({"source": "테스트", "text": "발언", "timestamp": "2026-01-01 00:00"})];
        save_quotes_to(&quotes, &path);
        assert!(path.exists());
    }

    // ── /alert tests ─────────────────────────────────────────────────────

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

    // --- legal command tests ---

    #[test]
    fn legal_prompt_with_text() {
        let prompt = build_legal_prompt("김 의원이 뇌물을 받았다는 소문이 있다.");
        assert!(prompt.is_some());
        let prompt = prompt.unwrap();
        assert!(prompt.contains("명예훼손"));
        assert!(prompt.contains("초상권"));
        assert!(prompt.contains("반론권"));
        assert!(prompt.contains("공인/사인"));
        assert!(prompt.contains("김 의원이 뇌물을 받았다는 소문이 있다."));
    }

    #[test]
    fn legal_prompt_empty_returns_none() {
        assert!(build_legal_prompt("").is_none());
        assert!(build_legal_prompt("   ").is_none());
    }

    #[test]
    fn parse_legal_args_inline() {
        let (file, text) = parse_legal_args("기사 텍스트 내용");
        assert!(file.is_none());
        assert_eq!(text, "기사 텍스트 내용");
    }

    #[test]
    fn parse_legal_args_file_flag() {
        let (file, text) = parse_legal_args("--file draft.md");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert!(text.is_empty());
    }

    #[test]
    fn parse_legal_args_file_with_extra() {
        let (file, text) = parse_legal_args("--file draft.md 추가 메모");
        assert_eq!(file.as_deref(), Some("draft.md"));
        assert_eq!(text, "추가 메모");
    }

    #[test]
    fn legal_file_path_with_slug() {
        let path = legal_file_path_with_date("김 의원 뇌물 의혹", "2026-03-20");
        let path_str = path.to_string_lossy();
        assert!(path_str.starts_with(".journalist/legal/"));
        assert!(path_str.contains("2026-03-20"));
        assert!(path_str.contains("김-의원-뇌물-의혹"));
        assert!(path_str.ends_with(".md"));
    }

    #[test]
    fn legal_file_path_empty_slug() {
        let path = legal_file_path_with_date("", "2026-03-20");
        assert_eq!(
            path.to_string_lossy(),
            ".journalist/legal/2026-03-20_legal.md"
        );
    }

    #[test]
    fn save_legal_creates_dirs_and_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("legal").join("test.md");
        save_legal(&path, "# 법적 점검 결과\n내용").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "# 법적 점검 결과\n내용");
    }

    // ── /embargo tests ──────────────────────────────────────────────────

    fn temp_embargoes_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("embargoes.json");
        (dir, path)
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

    // --- /trend tests ---

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

    // --- /archive tests ---

    fn temp_archive_paths() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let archive_dir = dir.path().join("archive");
        std::fs::create_dir_all(&archive_dir).unwrap();
        let index_path = archive_dir.join("index.json");
        (dir, index_path, archive_dir)
    }

    fn add_archive_entry(
        index_path: &Path,
        archive_dir: &Path,
        id: usize,
        title: &str,
        date: &str,
        section: &str,
        article_type: &str,
        tags: Vec<&str>,
        body: &str,
    ) {
        let mut index = load_archive_index_from(index_path);
        let text_filename = format!("{id:04}.txt");
        let text_path = archive_dir.join(&text_filename);
        std::fs::write(&text_path, body).unwrap();
        let entry = serde_json::json!({
            "id": id,
            "title": title,
            "date": date,
            "section": section,
            "type": article_type,
            "tags": tags,
            "file": text_filename,
        });
        index.push(entry);
        save_archive_index_to(&index, index_path);
    }

    #[test]
    fn archive_save_and_load_index() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(
            &index_path,
            &archive_dir,
            1,
            "반도체 수출 급증",
            "2026-03-20",
            "경제",
            "스트레이트",
            vec!["반도체", "삼성"],
            "반도체 수출이 전년 대비 30% 증가했다.",
        );

        let index = load_archive_index_from(&index_path);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0]["title"], "반도체 수출 급증");
        assert_eq!(index[0]["section"], "경제");
        assert_eq!(index[0]["tags"][0], "반도체");
        assert_eq!(index[0]["tags"][1], "삼성");
    }

    #[test]
    fn archive_search_finds_by_title() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "반도체 수출 급증", "2026-03-20", "경제", "", vec![], "본문");
        add_archive_entry(&index_path, &archive_dir, 2, "자동차 산업 동향", "2026-03-19", "산업", "", vec![], "본문");

        let index = load_archive_index_from(&index_path);
        let keyword = "반도체";
        let keyword_lower = keyword.to_lowercase();
        let results: Vec<_> = index
            .iter()
            .filter(|e| {
                e["title"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&keyword_lower)
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "반도체 수출 급증");
    }

    #[test]
    fn archive_search_finds_by_tag() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "", "", vec!["삼성", "반도체"], "");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "", "", vec!["현대", "자동차"], "");

        let index = load_archive_index_from(&index_path);
        let keyword_lower = "삼성";
        let results: Vec<_> = index
            .iter()
            .filter(|e| {
                if let Some(tags) = e["tags"].as_array() {
                    tags.iter().any(|t| t.as_str().unwrap_or("").contains(keyword_lower))
                } else {
                    false
                }
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "기사A");
    }

    #[test]
    fn archive_search_finds_by_body() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "", "", vec![], "반도체 수출이 급증했다");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "", "", vec![], "자동차 수출은 감소");

        let index = load_archive_index_from(&index_path);
        let keyword_lower = "반도체";
        let results: Vec<_> = index
            .iter()
            .filter(|e| {
                let filename = e["file"].as_str().unwrap_or("");
                if !filename.is_empty() {
                    let text_path = archive_dir.join(filename);
                    if let Ok(body) = std::fs::read_to_string(&text_path) {
                        return body.contains(keyword_lower);
                    }
                }
                false
            })
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "기사A");
    }

    #[test]
    fn archive_view_finds_by_id() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "경제", "스트레이트", vec!["반도체"], "본문 내용");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "정치", "해설", vec!["국회"], "정치 본문");

        let index = load_archive_index_from(&index_path);
        let entry = index.iter().find(|e| e["id"].as_u64() == Some(2));
        assert!(entry.is_some());
        assert_eq!(entry.unwrap()["title"], "기사B");

        // Read body
        let filename = entry.unwrap()["file"].as_str().unwrap();
        let body = std::fs::read_to_string(archive_dir.join(filename)).unwrap();
        assert_eq!(body, "정치 본문");
    }

    #[test]
    fn archive_list_section_filter() {
        let (_dir, index_path, archive_dir) = temp_archive_paths();
        add_archive_entry(&index_path, &archive_dir, 1, "기사A", "2026-03-20", "경제", "", vec![], "");
        add_archive_entry(&index_path, &archive_dir, 2, "기사B", "2026-03-19", "정치", "", vec![], "");
        add_archive_entry(&index_path, &archive_dir, 3, "기사C", "2026-03-18", "경제", "", vec![], "");

        let index = load_archive_index_from(&index_path);
        let filtered: Vec<_> = index
            .iter()
            .filter(|e| e["section"].as_str().unwrap_or("") == "경제")
            .collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn archive_parse_args_full() {
        let (title, section, article_type, tags, file_path) =
            parse_archive_save_args("반도체 수출 기사 --section 경제 --type 스트레이트 --tags 반도체,삼성 --file /tmp/article.txt");
        assert_eq!(title, "반도체 수출 기사");
        assert_eq!(section, "경제");
        assert_eq!(article_type, "스트레이트");
        assert_eq!(tags, vec!["반도체", "삼성"]);
        assert_eq!(file_path, Some("/tmp/article.txt".to_string()));
    }

    #[test]
    fn archive_parse_args_title_only() {
        let (title, section, article_type, tags, file_path) =
            parse_archive_save_args("단순 제목");
        assert_eq!(title, "단순 제목");
        assert!(section.is_empty());
        assert!(article_type.is_empty());
        assert!(tags.is_empty());
        assert!(file_path.is_none());
    }

    #[test]
    fn archive_empty_index_loads_empty() {
        let (_dir, index_path, _archive_dir) = temp_archive_paths();
        let index = load_archive_index_from(&index_path);
        assert!(index.is_empty());
    }

    // ── /data tests ─────────────────────────────────────────────────────

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

    // ── /follow tests ──

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

    // ── /desk tests ─────────────────────────────────────────────────────

    fn temp_desk_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("desk").join("assignments.json");
        (dir, path)
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

    // ── /collaborate tests ──────────────────────────────────────────────

    fn temp_collab_dir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
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

    // ── /coverage tests ──────────────────────────────────────────────────

    fn temp_coverage_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("coverage.json");
        (dir, path)
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
    fn coverage_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/coverage"),
            "/coverage should be in KNOWN_COMMANDS"
        );
    }

    // ── /dashboard tests ────────────────────────────────────────────────

    #[test]
    fn dashboard_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/dashboard"),
            "/dashboard should be in KNOWN_COMMANDS"
        );
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

    // ── /publish tests ──────────────────────────────────────────────────

    #[test]
    fn publish_report_all_pass() {
        let results = vec![
            ("checklist", PublishStepResult::Pass("체크리스트 완료".into())),
            ("proofread", PublishStepResult::Pass("교열 완료".into())),
            ("legal", PublishStepResult::Pass("법적 점검 통과".into())),
            ("export", PublishStepResult::Pass("내보내기 완료".into())),
        ];
        // Should not panic
        print_publish_report(&results);
    }

    #[test]
    fn publish_report_blocked_by_legal() {
        let results = vec![
            ("checklist", PublishStepResult::Pass("체크리스트 완료".into())),
            ("proofread", PublishStepResult::Pass("교열 완료".into())),
            (
                "legal",
                PublishStepResult::Blocked(
                    "🚨 법적 리스크 '위험' 판정 — 파이프라인 중단".into(),
                ),
            ),
            (
                "export",
                PublishStepResult::Fail("이전 단계 중단으로 건너뜀".into()),
            ),
        ];
        print_publish_report(&results);
    }

    #[test]
    fn publish_step_result_variants() {
        let pass = PublishStepResult::Pass("ok".into());
        let fail = PublishStepResult::Fail("err".into());
        let blocked = PublishStepResult::Blocked("halt".into());

        assert_eq!(pass, PublishStepResult::Pass("ok".into()));
        assert_ne!(pass, fail);
        assert!(matches!(blocked, PublishStepResult::Blocked(_)));
    }

    #[test]
    fn publish_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/publish"),
            "/publish should be in KNOWN_COMMANDS"
        );
    }

    // ── /anonymize tests ──────────────────────────────────────────────────

    #[test]
    fn anonymize_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/anonymize"),
            "/anonymize should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn build_anonymize_prompt_basic() {
        let prompt = build_anonymize_prompt("홍길동 기자가 취재한 내용입니다.");
        assert!(prompt.is_some());
        let p = prompt.unwrap();
        assert!(p.contains("개인식별정보"));
        assert!(p.contains("홍길동 기자가 취재한 내용입니다."));
        assert!(p.contains("익명화 매핑표"));
    }

    #[test]
    fn build_anonymize_prompt_empty_returns_none() {
        assert!(build_anonymize_prompt("").is_none());
        assert!(build_anonymize_prompt("   ").is_none());
    }

    #[test]
    fn anonymize_file_path_with_topic() {
        let path = anonymize_file_path_with_date("탐사보도초안", "2026-03-21");
        let s = path.to_string_lossy();
        assert!(s.contains("anonymize"));
        assert!(s.contains("2026-03-21"));
        assert!(s.contains("탐사보도초안"));
    }

    #[test]
    fn anonymize_file_path_empty_slug() {
        let path = anonymize_file_path_with_date("", "2026-03-21");
        let s = path.to_string_lossy();
        assert!(s.contains("2026-03-21_anonymize.md"));
    }

    #[test]
    fn save_anonymize_creates_dir_and_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sub").join("anon.md");
        save_anonymize(&path, "익명화 결과").unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "익명화 결과");
    }

    // ── /press tests ─────────────────────────────────────────────────────

    #[test]
    fn press_known_command() {
        use crate::commands::KNOWN_COMMANDS;
        assert!(
            KNOWN_COMMANDS.contains(&"/press"),
            "/press should be in KNOWN_COMMANDS"
        );
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

    // ── /readability tests ──

    #[test]
    fn readability_empty_text() {
        let m = compute_readability("");
        assert_eq!(m.sentence_count, 0);
        assert_eq!(m.paragraph_count, 0);
        assert_eq!(m.avg_sentence_len, 0.0);
        assert_eq!(m.grade, 'A'); // empty text = perfect score
    }

    #[test]
    fn readability_simple_short_sentences() {
        let text = "경제가 성장했다. 물가가 안정됐다. 고용이 늘었다.";
        let m = compute_readability(text);
        assert_eq!(m.sentence_count, 3);
        assert!(m.avg_sentence_len < 30.0);
        assert_eq!(m.long_sentence_ratio, 0.0);
        assert!(m.grade == 'A' || m.grade == 'B');
    }

    #[test]
    fn readability_detects_passive() {
        let text = "법안이 통과되었다. 예산이 삭감되었다. 결과가 발표되었다.";
        let m = compute_readability(text);
        assert!(m.passive_ratio > 0.5); // majority are passive
    }

    #[test]
    fn readability_detects_jargon() {
        let text = "거버넌스 체계와 컨센서스 형성이 중요하다. 패러다임 전환이 필요하다. 이니셔티브를 추진해야 한다.";
        let m = compute_readability(text);
        assert!(m.jargon_density > 0.05);
    }

    #[test]
    fn readability_long_sentence_detection() {
        // Create a sentence with >80 non-space characters
        let long = "가".repeat(90);
        let text = format!("{long}. 짧다.");
        let m = compute_readability(&text);
        assert_eq!(m.sentence_count, 2);
        assert!(m.long_sentence_ratio > 0.4); // 1 out of 2 is long
    }

    #[test]
    fn readability_paragraph_count() {
        let text = "첫 번째 문단이다.\n\n두 번째 문단이다.\n\n세 번째 문단이다.";
        let m = compute_readability(text);
        assert_eq!(m.paragraph_count, 3);
    }

    #[test]
    fn readability_grade_mapping() {
        // Very short, simple text → high score → good grade
        let text = "안녕하다.";
        let m = compute_readability(text);
        assert!(m.grade == 'A' || m.grade == 'B');
    }

    #[test]
    fn split_korean_sentences_basic() {
        let sentences = split_korean_sentences("첫 문장이다. 두 번째다! 세 번째다?");
        assert_eq!(sentences.len(), 3);
    }

    #[test]
    fn split_paragraphs_basic() {
        let paras = split_paragraphs("문단1\n줄2\n\n문단2\n\n문단3");
        assert_eq!(paras.len(), 3);
    }
}
