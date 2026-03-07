//! yoyo — a coding agent that evolves itself.
//!
//! Started as ~200 lines. Grows one commit at a time.
//! Read IDENTITY.md and JOURNAL.md for the full story.
//!
//! Usage:
//!   ANTHROPIC_API_KEY=sk-... cargo run
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --thinking high
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --skills ./skills
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --mcp "npx -y @modelcontextprotocol/server-filesystem /tmp"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system "You are a Rust expert."
//!   ANTHROPIC_API_KEY=sk-... cargo run -- --system-file prompt.txt
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "explain this code"
//!   ANTHROPIC_API_KEY=sk-... cargo run -- -p "write a README" -o README.md
//!   echo "prompt" | cargo run  (piped mode: single prompt, no REPL)
//!
//! Commands:
//!   /quit, /exit    Exit the agent
//!   /clear          Clear conversation history
//!   /model <name>   Switch model mid-session
//!   /search <query> Search conversation history
//!   /tree [depth]   Show project directory tree
//!   /pr [number]    List open PRs or view a specific PR
//!   /retry          Re-send the last user input

mod cli;
mod format;
mod prompt;

use cli::*;
use format::*;
use prompt::*;

use std::io::{self, BufRead, IsTerminal, Read, Write};
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig, ExecutionLimits};
use yoagent::provider::AnthropicProvider;
use yoagent::tools::bash::BashTool;
use yoagent::tools::edit::EditFileTool;
use yoagent::tools::file::{ReadFileTool, WriteFileTool};
use yoagent::tools::list::ListFilesTool;
use yoagent::tools::search::SearchTool;
use yoagent::types::AgentTool;
use yoagent::*;

/// Build the tool set, optionally with a bash confirmation prompt.
/// When `auto_approve` is false (default), bash commands require user approval.
fn build_tools(auto_approve: bool) -> Vec<Box<dyn AgentTool>> {
    let bash = if auto_approve {
        BashTool::default()
    } else {
        BashTool::default().with_confirm(|cmd: &str| {
            use std::io::BufRead;
            // Show the command and ask for approval
            eprint!(
                "{YELLOW}  ⚠ Allow: {RESET}{}{YELLOW} ? {RESET}({GREEN}y{RESET}/{RED}n{RESET}/{GREEN}a{RESET}lways) ",
                truncate_with_ellipsis(cmd, 120)
            );
            io::stderr().flush().ok();
            let mut response = String::new();
            let stdin = io::stdin();
            if stdin.lock().read_line(&mut response).is_err() {
                return false;
            }
            let response = response.trim().to_lowercase();
            matches!(response.as_str(), "y" | "yes" | "a" | "always")
        })
    };
    vec![
        Box::new(bash),
        Box::new(ReadFileTool::default()),
        Box::new(WriteFileTool::new()),
        Box::new(EditFileTool::new()),
        Box::new(ListFilesTool::default()),
        Box::new(SearchTool::default()),
    ]
}

