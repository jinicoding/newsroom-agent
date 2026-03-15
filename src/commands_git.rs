//! Git-related command handlers: /diff, /undo, /commit, /pr, /git, /review.

use crate::commands::auto_compact_if_needed;
use crate::format::*;
use crate::git::*;
use crate::prompt::*;

use std::io::{self, Write};
use yoagent::agent::Agent;
use yoagent::*;

// ── /diff ────────────────────────────────────────────────────────────────

/// A parsed line from `git diff --stat` output.
/// Example: " src/main.rs | 42 +++++++++-------"
#[derive(Debug, Clone, PartialEq)]
pub struct DiffStatEntry {
    pub file: String,
    pub insertions: u32,
    pub deletions: u32,
}

/// Summary totals from `git diff --stat` output.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffStatSummary {
    pub entries: Vec<DiffStatEntry>,
    pub total_insertions: u32,
    pub total_deletions: u32,
}

/// Parse `git diff --stat` output into structured entries.
///
/// Each line looks like:
///   " src/commands.rs | 42 +++++++++-------"
/// The last line is a summary like:
///   " 3 files changed, 25 insertions(+), 10 deletions(-)"
pub fn parse_diff_stat(stat_output: &str) -> DiffStatSummary {
    let mut entries = Vec::new();
    let mut total_insertions: u32 = 0;
    let mut total_deletions: u32 = 0;

    for line in stat_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to parse summary line: "N file(s) changed, N insertion(s)(+), N deletion(s)(-)"
        if trimmed.contains("changed")
            && (trimmed.contains("insertion") || trimmed.contains("deletion"))
        {
            // Parse insertions
            if let Some(ins_part) = trimmed.split("insertion").next() {
                if let Some(num_str) = ins_part.split(',').next_back() {
                    if let Ok(n) = num_str.trim().parse::<u32>() {
                        total_insertions = n;
                    }
                }
            }
            // Parse deletions
            if let Some(del_part) = trimmed.split("deletion").next() {
                if let Some(num_str) = del_part.split(',').next_back() {
                    if let Ok(n) = num_str.trim().parse::<u32>() {
                        total_deletions = n;
                    }
                }
            }
            continue;
        }

        // Try to parse file entry: "file | N +++---" or "file | Bin 0 -> 1234 bytes"
        if let Some(pipe_pos) = trimmed.find('|') {
            let file = trimmed[..pipe_pos].trim().to_string();
            let stats_part = trimmed[pipe_pos + 1..].trim();

            if file.is_empty() {
                continue;
            }

            // Count + and - characters in the visual bar
            let insertions = stats_part.chars().filter(|&c| c == '+').count() as u32;
            let deletions = stats_part.chars().filter(|&c| c == '-').count() as u32;

            entries.push(DiffStatEntry {
                file,
                insertions,
                deletions,
            });
        }
    }

    // If no summary line was found, compute totals from entries
    if total_insertions == 0 && total_deletions == 0 {
        total_insertions = entries.iter().map(|e| e.insertions).sum();
        total_deletions = entries.iter().map(|e| e.deletions).sum();
    }

    DiffStatSummary {
        entries,
        total_insertions,
        total_deletions,
    }
}

/// Format a diff stat summary with colors for display.
pub fn format_diff_stat(summary: &DiffStatSummary) -> String {
    let mut output = String::new();

    if summary.entries.is_empty() {
        return output;
    }

    // Find max filename length for alignment
    let max_name_len = summary
        .entries
        .iter()
        .map(|e| e.file.len())
        .max()
        .unwrap_or(0);

    output.push_str(&format!("{DIM}  File summary:{RESET}\n"));
    for entry in &summary.entries {
        let total_changes = entry.insertions + entry.deletions;
        let ins_str = if entry.insertions > 0 {
            format!("{GREEN}+{}{RESET}", entry.insertions)
        } else {
            String::new()
        };
        let del_str = if entry.deletions > 0 {
            format!("{RED}-{}{RESET}", entry.deletions)
        } else {
            String::new()
        };
        let sep = if entry.insertions > 0 && entry.deletions > 0 {
            " "
        } else {
            ""
        };
        output.push_str(&format!(
            "    {:<width$}  {}{DIM}{:>4}{RESET} {ins_str}{sep}{del_str}\n",
            entry.file,
            "",
            total_changes,
            width = max_name_len,
        ));
    }

    // Summary line
    let files_count = summary.entries.len();
    output.push_str(&format!(
        "\n  {DIM}{files_count} file{s} changed{RESET}",
        s = if files_count == 1 { "" } else { "s" }
    ));
    if summary.total_insertions > 0 {
        output.push_str(&format!(", {GREEN}+{}{RESET}", summary.total_insertions));
    }
    if summary.total_deletions > 0 {
        output.push_str(&format!(", {RED}-{}{RESET}", summary.total_deletions));
    }
    output.push('\n');

    output
}