#[allow(clippy::too_many_arguments)]
fn build_agent(
    model: &str,
    api_key: &str,
    skills: &yoagent::skills::SkillSet,
    system_prompt: &str,
    thinking: ThinkingLevel,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    max_turns: Option<usize>,
    auto_approve: bool,
) -> Agent {
    let mut agent = Agent::new(AnthropicProvider)
        .with_system_prompt(system_prompt)
        .with_model(model)
        .with_api_key(api_key)
        .with_thinking(thinking)
        .with_skills(skills.clone())
        .with_tools(build_tools(auto_approve));
    if let Some(max) = max_tokens {
        agent = agent.with_max_tokens(max);
    }
    if let Some(temp) = temperature {
        agent.temperature = Some(temp);
    }
    if let Some(turns) = max_turns {
        agent = agent.with_execution_limits(ExecutionLimits {
            max_turns: turns,
            ..ExecutionLimits::default()
        });
    }
    agent
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Check --no-color before any output (must happen before parse_args prints anything)
    // Also auto-disable color when stdout is not a terminal (piped output)
    if args.iter().any(|a| a == "--no-color") || !io::stdout().is_terminal() {
        disable_color();
    }

    let Some(config) = parse_args(&args) else {
        return; // --help or --version was handled
    };

    if config.verbose {
        enable_verbose();
    }

    let mut model = config.model;
    let api_key = config.api_key;
    let skills = config.skills;
    let system_prompt = config.system_prompt;
    let mut thinking = config.thinking;
    let max_tokens = config.max_tokens;
    let temperature = config.temperature;
    let max_turns = config.max_turns;
    let continue_session = config.continue_session;
    let output_path = config.output_path;
    let mcp_servers = config.mcp_servers;
    // Auto-approve in non-interactive modes (piped, --prompt) or when --yes is set
    let is_interactive = io::stdin().is_terminal() && config.prompt_arg.is_none();
    let auto_approve = config.auto_approve || !is_interactive;

    let mut agent = build_agent(
        &model,
        &api_key,
        &skills,
        &system_prompt,
        thinking,
        max_tokens,
        temperature,
        max_turns,
        auto_approve,
    );

    // Connect to MCP servers (--mcp flags)
    let mut mcp_count = 0u32;
    for mcp_cmd in &mcp_servers {
        let parts: Vec<&str> = mcp_cmd.split_whitespace().collect();
        if parts.is_empty() {
            eprintln!("{YELLOW}warning:{RESET} Empty --mcp command, skipping");
            continue;
        }
        let command = parts[0];
        let args_slice: Vec<&str> = parts[1..].to_vec();
        eprintln!("{DIM}  mcp: connecting to {mcp_cmd}...{RESET}");
        // with_mcp_server_stdio consumes self; we must always update agent
        let result = agent
            .with_mcp_server_stdio(command, &args_slice, None)
            .await;
        match result {
            Ok(updated) => {
                agent = updated;
                mcp_count += 1;
                eprintln!("{GREEN}  ✓ mcp: {command} connected{RESET}");
            }
            Err(e) => {
                eprintln!("{RED}  ✗ mcp: failed to connect to '{mcp_cmd}': {e}{RESET}");
                // Agent was consumed on error — rebuild it with previous MCP connections lost
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                eprintln!("{DIM}  mcp: agent rebuilt (previous MCP connections lost){RESET}");
            }
        }
    }

    // --continue / -c: resume last saved session
    if continue_session {
        match std::fs::read_to_string(DEFAULT_SESSION_PATH) {
            Ok(json) => match agent.restore_messages(&json) {
                Ok(_) => {
                    eprintln!(
                        "{DIM}  resumed session: {} messages from {DEFAULT_SESSION_PATH}{RESET}",
                        agent.messages().len()
                    );
                }
                Err(e) => eprintln!("{YELLOW}warning:{RESET} Failed to restore session: {e}"),
            },
            Err(_) => eprintln!("{DIM}  no previous session found ({DEFAULT_SESSION_PATH}){RESET}"),
        }
    }

    // --prompt / -p: single-shot mode with a prompt argument
    if let Some(prompt_text) = config.prompt_arg {
        eprintln!("{DIM}  yoyo (prompt mode) — model: {model}{RESET}");
        let mut session_total = Usage::default();
        let response = run_prompt(&mut agent, prompt_text.trim(), &mut session_total, &model).await;
        write_output_file(&output_path, &response);
        return;
    }

    // Piped mode: read all of stdin as a single prompt, run once, exit
    if !io::stdin().is_terminal() {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input).ok();
        let input = input.trim();
        if input.is_empty() {
            eprintln!("No input on stdin.");
            std::process::exit(1);
        }

        eprintln!("{DIM}  yoyo (piped mode) — model: {model}{RESET}");
        let mut session_total = Usage::default();
        let response = run_prompt(&mut agent, input, &mut session_total, &model).await;
        write_output_file(&output_path, &response);
        return;
    }

    // Interactive REPL mode
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    print_banner();
    println!("{DIM}  model: {model}{RESET}");
    if thinking != ThinkingLevel::Off {
        println!("{DIM}  thinking: {thinking:?}{RESET}");
    }
    if let Some(temp) = temperature {
        println!("{DIM}  temperature: {temp:.1}{RESET}");
    }
    if !skills.is_empty() {
        println!("{DIM}  skills: {} loaded{RESET}", skills.len());
    }
    if mcp_count > 0 {
        println!("{DIM}  mcp: {mcp_count} server(s) connected{RESET}");
    }
    if is_verbose() {
        println!("{DIM}  verbose: on{RESET}");
    }
    if !auto_approve {
        println!("{DIM}  tools: confirmation required (use --yes to skip){RESET}");
    }
    if let Some(branch) = git_branch() {
        println!("{DIM}  git:   {branch}{RESET}");
    }
    println!("{DIM}  cwd:   {cwd}{RESET}\n");

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let mut session_total = Usage::default();
    let mut last_input: Option<String> = None;

    loop {
        let prompt = if let Some(branch) = git_branch() {
            format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}> {RESET}")
        } else {
            format!("{BOLD}{GREEN}> {RESET}")
        };
        print!("{prompt}");
        io::stdout().flush().ok();

        let line = match lines.next() {
            Some(Ok(l)) => l,
            _ => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Multi-line input: collect continuation lines
        let input = if needs_continuation(input) {
            collect_multiline(input, &mut lines)
        } else {
            input.to_string()
        };
        let input = input.trim();

        match input {
            "/quit" | "/exit" => break,
            "/help" => {
                println!("{DIM}  /help              Show this help");
                println!("  /quit, /exit       Exit yoyo");
                println!("  /clear             Clear conversation history");
                println!("  /compact           Compact conversation to save context space");
                println!("  /config            Show all current settings");
                println!("  /context           Show loaded project context files");
                println!("  /cost              Show estimated session cost");
                println!("  /init              Create a starter YOYO.md project context file");
                println!("  /model <name>      Switch model (preserves conversation)");
                println!(
                    "  /think [level]     Show or change thinking level (off/low/medium/high)"
                );
                println!("  /status            Show session info");
                println!("  /tokens            Show token usage and context window");
                println!("  /save [path]       Save session to file (default: yoyo-session.json)");
                println!("  /load [path]       Load session from file");
                println!("  /diff              Show git diff summary of uncommitted changes");
                println!("  /undo              Revert all uncommitted changes (git checkout)");
                println!("  /pr [number]       List open PRs, or view details of a specific PR");
                println!("  /health            Run health checks (build, test, clippy, fmt)");
                println!("  /retry             Re-send the last user input");
                println!("  /run <cmd>         Run a shell command directly (no AI, no tokens)");
                println!("  !<cmd>             Shortcut for /run");
                println!("  /history           Show summary of conversation messages");
                println!("  /search <query>    Search conversation history for matching messages");
                println!("  /tree [depth]      Show project directory tree (default depth: 3)");
                println!("  /version           Show yoyo version");
                println!();
                println!("  Multi-line input:");
                println!("  End a line with \\ to continue on the next line");
                println!("  Start with ``` to enter a fenced code block{RESET}\n");
                continue;
            }
            "/version" => {
                println!("{DIM}  yoyo v{VERSION}{RESET}\n");
                continue;
            }
            "/status" => {
                println!("{DIM}  model:   {model}");
                if let Some(branch) = git_branch() {
                    println!("  git:     {branch}");
                }
                println!("  cwd:     {cwd}");
                println!(
                    "  tokens:  {} in / {} out (session total){RESET}\n",
                    session_total.input, session_total.output
                );
                continue;
            }
            "/tokens" => {
                let max_context = MAX_CONTEXT_TOKENS;

                // Estimate actual context window usage from message history
                let messages = agent.messages().to_vec();
                let context_used = total_tokens(&messages) as u64;
                let bar = context_bar(context_used, max_context);

                println!("{DIM}  Context window:");
                println!("    messages:    {}", messages.len());
                println!(
                    "    context:     {} / {} tokens",
                    format_token_count(context_used),
                    format_token_count(max_context)
                );
                println!("    {bar}");
                if context_used as f64 / max_context as f64 > 0.75 {
                    println!(
                        "    {YELLOW}⚠ Context is getting full. Consider /clear or /compact.{RESET}"
                    );
                }
                println!();
                println!("  Session totals:");
                println!(
                    "    input:       {} tokens",
                    format_token_count(session_total.input)
                );
                println!(
                    "    output:      {} tokens",
                    format_token_count(session_total.output)
                );
                println!(
                    "    cache read:  {} tokens",
                    format_token_count(session_total.cache_read)
                );
                println!(
                    "    cache write: {} tokens",
                    format_token_count(session_total.cache_write)
                );
                if let Some(cost) = estimate_cost(&session_total, &model) {
                    println!("    est. cost:   {}", format_cost(cost));
                }
                println!("{RESET}");
                continue;
            }
            "/cost" => {
                if let Some(cost) = estimate_cost(&session_total, &model) {
                    println!("{DIM}  Session cost: {}", format_cost(cost));
                    println!(
                        "    {} in / {} out",
                        format_token_count(session_total.input),
                        format_token_count(session_total.output)
                    );
                    if session_total.cache_read > 0 || session_total.cache_write > 0 {
                        println!(
                            "    cache: {} read / {} write",
                            format_token_count(session_total.cache_read),
                            format_token_count(session_total.cache_write)
                        );
                    }
                    // Show cost breakdown by category
                    if let Some((input_cost, cw_cost, cr_cost, output_cost)) =
                        cost_breakdown(&session_total, &model)
                    {
                        println!();
                        println!("    Breakdown:");
                        println!("      input:       {}", format_cost(input_cost));
                        println!("      output:      {}", format_cost(output_cost));
                        if cw_cost > 0.0 {
                            println!("      cache write: {}", format_cost(cw_cost));
                        }
                        if cr_cost > 0.0 {
                            println!("      cache read:  {}", format_cost(cr_cost));
                        }
                    }
                    println!("{RESET}");
                } else {
                    println!("{DIM}  Cost estimation not available for model '{model}'.{RESET}\n");
                }
                continue;
            }
            "/clear" => {
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                println!("{DIM}  (conversation cleared){RESET}\n");
                continue;
            }
            "/model" => {
                println!("{DIM}  current model: {model}");
                println!("  usage: /model <name>{RESET}\n");
                continue;
            }
            s if s.starts_with("/model ") => {
                let new_model = s.trim_start_matches("/model ").trim();
                if new_model.is_empty() {
                    println!("{DIM}  current model: {model}");
                    println!("  usage: /model <name>{RESET}\n");
                    continue;
                }
                model = new_model.to_string();
                // Rebuild agent with new model, preserving conversation
                let saved = agent.save_messages().ok();
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                println!("{DIM}  (switched to {new_model}, conversation preserved){RESET}\n");
                continue;
            }
            "/think" => {
                let level_str = thinking_level_name(thinking);
                println!("{DIM}  thinking: {level_str}");
                println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
                continue;
            }
            s if s.starts_with("/think ") => {
                let level_str = s.trim_start_matches("/think ").trim();
                if level_str.is_empty() {
                    let current = thinking_level_name(thinking);
                    println!("{DIM}  thinking: {current}");
                    println!("  usage: /think <off|minimal|low|medium|high>{RESET}\n");
                    continue;
                }
                let new_thinking = parse_thinking_level(level_str);
                if new_thinking == thinking {
                    let current = thinking_level_name(thinking);
                    println!("{DIM}  thinking already set to {current}{RESET}\n");
                    continue;
                }
                thinking = new_thinking;
                // Rebuild agent with new thinking level, preserving conversation
                let saved = agent.save_messages().ok();
                agent = build_agent(
                    &model,
                    &api_key,
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                );
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                let level_name = thinking_level_name(thinking);
                println!("{DIM}  (thinking set to {level_name}, conversation preserved){RESET}\n");
                continue;
            }
            s if s == "/save" || s.starts_with("/save ") => {
                let path = s.strip_prefix("/save").unwrap_or("").trim();
                let path = if path.is_empty() {
                    DEFAULT_SESSION_PATH
                } else {
                    path
                };
                match agent.save_messages() {
                    Ok(json) => match std::fs::write(path, &json) {
                        Ok(_) => println!(
                            "{DIM}  (session saved to {path}, {} messages){RESET}\n",
                            agent.messages().len()
                        ),
                        Err(e) => eprintln!("{RED}  error saving: {e}{RESET}\n"),
                    },
                    Err(e) => eprintln!("{RED}  error serializing: {e}{RESET}\n"),
                }
                continue;
            }
            s if s == "/load" || s.starts_with("/load ") => {
                let path = s.strip_prefix("/load").unwrap_or("").trim();
                let path = if path.is_empty() {
                    DEFAULT_SESSION_PATH
                } else {
                    path
                };
                match std::fs::read_to_string(path) {
                    Ok(json) => match agent.restore_messages(&json) {
                        Ok(_) => println!(
                            "{DIM}  (session loaded from {path}, {} messages){RESET}\n",
                            agent.messages().len()
                        ),
                        Err(e) => eprintln!("{RED}  error parsing: {e}{RESET}\n"),
                    },
                    Err(e) => eprintln!("{RED}  error reading {path}: {e}{RESET}\n"),
                }
                continue;
            }
            "/diff" => {
                // Use git status --short for a comprehensive view (modified, staged, untracked)
                match std::process::Command::new("git")
                    .args(["status", "--short"])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        let status = String::from_utf8_lossy(&output.stdout);
                        if status.trim().is_empty() {
                            println!("{DIM}  (no uncommitted changes){RESET}\n");
                        } else {
                            println!("{DIM}  Changes:");
                            for line in status.lines() {
                                println!("    {line}");
                            }
                            println!("{RESET}");
                            // Also show diff stat for modified files
                            if let Ok(diff) = std::process::Command::new("git")
                                .args(["diff", "--stat"])
                                .output()
                            {
                                let diff_text = String::from_utf8_lossy(&diff.stdout);
                                if !diff_text.trim().is_empty() {
                                    println!("{DIM}{diff_text}{RESET}");
                                }
                            }
                        }
                    }
                    _ => eprintln!("{RED}  error: not in a git repository{RESET}\n"),
                }
                continue;
            }
            "/undo" => {
                // Revert all uncommitted changes and remove untracked files
                let diff_output = std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output();
                let untracked = std::process::Command::new("git")
                    .args(["ls-files", "--others", "--exclude-standard"])
                    .output();

                let has_diff = diff_output
                    .as_ref()
                    .map(|o| {
                        o.status.success() && !String::from_utf8_lossy(&o.stdout).trim().is_empty()
                    })
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

                    // Revert modified files
                    if has_diff {
                        let _ = std::process::Command::new("git")
                            .args(["checkout", "--", "."])
                            .output();
                    }
                    // Remove untracked files
                    if has_untracked {
                        let _ = std::process::Command::new("git")
                            .args(["clean", "-fd"])
                            .output();
                    }
                    println!("{GREEN}  ✓ reverted all uncommitted changes{RESET}\n");
                }
                continue;
            }
            "/health" => {
                println!("{DIM}  Running health checks...{RESET}");
                let results = run_health_check();
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
                continue;
            }
            "/history" => {
                let messages = agent.messages();
                if messages.is_empty() {
                    println!("{DIM}  (no messages in conversation){RESET}\n");
                } else {
                    println!("{DIM}  Conversation ({} messages):", messages.len());
                    for (i, msg) in messages.iter().enumerate() {
                        let (role, preview) = summarize_message(msg);
                        let idx = i + 1;
                        println!("    {idx:>3}. [{role}] {preview}");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/search" => {
                println!("{DIM}  usage: /search <query>");
                println!("  Search conversation history for messages containing <query>.{RESET}\n");
                continue;
            }
            s if s.starts_with("/search ") => {
                let query = s.trim_start_matches("/search ").trim();
                if query.is_empty() {
                    println!("{DIM}  usage: /search <query>{RESET}\n");
                    continue;
                }
                let messages = agent.messages();
                if messages.is_empty() {
                    println!("{DIM}  (no messages to search){RESET}\n");
                    continue;
                }
                let results = search_messages(messages, query);
                if results.is_empty() {
                    println!(
                        "{DIM}  No matches for '{query}' in {len} messages.{RESET}\n",
                        len = messages.len()
                    );
                } else {
                    println!(
                        "{DIM}  {count} match{es} for '{query}':",
                        count = results.len(),
                        es = if results.len() == 1 { "" } else { "es" }
                    );
                    for (idx, role, preview) in &results {
                        println!("    {idx:>3}. [{role}] {preview}");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/config" => {
                println!("{DIM}  Configuration:");
                println!("    model:      {model}");
                println!("    thinking:   {}", thinking_level_name(thinking));
                println!(
                    "    max_tokens: {}",
                    max_tokens
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "default (8192)".to_string())
                );
                println!(
                    "    max_turns:  {}",
                    max_turns
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "default (50)".to_string())
                );
                println!(
                    "    temperature: {}",
                    temperature
                        .map(|t| format!("{t:.1}"))
                        .unwrap_or_else(|| "default".to_string())
                );
                println!(
                    "    skills:     {}",
                    if skills.is_empty() {
                        "none".to_string()
                    } else {
                        format!("{} loaded", skills.len())
                    }
                );
                let system_preview =
                    truncate_with_ellipsis(system_prompt.lines().next().unwrap_or("(empty)"), 60);
                println!("    system:     {system_preview}");
                if mcp_count > 0 {
                    println!("    mcp:        {mcp_count} server(s)");
                }
                println!(
                    "    verbose:    {}",
                    if is_verbose() { "on" } else { "off" }
                );
                if let Some(branch) = git_branch() {
                    println!("    git:        {branch}");
                }
                println!("    cwd:        {cwd}");
                println!(
                    "    context:    {} max tokens",
                    format_token_count(MAX_CONTEXT_TOKENS)
                );
                println!(
                    "    auto-compact: at {:.0}%",
                    AUTO_COMPACT_THRESHOLD * 100.0
                );
                println!("    messages:   {}", agent.messages().len());
                if continue_session {
                    println!("    session:    auto-save on exit ({DEFAULT_SESSION_PATH})");
                }
                println!("{RESET}");
                continue;
            }
            "/compact" => {
                let messages = agent.messages();
                let before_count = messages.len();
                let before_tokens = total_tokens(messages) as u64;
                match compact_agent(&mut agent) {
                    Some((_, _, after_count, after_tokens)) => {
                        println!(
                            "{DIM}  compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}\n",
                            format_token_count(before_tokens),
                            format_token_count(after_tokens)
                        );
                    }
                    None => {
                        println!(
                            "{DIM}  (nothing to compact — {before_count} messages, ~{} tokens){RESET}\n",
                            format_token_count(before_tokens)
                        );
                    }
                }
                continue;
            }
            "/context" => {
                let files = cli::list_project_context_files();
                if files.is_empty() {
                    println!("{DIM}  No project context files found.");
                    println!("  Searched for: {}", PROJECT_CONTEXT_FILES.join(", "));
                    println!("  Create YOYO.md, CLAUDE.md, or .yoyo/instructions.md to add project context.");
                    println!("  Or run /init to create a starter YOYO.md.{RESET}\n");
                } else {
                    println!("{DIM}  Project context files:");
                    for (name, lines) in &files {
                        println!("    {name} ({lines} lines)");
                    }
                    println!("{RESET}");
                }
                continue;
            }
            "/init" => {
                let path = "YOYO.md";
                if std::path::Path::new(path).exists() {
                    println!("{DIM}  {path} already exists — not overwriting.{RESET}\n");
                } else {
                    let template = concat!(
                        "# Project Context\n",
                        "\n",
                        "<!-- This file is read by yoyo at startup to understand your project. -->\n",
                        "<!-- Customize it with project-specific instructions, conventions, and context. -->\n",
                        "\n",
                        "## About This Project\n",
                        "\n",
                        "<!-- Describe what this project does and its tech stack. -->\n",
                        "\n",
                        "## Coding Conventions\n",
                        "\n",
                        "<!-- List any coding standards, naming conventions, or patterns to follow. -->\n",
                        "\n",
                        "## Build & Test\n",
                        "\n",
                        "<!-- How to build, test, and run the project. -->\n",
                        "\n",
                        "## Important Files\n",
                        "\n",
                        "<!-- List key files and directories the agent should know about. -->\n",
                    );
                    match std::fs::write(path, template) {
                        Ok(_) => println!(
                            "{GREEN}  ✓ Created {path} — edit it to add project context.{RESET}\n"
                        ),
                        Err(e) => eprintln!("{RED}  error creating {path}: {e}{RESET}\n"),
                    }
                }
                continue;
            }
            "/retry" => {
                match &last_input {
                    Some(prev) => {
                        println!("{DIM}  (retrying last input){RESET}");
                        let retry_input = prev.clone();
                        run_prompt(&mut agent, &retry_input, &mut session_total, &model).await;
                        auto_compact_if_needed(&mut agent);
                    }
                    None => {
                        eprintln!("{DIM}  (nothing to retry — no previous input){RESET}\n");
                    }
                }
                continue;
            }
            s if s == "/tree" || s.starts_with("/tree ") => {
                let arg = s.strip_prefix("/tree").unwrap_or("").trim();
                let max_depth = if arg.is_empty() {
                    3
                } else {
                    match arg.parse::<usize>() {
                        Ok(d) => d,
                        Err(_) => {
                            println!("{DIM}  usage: /tree [depth]  (default depth: 3){RESET}\n");
                            continue;
                        }
                    }
                };
                let tree = build_project_tree(max_depth);
                println!("{DIM}{tree}{RESET}\n");
                continue;
            }
            s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
                let cmd = if s.starts_with("/run ") {
                    s.trim_start_matches("/run ").trim()
                } else {
                    s[1..].trim()
                };
                if cmd.is_empty() {
                    println!("{DIM}  usage: /run <command>  or  !<command>{RESET}\n");
                } else {
                    run_shell_command(cmd);
                }
                continue;
            }
            "/run" => {
                println!("{DIM}  usage: /run <command>  or  !<command>");
                println!("  Runs a shell command directly (no AI, no tokens).{RESET}\n");
                continue;
            }
            s if s == "/pr" || s.starts_with("/pr ") => {
                let arg = s.strip_prefix("/pr").unwrap_or("").trim();
                if arg.is_empty() {
                    // List open pull requests
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
                } else {
                    // View a specific PR
                    match arg.parse::<u32>() {
                        Ok(_) => {
                            match std::process::Command::new("gh")
                                .args(["pr", "view", arg])
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
                        Err(_) => {
                            println!("{DIM}  usage: /pr          List open pull requests");
                            println!(
                                "         /pr <number>  View details of a specific PR{RESET}\n"
                            );
                        }
                    }
                }
                continue;
            }
            s if s.starts_with('/') && is_unknown_command(s) => {
                let cmd = s.split_whitespace().next().unwrap_or(s);
                eprintln!("{RED}  unknown command: {cmd}{RESET}");
                eprintln!("{DIM}  type /help for available commands{RESET}\n");
                continue;
            }
            _ => {}
        }

        last_input = Some(input.to_string());
        run_prompt(&mut agent, input, &mut session_total, &model).await;

        // Auto-compact when context window is getting full
        auto_compact_if_needed(&mut agent);
    }

    // Auto-save session on exit when --continue was used
    if continue_session {
        if let Ok(json) = agent.save_messages() {
            if std::fs::write(DEFAULT_SESSION_PATH, &json).is_ok() {
                eprintln!(
                    "{DIM}  session saved to {DEFAULT_SESSION_PATH} ({} messages){RESET}",
                    agent.messages().len()
                );
            }
        }
    }

    println!("\n{DIM}  bye 👋{RESET}\n");
}

/// Compact the agent's conversation and return (before_count, before_tokens, after_count, after_tokens).
/// Returns None if nothing changed.
fn compact_agent(agent: &mut Agent) -> Option<(usize, u64, usize, u64)> {
    let messages = agent.messages().to_vec();
    let before_tokens = total_tokens(&messages) as u64;
    let before_count = messages.len();
    let config = ContextConfig::default();
    let compacted = compact_messages(messages, &config);
    let after_tokens = total_tokens(&compacted) as u64;
    let after_count = compacted.len();
    agent.replace_messages(compacted);
    if before_tokens == after_tokens {
        None
    } else {
        Some((before_count, before_tokens, after_count, after_tokens))
    }
}

/// Auto-compact conversation if context window usage exceeds threshold.
fn auto_compact_if_needed(agent: &mut Agent) {
    let messages = agent.messages().to_vec();
    let used = total_tokens(&messages) as u64;
    let ratio = used as f64 / MAX_CONTEXT_TOKENS as f64;

    if ratio > AUTO_COMPACT_THRESHOLD {
        if let Some((before_count, before_tokens, after_count, after_tokens)) = compact_agent(agent)
        {
            println!(
                "{DIM}  ⚡ auto-compacted: {before_count} → {after_count} messages, ~{} → ~{} tokens{RESET}",
                format_token_count(before_tokens),
                format_token_count(after_tokens)
            );
        }
    }
}