pub fn handle_diff() {
    // Check if we're in a git repo
    let status_output = std::process::Command::new("git")
        .args(["status", "--short"])
        .output();

    match status_output {
        Ok(output) if output.status.success() => {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.trim().is_empty() {
                println!("{DIM}  (no uncommitted changes){RESET}\n");
                return;
            }

            // Get the stat summary
            let stat_text = std::process::Command::new("git")
                .args(["diff", "--stat"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            // Also include staged changes in stat
            let staged_stat_text = std::process::Command::new("git")
                .args(["diff", "--cached", "--stat"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            // Show file status list
            println!("{DIM}  Changes:");
            for line in status.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Color by status code
                let (color, rest) = if trimmed.len() >= 2 {
                    let code = &trimmed[..2];
                    match code.chars().next().unwrap_or(' ') {
                        'M' | 'A' | 'R' => (format!("{GREEN}"), trimmed),
                        'D' => (format!("{RED}"), trimmed),
                        '?' => (format!("{YELLOW}"), trimmed),
                        _ => (format!("{DIM}"), trimmed),
                    }
                } else {
                    (format!("{DIM}"), trimmed)
                };
                println!("    {color}{rest}{RESET}");
            }
            println!("{RESET}");

            // Parse and display stat summary
            let combined_stat =
                if !staged_stat_text.trim().is_empty() && !stat_text.trim().is_empty() {
                    format!("{}\n{}", staged_stat_text, stat_text)
                } else if !staged_stat_text.trim().is_empty() {
                    staged_stat_text
                } else {
                    stat_text
                };

            if !combined_stat.trim().is_empty() {
                let summary = parse_diff_stat(&combined_stat);
                let formatted = format_diff_stat(&summary);
                if !formatted.is_empty() {
                    print!("{formatted}");
                }
            }

            // Show the full diff
            let full_diff = std::process::Command::new("git")
                .args(["diff"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();

            if !full_diff.trim().is_empty() {
                println!("\n{DIM}  ── Full diff ──{RESET}");
                for line in full_diff.lines() {
                    if line.starts_with('+') && !line.starts_with("+++") {
                        println!("{GREEN}{line}{RESET}");
                    } else if line.starts_with('-') && !line.starts_with("---") {
                        println!("{RED}{line}{RESET}");
                    } else if line.starts_with("@@") {
                        println!("{CYAN}{line}{RESET}");
                    } else if line.starts_with("diff ") || line.starts_with("index ") {
                        println!("{BOLD}{line}{RESET}");
                    } else {
                        println!("{DIM}{line}{RESET}");
                    }
                }
                println!();
            }
        }
        _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
    }
}

// ── /undo ────────────────────────────────────────────────────────────────

pub fn handle_undo() {
    let diff_output = std::process::Command::new("git")
        .args(["diff", "--stat"])
        .output();
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .output();

    let has_diff = diff_output
        .as_ref()
        .map(|o| o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);
    let untracked_files: Vec<String> = untracked
        .as_ref()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();
    let has_untracked = !untracked_files.is_empty();

    if !has_diff && !has_untracked {
        println!("{DIM}  (nothing to undo — no uncommitted changes){RESET}\n");
    } else {
        if has_diff {
            if let Ok(ref output) = diff_output {
                let diff = String::from_utf8_lossy(&output.stdout);
                println!("{DIM}{diff}{RESET}");
            }
        }
        if has_untracked {
            println!("{DIM}  untracked files:");
            for f in &untracked_files {
                println!("    {f}");
            }
            println!("{RESET}");
        }

        if has_diff {
            let _ = std::process::Command::new("git")
                .args(["checkout", "--", "."])
                .output();
        }
        if has_untracked {
            let _ = std::process::Command::new("git")
                .args(["clean", "-fd"])
                .output();
        }
        println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
    }
}

// ── /commit ──────────────────────────────────────────────────────────────

pub fn handle_commit(input: &str) {
    let arg = input.strip_prefix("/commit").unwrap_or("").trim();
    if !arg.is_empty() {
        let (ok, output) = run_git_commit(arg);
        if ok {
            println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
        } else {
            eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
        }
    } else {
        match get_staged_diff() {
            None => {
                eprintln!("{RED}  error: not in a git repository{RESET}\n");
            }
            Some(diff) if diff.trim().is_empty() => {
                println!("{DIM}  nothing staged — use `git add` first{RESET}\n");
            }
            Some(diff) => {
                let suggested = generate_commit_message(&diff);
                println!("{DIM}  Suggested commit message:{RESET}");
                println!("    {BOLD}{suggested}{RESET}");
                eprint!(
                    "\n  {DIM}({GREEN}y{RESET}{DIM})es / ({RED}n{RESET}{DIM})o / ({CYAN}e{RESET}{DIM})dit: {RESET}"
                );
                io::stderr().flush().ok();
                let mut response = String::new();
                if io::stdin().read_line(&mut response).is_ok() {
                    let response = response.trim().to_lowercase();
                    match response.as_str() {
                        "y" | "yes" | "" => {
                            let (ok, output) = run_git_commit(&suggested);
                            if ok {
                                println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                            } else {
                                eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                            }
                        }
                        "e" | "edit" => {
                            println!("{DIM}  Enter your commit message:{RESET}");
                            eprint!("  > ");
                            io::stderr().flush().ok();
                            let mut custom_msg = String::new();
                            if io::stdin().read_line(&mut custom_msg).is_ok() {
                                let custom_msg = custom_msg.trim();
                                if custom_msg.is_empty() {
                                    println!("{DIM}  (commit cancelled — empty message){RESET}\n");
                                } else {
                                    let (ok, output) = run_git_commit(custom_msg);
                                    if ok {
                                        println!("{GREEN}  ✓ {}{RESET}\n", output.trim());
                                    } else {
                                        eprintln!("{RED}  ✗ {}{RESET}\n", output.trim());
                                    }
                                }
                            }
                        }
                        _ => {
                            println!("{DIM}  (commit cancelled){RESET}\n");
                        }
                    }
                }
            }
        }
    }
}

// ── /pr ──────────────────────────────────────────────────────────────────

/// Represents a parsed `/pr` subcommand.
#[derive(Debug, PartialEq)]
pub enum PrSubcommand {
    List,
    View(u32),
    Diff(u32),
    Comment(u32, String),
    Checkout(u32),
    Create { draft: bool },
    Help,
}

/// Parse the argument string after `/pr` into a `PrSubcommand`.
pub fn parse_pr_args(arg: &str) -> PrSubcommand {
    let arg = arg.trim();
    if arg.is_empty() {
        return PrSubcommand::List;
    }

    let parts: Vec<&str> = arg.splitn(3, char::is_whitespace).collect();

    // Check for "create" subcommand first (before trying to parse as number)
    if parts[0].eq_ignore_ascii_case("create") {
        let draft = parts
            .get(1)
            .map(|s| s.trim_start_matches('-').eq_ignore_ascii_case("draft"))
            .unwrap_or(false);
        return PrSubcommand::Create { draft };
    }

    let number = match parts[0].parse::<u32>() {
        Ok(n) => n,
        Err(_) => return PrSubcommand::Help,
    };

    if parts.len() == 1 {
        return PrSubcommand::View(number);
    }

    match parts[1].to_lowercase().as_str() {
        "diff" => PrSubcommand::Diff(number),
        "checkout" => PrSubcommand::Checkout(number),
        "comment" => {
            let text = if parts.len() == 3 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };
            if text.is_empty() {
                PrSubcommand::Help
            } else {
                PrSubcommand::Comment(number, text)
            }
        }
        _ => PrSubcommand::Help,
    }
}

pub async fn handle_pr(input: &str, agent: &mut Agent, session_total: &mut Usage, model: &str) {
    let arg = input.strip_prefix("/pr").unwrap_or("").trim();
    match parse_pr_args(arg) {
        PrSubcommand::List => {
            match std::process::Command::new("gh")
                .args(["pr", "list", "--limit", "10"])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (no open pull requests){RESET}\n");
                    } else {
                        println!("{DIM}  Open pull requests:");
                        for line in text.lines() {
                            println!("    {line}");
                        }
                        println!("{RESET}");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::View(number) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "view", &num_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    println!("{DIM}{text}{RESET}");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Diff(number) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "diff", &num_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if text.trim().is_empty() {
                        println!("{DIM}  (no diff for PR #{number}){RESET}\n");
                    } else {
                        println!("{DIM}{text}{RESET}");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Comment(number, text) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "comment", &num_str, "--body", &text])
                .output()
            {
                Ok(output) if output.status.success() => {
                    println!("{GREEN}  ✓ comment added to PR #{number}{RESET}\n");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Checkout(number) => {
            let num_str = number.to_string();
            match std::process::Command::new("gh")
                .args(["pr", "checkout", &num_str])
                .output()
            {
                Ok(output) if output.status.success() => {
                    println!("{GREEN}  ✓ checked out PR #{number}{RESET}\n");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Create { draft } => {
            // 1. Detect current branch
            let branch = match git_branch() {
                Some(b) => b,
                None => {
                    eprintln!("{RED}  error: not in a git repository{RESET}\n");
                    return;
                }
            };
            let base = detect_base_branch();

            if branch == base {
                eprintln!(
                    "{RED}  error: already on {base} — switch to a feature branch first{RESET}\n"
                );
                return;
            }

            // 2. Get diff and commits
            let diff = get_branch_diff(&base).unwrap_or_default();
            let commits = get_branch_commits(&base).unwrap_or_default();

            if diff.trim().is_empty() && commits.trim().is_empty() {
                println!(
                    "{DIM}  (no changes between {branch} and {base} — nothing to create a PR for){RESET}\n"
                );
                return;
            }

            // 3. Show what we found
            let commit_count = commits.lines().filter(|l| !l.is_empty()).count();
            println!(
                "{DIM}  Branch: {branch} → {base} ({commit_count} commit{s}){RESET}",
                s = if commit_count == 1 { "" } else { "s" }
            );
            println!("{DIM}  Generating PR description with AI...{RESET}");

            // 4. Ask AI to generate title + description
            let prompt = build_pr_description_prompt(&branch, &base, &commits, &diff);
            let response = run_prompt(agent, &prompt, session_total, model).await;

            // 5. Parse the AI's response
            let (title, body) = match parse_pr_description(&response) {
                Some(parsed) => parsed,
                None => {
                    eprintln!(
                        "{RED}  error: could not parse AI response into PR title/description{RESET}"
                    );
                    eprintln!("{DIM}  (try again or create manually with `gh pr create`){RESET}\n");
                    return;
                }
            };

            println!("{DIM}  Title: {BOLD}{title}{RESET}");
            println!("{DIM}  Draft: {}{RESET}", if draft { "yes" } else { "no" });

            // 6. Create the PR via gh CLI
            let mut gh_args = vec![
                "pr".to_string(),
                "create".to_string(),
                "--title".to_string(),
                title.clone(),
                "--body".to_string(),
                body,
                "--base".to_string(),
                base.clone(),
            ];
            if draft {
                gh_args.push("--draft".to_string());
            }

            let gh_str_args: Vec<&str> = gh_args.iter().map(|s| s.as_str()).collect();
            match std::process::Command::new("gh").args(&gh_str_args).output() {
                Ok(output) if output.status.success() => {
                    let url = String::from_utf8_lossy(&output.stdout);
                    let url = url.trim();
                    if url.is_empty() {
                        println!("{GREEN}  ✓ PR created: {title}{RESET}\n");
                    } else {
                        println!("{GREEN}  ✓ PR created: {url}{RESET}\n");
                    }
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("{RED}  error: {}{RESET}\n", stderr.trim());
                }
                Err(_) => {
                    eprintln!("{RED}  error: `gh` CLI not found. Install it from https://cli.github.com{RESET}\n");
                }
            }
        }
        PrSubcommand::Help => {
            println!("{DIM}  usage: /pr                         List open pull requests");
            println!(
                "         /pr create [--draft]        Create PR with AI-generated description"
            );
            println!("         /pr <number>                View details of a specific PR");
            println!("         /pr <number> diff           Show the diff of a PR");
            println!("         /pr <number> comment <text> Add a comment to a PR");
            println!("         /pr <number> checkout       Checkout a PR locally{RESET}\n");
        }
    }
}

// ── /git ─────────────────────────────────────────────────────────────────

pub fn handle_git(input: &str) {
    let arg = input.strip_prefix("/git").unwrap_or("").trim();
    let subcmd = parse_git_args(arg);
    run_git_subcommand(&subcmd);
}

// ── /review ──────────────────────────────────────────────────────────────

/// Build a review prompt for either staged changes or a specific file.
/// Returns None if there's nothing to review, Some(prompt) otherwise.
pub fn build_review_content(arg: &str) -> Option<(String, String)> {
    let arg = arg.trim();
    if arg.is_empty() {
        // Review staged changes
        match get_staged_diff() {
            None => {
                eprintln!("{RED}  error: not in a git repository{RESET}\n");
                None
            }
            Some(diff) if diff.trim().is_empty() => {
                // Fall back to unstaged diff if nothing staged
                let unstaged = std::process::Command::new("git")
                    .args(["diff"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_default();
                if unstaged.trim().is_empty() {
                    println!("{DIM}  nothing to review — no staged or unstaged changes{RESET}\n");
                    None
                } else {
                    println!("{DIM}  reviewing unstaged changes...{RESET}");
                    Some(("unstaged changes".to_string(), unstaged))
                }
            }
            Some(diff) => {
                println!("{DIM}  reviewing staged changes...{RESET}");
                Some(("staged changes".to_string(), diff))
            }
        }
    } else {
        // Review a specific file
        let path = std::path::Path::new(arg);
        if !path.exists() {
            eprintln!("{RED}  error: file not found: {arg}{RESET}\n");
            return None;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    println!("{DIM}  file is empty — nothing to review{RESET}\n");
                    None
                } else {
                    println!("{DIM}  reviewing {arg}...{RESET}");
                    Some((arg.to_string(), content))
                }
            }
            Err(e) => {
                eprintln!("{RED}  error reading {arg}: {e}{RESET}\n");
                None
            }
        }
    }
}

/// Build the review prompt to send to the AI.
pub fn build_review_prompt(label: &str, content: &str) -> String {
    // Truncate if very large
    let max_chars = 30_000;
    let content_preview = if content.len() > max_chars {
        let truncated = &content[..max_chars];
        format!(
            "{truncated}\n\n... (truncated, {} more chars)",
            content.len() - max_chars
        )
    } else {
        content.to_string()
    };

    format!(
        r#"Review the following code ({label}). Look for:

1. **Bugs** — logic errors, off-by-one errors, null/None handling, race conditions
2. **Security** — injection vulnerabilities, unsafe operations, credential exposure
3. **Style** — naming, idiomatic patterns, unnecessary complexity, dead code
4. **Performance** — obvious inefficiencies, unnecessary allocations, N+1 patterns
5. **Suggestions** — improvements, missing error handling, better approaches

Be specific: reference line numbers or code snippets. Be concise — skip things that look fine.
If the code looks good overall, say so briefly and note any minor suggestions.

```
{content_preview}
```"#
    )
}

/// Handle the /review command: review staged changes or a specific file.
/// Returns the review prompt if sent to AI, None otherwise.
pub async fn handle_review(
    input: &str,
    agent: &mut Agent,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let arg = input.strip_prefix("/review").unwrap_or("").trim();

    match build_review_content(arg) {
        Some((label, content)) => {
            let prompt = build_review_prompt(&label, &content);
            run_prompt(agent, &prompt, session_total, model).await;
            auto_compact_if_needed(agent);
            Some(prompt)
        }
        None => None,
    }
}