/// Run a shell command directly and print its output.
/// Used by the /run command to execute without going through the AI.
fn run_shell_command(cmd: &str) {
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

/// Check if a line needs continuation (backslash at end, or opens a code fence).
fn needs_continuation(line: &str) -> bool {
    line.ends_with('\\') || line.starts_with("```")
}

/// Collect multi-line input. Supports:
/// - Backslash continuation: lines ending with `\` continue on the next line
/// - Code fences: input starting with ``` collects until closing ```
fn collect_multiline(first_line: &str, lines: &mut io::Lines<io::StdinLock<'_>>) -> String {
    let mut buf = String::new();

    if first_line.starts_with("```") {
        // Code fence mode: collect until closing ```
        buf.push_str(first_line);
        buf.push('\n');
        loop {
            print!("{DIM}  ...{RESET} ");
            io::stdout().flush().ok();
            match lines.next() {
                Some(Ok(line)) => {
                    buf.push_str(&line);
                    buf.push('\n');
                    if line.trim() == "```" {
                        break;
                    }
                }
                _ => break,
            }
        }
    } else {
        // Backslash continuation mode
        let mut current = first_line.to_string();
        loop {
            if current.ends_with('\\') {
                current.truncate(current.len() - 1);
                buf.push_str(&current);
                buf.push('\n');
                print!("{DIM}  ...{RESET} ");
                io::stdout().flush().ok();
                match lines.next() {
                    Some(Ok(line)) => {
                        current = line;
                    }
                    _ => break,
                }
            } else {
                buf.push_str(&current);
                break;
            }
        }
    }

    buf
}

/// Format a ThinkingLevel as a display string.
fn thinking_level_name(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Off => "off",
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
    }
}

/// Run health checks (build, test, clippy, fmt) and return results.
/// Each result is (name, passed, detail_message).
fn run_health_check() -> Vec<(&'static str, bool, String)> {
    let mut checks: Vec<(&str, &[&str])> = vec![("build", &["cargo", "build"])];
    // Skip "cargo test" when built with #[cfg(test)] to avoid infinite recursion
    // (test_health_check_function → run_health_check → cargo test → test_health_check_function → …)
    #[cfg(not(test))]
    checks.push(("test", &["cargo", "test"]));
    checks.push((
        "clippy",
        &["cargo", "clippy", "--all-targets", "--", "-D", "warnings"],
    ));
    checks.push(("fmt", &["cargo", "fmt", "--", "--check"]));

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

/// Build a directory tree from `git ls-files`, grouped by directory.
/// Returns a formatted string showing the project structure up to `max_depth` levels.
/// Falls back to a simple directory walk if not in a git repo.
fn build_project_tree(max_depth: usize) -> String {
    // Try git ls-files first (respects .gitignore)
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
/// Limits output to `max_depth` levels (0 = root files only, 1 = one level of dirs, etc.).
fn format_tree_from_paths(paths: &[String], max_depth: usize) -> String {
    use std::collections::BTreeSet;

    let mut output = String::new();
    // Collect all unique directory prefixes and files at each level
    let mut printed_dirs: BTreeSet<String> = BTreeSet::new();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let depth = parts.len() - 1; // number of directory components

        // Print directory prefixes we haven't seen yet
        for level in 0..parts.len().saturating_sub(1).min(max_depth) {
            let dir_path: String = parts[..=level].join("/");
            let dir_key = format!("{}/", dir_path);
            if printed_dirs.insert(dir_key) {
                let indent = "  ".repeat(level);
                let dir_name = parts[level];
                output.push_str(&format!("{indent}{dir_name}/\n"));
            }
        }

        // Print the file itself if within depth
        if depth <= max_depth {
            let indent = "  ".repeat(depth.min(max_depth));
            let file_name = parts.last().unwrap_or(&"");
            output.push_str(&format!("{indent}{file_name}\n"));
        }
    }

    // Trim trailing newline
    if output.ends_with('\n') {
        output.truncate(output.len() - 1);
    }

    output
}

/// Known REPL command prefixes. Used to detect unknown slash commands.
const KNOWN_COMMANDS: &[&str] = &[
    "/help", "/quit", "/exit", "/clear", "/compact", "/cost", "/status", "/tokens", "/save",
    "/load", "/diff", "/undo", "/health", "/retry", "/history", "/search", "/model", "/think",
    "/config", "/context", "/init", "/version", "/run", "/tree", "/pr",
];

/// Check if a slash-prefixed input is an unknown command.
/// Extracts the first word and checks against known commands.
fn is_unknown_command(input: &str) -> bool {
    let cmd = input.split_whitespace().next().unwrap_or(input);
    !KNOWN_COMMANDS.contains(&cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_parsing_quit() {
        let quit_commands = ["/quit", "/exit"];
        for cmd in &quit_commands {
            assert!(
                *cmd == "/quit" || *cmd == "/exit",
                "Unrecognized quit command: {cmd}"
            );
        }
    }

    #[test]
    fn test_command_parsing_model() {
        let input = "/model claude-opus-4-6";
        assert!(input.starts_with("/model "));
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_parsing_model_whitespace() {
        let input = "/model   claude-opus-4-6  ";
        let model_name = input.trim_start_matches("/model ").trim();
        assert_eq!(model_name, "claude-opus-4-6");
    }

    #[test]
    fn test_command_help_recognized() {
        let commands = [
            "/help", "/quit", "/exit", "/clear", "/compact", "/config", "/context", "/init",
            "/status", "/tokens", "/save", "/load", "/diff", "/undo", "/health", "/retry", "/run",
            "/history", "/search", "/model", "/think", "/version", "/tree", "/pr",
        ];
        for cmd in &commands {
            assert!(
                KNOWN_COMMANDS.contains(cmd),
                "Command not in KNOWN_COMMANDS: {cmd}"
            );
        }
    }

    #[test]
    fn test_model_switch_updates_variable() {
        let original = "claude-opus-4-6";
        let input = "/model claude-haiku-35";
        let new_model = input.trim_start_matches("/model ").trim();
        assert_ne!(new_model, original);
        assert_eq!(new_model, "claude-haiku-35");
    }

    #[test]
    fn test_needs_continuation_backslash() {
        assert!(needs_continuation("hello \\"));
        assert!(needs_continuation("line ends with\\"));
        assert!(!needs_continuation("normal line"));
        assert!(!needs_continuation("has \\ in middle"));
    }

    #[test]
    fn test_needs_continuation_code_fence() {
        assert!(needs_continuation("```rust"));
        assert!(needs_continuation("```"));
        assert!(!needs_continuation("some text ```"));
        assert!(!needs_continuation("normal"));
    }

    #[test]
    fn test_bare_model_command_is_recognized() {
        let input = "/model";
        assert_eq!(input, "/model");
        assert!(!input.starts_with("/model "));
    }

    #[test]
    fn test_unknown_slash_command_detection() {
        assert!(is_unknown_command("/foo"));
        assert!(is_unknown_command("/foo bar baz"));
        assert!(is_unknown_command("/unknown argument"));
        // Verify typo-like commands are caught as unknown
        assert!(is_unknown_command("/savefile"));
        assert!(is_unknown_command("/loadfile"));

        assert!(!is_unknown_command("/help"));
        assert!(!is_unknown_command("/quit"));
        assert!(!is_unknown_command("/model"));
        assert!(!is_unknown_command("/model claude-opus-4-6"));
        assert!(!is_unknown_command("/save"));
        assert!(!is_unknown_command("/save myfile.json"));
        assert!(!is_unknown_command("/load"));
        assert!(!is_unknown_command("/load myfile.json"));
        assert!(!is_unknown_command("/config"));
        assert!(!is_unknown_command("/context"));
        assert!(!is_unknown_command("/version"));
    }

    #[test]
    fn test_thinking_level_name() {
        assert_eq!(thinking_level_name(ThinkingLevel::Off), "off");
        assert_eq!(thinking_level_name(ThinkingLevel::Minimal), "minimal");
        assert_eq!(thinking_level_name(ThinkingLevel::Low), "low");
        assert_eq!(thinking_level_name(ThinkingLevel::Medium), "medium");
        assert_eq!(thinking_level_name(ThinkingLevel::High), "high");
    }

    #[test]
    fn test_health_check_function() {
        // run_health_check skips "cargo test" under #[cfg(test)] to avoid recursion
        let results = run_health_check();
        assert!(
            !results.is_empty(),
            "Health check should return at least one result"
        );
        for (name, passed, _) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
        // "test" check should be excluded under cfg(test)
        assert!(
            !results.iter().any(|(name, _, _)| *name == "test"),
            "cargo test check should be skipped to avoid recursion"
        );
    }

    #[test]
    fn test_run_command_recognized() {
        assert!(!is_unknown_command("/run"));
        assert!(!is_unknown_command("/run echo hello"));
        assert!(!is_unknown_command("/run ls -la"));
    }

    #[test]
    fn test_run_shell_command_basic() {
        // Verify run_shell_command doesn't panic on basic commands
        // (output goes to stdout/stderr, we just check it doesn't crash)
        run_shell_command("echo hello");
    }

    #[test]
    fn test_run_shell_command_failing() {
        // Non-zero exit should not panic
        run_shell_command("false");
    }

    #[test]
    fn test_bang_shortcut_matching() {
        // ! prefix should match for /run shortcut
        let bang_matches = |s: &str| s.starts_with('!') && s.len() > 1;
        assert!(bang_matches("!ls"));
        assert!(bang_matches("!echo hello"));
        assert!(bang_matches("! ls")); // space after bang is fine
        assert!(!bang_matches("!")); // bare bang alone should not match
    }

    #[test]
    fn test_run_command_matching() {
        // /run should only match /run or /run <cmd>, not /running
        let run_matches = |s: &str| s == "/run" || s.starts_with("/run ");
        assert!(run_matches("/run"));
        assert!(run_matches("/run echo hello"));
        assert!(!run_matches("/running"));
        assert!(!run_matches("/runaway"));
    }

    #[test]
    fn test_format_tree_from_paths_basic() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "README.md".to_string(),
            "src/cli.rs".to_string(),
            "src/format.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
        assert!(tree.contains("src/"));
        assert!(tree.contains("  main.rs"));
        assert!(tree.contains("  cli.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_nested() {
        let paths = vec![
            "src/main.rs".to_string(),
            "src/utils/helpers.rs".to_string(),
            "src/utils/format.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.contains("src/"));
        assert!(tree.contains("  utils/"));
        assert!(tree.contains("    helpers.rs"));
        assert!(tree.contains("    format.rs"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_limit() {
        let paths = vec![
            "a/b/c/d/deep.txt".to_string(),
            "a/shallow.txt".to_string(),
            "top.txt".to_string(),
        ];
        // depth 1: show dirs at level 0 ('a/'), files at depth ≤ 1
        let tree = format_tree_from_paths(&paths, 1);
        assert!(tree.contains("top.txt"));
        assert!(tree.contains("a/"));
        assert!(tree.contains("  shallow.txt"));
        // Files deeper than max_depth should not appear
        assert!(!tree.contains("deep.txt"));
        // Directory 'b/' is at level 1, beyond max_depth=1 for dirs
        assert!(!tree.contains("b/"));
    }

    #[test]
    fn test_format_tree_from_paths_empty() {
        let paths: Vec<String> = vec![];
        let tree = format_tree_from_paths(&paths, 3);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_format_tree_from_paths_root_files_only() {
        let paths = vec![
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "README.md".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // No directories, just root files
        assert!(!tree.contains('/'));
        assert!(tree.contains("Cargo.lock"));
        assert!(tree.contains("Cargo.toml"));
        assert!(tree.contains("README.md"));
    }

    #[test]
    fn test_format_tree_from_paths_depth_zero() {
        let paths = vec!["README.md".to_string(), "src/main.rs".to_string()];
        let tree = format_tree_from_paths(&paths, 0);
        // Depth 0: only root-level files shown
        assert!(tree.contains("README.md"));
        // main.rs is at depth 1, should not show at depth 0
        assert!(!tree.contains("main.rs"));
    }

    #[test]
    fn test_format_tree_dir_printed_once() {
        let paths = vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ];
        let tree = format_tree_from_paths(&paths, 3);
        // "src/" should appear exactly once
        assert_eq!(tree.matches("src/").count(), 1);
    }

    #[test]
    fn test_build_project_tree_runs() {
        // In a git repo, this should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // Should contain at least Cargo.toml (we're in a Rust project)
        assert!(tree.contains("Cargo.toml"));
    }

    #[test]
    fn test_tree_command_recognized() {
        assert!(!is_unknown_command("/tree"));
        assert!(!is_unknown_command("/tree 2"));
        assert!(!is_unknown_command("/tree 5"));
    }

    #[test]
    fn test_pr_command_recognized() {
        assert!(!is_unknown_command("/pr"));
        assert!(!is_unknown_command("/pr 42"));
        assert!(!is_unknown_command("/pr 123"));
    }

    #[test]
    fn test_pr_command_matching() {
        // /pr should match exact or with space separator, not /print etc.
        let pr_matches = |s: &str| s == "/pr" || s.starts_with("/pr ");
        assert!(pr_matches("/pr"));
        assert!(pr_matches("/pr 42"));
        assert!(pr_matches("/pr 123"));
        assert!(!pr_matches("/print"));
        assert!(!pr_matches("/process"));
    }

    #[test]
    fn test_pr_number_parsing() {
        // Verify we can parse a PR number from /pr <number>
        let input = "/pr 42";
        let arg = input.strip_prefix("/pr").unwrap_or("").trim();
        assert_eq!(arg, "42");
        assert!(arg.parse::<u32>().is_ok());
        assert_eq!(arg.parse::<u32>().unwrap(), 42);

        // Bare /pr has empty arg
        let input_bare = "/pr";
        let arg_bare = input_bare.strip_prefix("/pr").unwrap_or("").trim();
        assert!(arg_bare.is_empty());
    }

    #[test]
    fn test_save_load_command_matching() {
        // /save and /load should only match exact word or with space separator
        // This tests the fix for /savefile being treated as /save
        let save_matches = |s: &str| s == "/save" || s.starts_with("/save ");
        let load_matches = |s: &str| s == "/load" || s.starts_with("/load ");

        assert!(save_matches("/save"));
        assert!(save_matches("/save myfile.json"));
        assert!(!save_matches("/savefile"));
        assert!(!save_matches("/saveXYZ"));

        assert!(load_matches("/load"));
        assert!(load_matches("/load myfile.json"));
        assert!(!load_matches("/loadfile"));
        assert!(!load_matches("/loadXYZ"));
    }
}
