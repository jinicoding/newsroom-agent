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
//!   /commit [msg]   Commit staged changes (AI-generates message if no msg)
//!   /docs <crate>   Look up docs.rs documentation for a Rust crate
//!   /docs <c> <i>   Look up a specific item within a crate
//!   /fix            Auto-fix build/lint errors (runs checks, sends failures to AI)
//!   /git <subcmd>   Quick git: status, log, add, diff, branch, stash
//!   /model <name>   Switch model mid-session
//!   /search <query> Search conversation history
//!   /tree [depth]   Show project directory tree
//!   /test           Auto-detect and run project tests
//!   /pr [number]    List open PRs, view/diff/comment/checkout a PR
//!   /retry          Re-send the last user input

mod cli;
mod commands;
mod docs;
mod format;
mod git;
mod prompt;

use cli::*;
use commands::{auto_compact_if_needed, is_unknown_command, thinking_level_name, KNOWN_COMMANDS};
use format::*;
use git::*;
use prompt::*;

use rustyline::completion::Completer;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::Editor;
use std::io::{self, IsTerminal, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use yoagent::agent::Agent;
use yoagent::context::ExecutionLimits;
use yoagent::openapi::{OpenApiConfig, OperationFilter};
use yoagent::provider::{
    AnthropicProvider, GoogleProvider, ModelConfig, OpenAiCompat, OpenAiCompatProvider,
};
use yoagent::tools::bash::BashTool;
use yoagent::tools::edit::EditFileTool;
use yoagent::tools::file::{ReadFileTool, WriteFileTool};
use yoagent::tools::list::ListFilesTool;
use yoagent::tools::search::SearchTool;
use yoagent::types::AgentTool;
use yoagent::*;

/// Rustyline helper that provides tab-completion for `/` slash commands.
struct YoyoHelper;

impl Completer for YoyoHelper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let prefix = &line[..pos];

        // Slash command completion: starts with '/' and no space yet
        if prefix.starts_with('/') && !prefix.contains(' ') {
            let matches: Vec<String> = KNOWN_COMMANDS
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|cmd| cmd.to_string())
                .collect();
            return Ok((0, matches));
        }

        // File path completion: extract the last whitespace-delimited word
        let word_start = prefix.rfind(char::is_whitespace).map_or(0, |i| i + 1);
        let word = &prefix[word_start..];
        if word.is_empty() {
            return Ok((pos, Vec::new()));
        }

        let matches = complete_file_path(word);
        Ok((word_start, matches))
    }
}

/// Complete a partial file path by listing directory entries that match.
/// Appends `/` to directory names for easy continued completion.
fn complete_file_path(partial: &str) -> Vec<String> {
    use std::path::Path;

    let path = Path::new(partial);

    // Determine the directory to scan and the filename prefix to match
    let (dir, file_prefix) =
        if partial.ends_with('/') || partial.ends_with(std::path::MAIN_SEPARATOR) {
            // User typed "src/" — list everything inside src/
            (partial.to_string(), String::new())
        } else if let Some(parent) = path.parent() {
            let parent_str = if parent.as_os_str().is_empty() {
                ".".to_string()
            } else {
                parent.to_string_lossy().to_string()
            };
            let file_prefix = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            (parent_str, file_prefix)
        } else {
            (".".to_string(), partial.to_string())
        };

    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let dir_prefix = if dir == "." && !partial.contains('/') {
        String::new()
    } else if partial.ends_with('/') || partial.ends_with(std::path::MAIN_SEPARATOR) {
        partial.to_string()
    } else {
        let parent = path.parent().unwrap_or(Path::new(""));
        if parent.as_os_str().is_empty() {
            String::new()
        } else {
            format!("{}/", parent.display())
        }
    };

    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(&file_prefix) {
            continue;
        }
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let candidate = if is_dir {
            format!("{}{}/", dir_prefix, name)
        } else {
            format!("{}{}", dir_prefix, name)
        };
        matches.push(candidate);
    }
    matches.sort();
    matches
}

impl Hinter for YoyoHelper {
    type Hint = String;
}

impl Highlighter for YoyoHelper {}

impl Validator for YoyoHelper {}

impl rustyline::Helper for YoyoHelper {}

/// Build the tool set, optionally with a bash confirmation prompt.
/// When `auto_approve` is false (default), bash commands require user approval.
/// The "always" option sets a session-wide flag so subsequent commands are auto-approved.
/// When `permissions` has patterns, matching commands are auto-approved or auto-denied.
fn build_tools(auto_approve: bool, permissions: &cli::PermissionConfig) -> Vec<Box<dyn AgentTool>> {
    let bash = if auto_approve {
        BashTool::default()
    } else {
        let always_approved = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&always_approved);
        let perms = permissions.clone();
        BashTool::default().with_confirm(move |cmd: &str| {
            // If user previously chose "always", skip the prompt
            if flag.load(Ordering::Relaxed) {
                eprintln!(
                    "{GREEN}  ✓ Auto-approved: {RESET}{}",
                    truncate_with_ellipsis(cmd, 120)
                );
                return true;
            }
            // Check permission patterns before prompting
            if let Some(allowed) = perms.check(cmd) {
                if allowed {
                    eprintln!(
                        "{GREEN}  ✓ Permitted: {RESET}{}",
                        truncate_with_ellipsis(cmd, 120)
                    );
                    return true;
                } else {
                    eprintln!(
                        "{RED}  ✗ Denied by permission rule: {RESET}{}",
                        truncate_with_ellipsis(cmd, 120)
                    );
                    return false;
                }
            }
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
            let approved = matches!(response.as_str(), "y" | "yes" | "a" | "always");
            if matches!(response.as_str(), "a" | "always") {
                flag.store(true, Ordering::Relaxed);
                eprintln!(
                    "{GREEN}  ✓ All subsequent commands will be auto-approved this session.{RESET}"
                );
            }
            approved
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

/// Create a ModelConfig for non-Anthropic providers.
fn create_model_config(provider: &str, model: &str, base_url: Option<&str>) -> ModelConfig {
    match provider {
        "openai" => {
            let mut config = ModelConfig::openai(model, model);
            if let Some(url) = base_url {
                config.base_url = url.to_string();
            }
            config
        }
        "google" => {
            let mut config = ModelConfig::google(model, model);
            if let Some(url) = base_url {
                config.base_url = url.to_string();
            }
            config
        }
        "ollama" => {
            let url = base_url.unwrap_or("http://localhost:11434/v1");
            ModelConfig::local(url, model)
        }
        "openrouter" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "openrouter".into();
            config.base_url = base_url
                .unwrap_or("https://openrouter.ai/api/v1")
                .to_string();
            config.compat = Some(OpenAiCompat::openrouter());
            config
        }
        "xai" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "xai".into();
            config.base_url = base_url.unwrap_or("https://api.x.ai/v1").to_string();
            config.compat = Some(OpenAiCompat::xai());
            config
        }
        "groq" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "groq".into();
            config.base_url = base_url
                .unwrap_or("https://api.groq.com/openai/v1")
                .to_string();
            config.compat = Some(OpenAiCompat::groq());
            config
        }
        "deepseek" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "deepseek".into();
            config.base_url = base_url
                .unwrap_or("https://api.deepseek.com/v1")
                .to_string();
            config.compat = Some(OpenAiCompat::deepseek());
            config
        }
        "mistral" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "mistral".into();
            config.base_url = base_url.unwrap_or("https://api.mistral.ai/v1").to_string();
            config.compat = Some(OpenAiCompat::mistral());
            config
        }
        "cerebras" => {
            let mut config = ModelConfig::openai(model, model);
            config.provider = "cerebras".into();
            config.base_url = base_url.unwrap_or("https://api.cerebras.ai/v1").to_string();
            config.compat = Some(OpenAiCompat::cerebras());
            config
        }
        "custom" => {
            let url = base_url.unwrap_or("http://localhost:8080/v1");
            ModelConfig::local(url, model)
        }
        _ => {
            // Unknown provider — treat as OpenAI-compatible with custom base URL
            let url = base_url.unwrap_or("http://localhost:8080/v1");
            let mut config = ModelConfig::local(url, model);
            config.provider = provider.to_string();
            config
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_agent(
    model: &str,
    api_key: &str,
    provider: &str,
    base_url: Option<&str>,
    skills: &yoagent::skills::SkillSet,
    system_prompt: &str,
    thinking: ThinkingLevel,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    max_turns: Option<usize>,
    auto_approve: bool,
    permissions: &cli::PermissionConfig,
) -> Agent {
    let mut agent = if provider == "anthropic" && base_url.is_none() {
        // Default Anthropic path — unchanged
        Agent::new(AnthropicProvider)
            .with_system_prompt(system_prompt)
            .with_model(model)
            .with_api_key(api_key)
            .with_thinking(thinking)
            .with_skills(skills.clone())
            .with_tools(build_tools(auto_approve, permissions))
    } else if provider == "google" {
        // Google uses its own provider
        let config = create_model_config(provider, model, base_url);
        Agent::new(GoogleProvider)
            .with_system_prompt(system_prompt)
            .with_model(model)
            .with_api_key(api_key)
            .with_thinking(thinking)
            .with_skills(skills.clone())
            .with_tools(build_tools(auto_approve, permissions))
            .with_model_config(config)
    } else {
        // All other providers use OpenAI-compatible API
        let config = create_model_config(provider, model, base_url);
        Agent::new(OpenAiCompatProvider)
            .with_system_prompt(system_prompt)
            .with_model(model)
            .with_api_key(api_key)
            .with_thinking(thinking)
            .with_skills(skills.clone())
            .with_tools(build_tools(auto_approve, permissions))
            .with_model_config(config)
    };

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
    let provider = config.provider;
    let base_url = config.base_url;
    let skills = config.skills;
    let system_prompt = config.system_prompt;
    let mut thinking = config.thinking;
    let max_tokens = config.max_tokens;
    let temperature = config.temperature;
    let max_turns = config.max_turns;
    let continue_session = config.continue_session;
    let output_path = config.output_path;
    let mcp_servers = config.mcp_servers;
    let openapi_specs = config.openapi_specs;
    // Auto-approve in non-interactive modes (piped, --prompt) or when --yes is set
    let is_interactive = io::stdin().is_terminal() && config.prompt_arg.is_none();
    let auto_approve = config.auto_approve || !is_interactive;
    let permissions = config.permissions;

    let mut agent = build_agent(
        &model,
        &api_key,
        &provider,
        base_url.as_deref(),
        &skills,
        &system_prompt,
        thinking,
        max_tokens,
        temperature,
        max_turns,
        auto_approve,
        &permissions,
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
                    &provider,
                    base_url.as_deref(),
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                    &permissions,
                );
                eprintln!("{DIM}  mcp: agent rebuilt (previous MCP connections lost){RESET}");
            }
        }
    }

    // Load OpenAPI specs (--openapi flags)
    let mut openapi_count = 0u32;
    for spec_path in &openapi_specs {
        eprintln!("{DIM}  openapi: loading {spec_path}...{RESET}");
        let result = agent
            .with_openapi_file(spec_path, OpenApiConfig::default(), &OperationFilter::All)
            .await;
        match result {
            Ok(updated) => {
                agent = updated;
                openapi_count += 1;
                eprintln!("{GREEN}  ✓ openapi: {spec_path} loaded{RESET}");
            }
            Err(e) => {
                eprintln!("{RED}  ✗ openapi: failed to load '{spec_path}': {e}{RESET}");
                // Agent was consumed on error — rebuild it
                agent = build_agent(
                    &model,
                    &api_key,
                    &provider,
                    base_url.as_deref(),
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                    &permissions,
                );
                eprintln!("{DIM}  openapi: agent rebuilt (previous connections lost){RESET}");
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
        if provider != "anthropic" {
            eprintln!("{DIM}  yoyo (prompt mode) — provider: {provider}, model: {model}{RESET}");
        } else {
            eprintln!("{DIM}  yoyo (prompt mode) — model: {model}{RESET}");
        }
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
    if provider != "anthropic" {
        println!("{DIM}  provider: {provider}{RESET}");
    }
    println!("{DIM}  model: {model}{RESET}");
    if let Some(ref url) = base_url {
        println!("{DIM}  base_url: {url}{RESET}");
    }
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
    if openapi_count > 0 {
        println!("{DIM}  openapi: {openapi_count} spec(s) loaded{RESET}");
    }
    if is_verbose() {
        println!("{DIM}  verbose: on{RESET}");
    }
    if !auto_approve {
        println!("{DIM}  tools: confirmation required (use --yes to skip){RESET}");
    }
    if !permissions.is_empty() {
        println!(
            "{DIM}  permissions: {} allow, {} deny pattern(s){RESET}",
            permissions.allow.len(),
            permissions.deny.len()
        );
    }
    if let Some(branch) = git_branch() {
        println!("{DIM}  git:   {branch}{RESET}");
    }
    println!("{DIM}  cwd:   {cwd}{RESET}\n");

    // Set up rustyline editor with slash-command tab-completion
    let mut rl = Editor::new().expect("Failed to initialize readline");
    rl.set_helper(Some(YoyoHelper));
    if let Some(history_path) = history_file_path() {
        if rl.load_history(&history_path).is_err() {
            // First run or history file doesn't exist yet — that's fine
        }
    }

    let mut session_total = Usage::default();
    let mut last_input: Option<String> = None;

    loop {
        let prompt = if let Some(branch) = git_branch() {
            format!("{BOLD}{GREEN}{branch}{RESET} {BOLD}{GREEN}> {RESET}")
        } else {
            format!("{BOLD}{GREEN}> {RESET}")
        };

        let line = match rl.readline(&prompt) {
            Ok(l) => l,
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: cancel current line, print new prompt
                println!();
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                break;
            }
            Err(_) => break,
        };

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Add to readline history
        let _ = rl.add_history_entry(&line);

        // Multi-line input: collect continuation lines
        let input = if needs_continuation(input) {
            collect_multiline_rl(input, &mut rl)
        } else {
            input.to_string()
        };
        let input = input.trim();

        match input {
            "/quit" | "/exit" => break,
            "/help" => {
                commands::handle_help();
                continue;
            }
            "/version" => {
                commands::handle_version();
                continue;
            }
            "/status" => {
                commands::handle_status(&model, &cwd, &session_total);
                continue;
            }
            "/tokens" => {
                commands::handle_tokens(&agent, &session_total, &model);
                continue;
            }
            "/cost" => {
                commands::handle_cost(&session_total, &model);
                continue;
            }
            "/clear" => {
                agent = build_agent(
                    &model,
                    &api_key,
                    &provider,
                    base_url.as_deref(),
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                    &permissions,
                );
                println!("{DIM}  (conversation cleared){RESET}\n");
                continue;
            }
            "/model" => {
                commands::handle_model_show(&model);
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
                    &provider,
                    base_url.as_deref(),
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                    &permissions,
                );
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                println!("{DIM}  (switched to {new_model}, conversation preserved){RESET}\n");
                continue;
            }
            "/think" => {
                commands::handle_think_show(thinking);
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
                    &provider,
                    base_url.as_deref(),
                    &skills,
                    &system_prompt,
                    thinking,
                    max_tokens,
                    temperature,
                    max_turns,
                    auto_approve,
                    &permissions,
                );
                if let Some(json) = saved {
                    let _ = agent.restore_messages(&json);
                }
                let level_name = thinking_level_name(thinking);
                println!("{DIM}  (thinking set to {level_name}, conversation preserved){RESET}\n");
                continue;
            }
            s if s == "/save" || s.starts_with("/save ") => {
                commands::handle_save(&agent, input);
                continue;
            }
            s if s == "/load" || s.starts_with("/load ") => {
                commands::handle_load(&mut agent, input);
                continue;
            }
            "/diff" => {
                commands::handle_diff();
                continue;
            }
            "/undo" => {
                commands::handle_undo();
                continue;
            }
            "/health" => {
                commands::handle_health();
                continue;
            }
            "/test" => {
                commands::handle_test();
                continue;
            }
            "/fix" => {
                if let Some(fix_prompt) =
                    commands::handle_fix(&mut agent, &mut session_total, &model).await
                {
                    last_input = Some(fix_prompt);
                }
                continue;
            }
            "/history" => {
                commands::handle_history(&agent);
                continue;
            }
            "/search" => {
                commands::handle_search(&agent, input);
                continue;
            }
            s if s.starts_with("/search ") => {
                commands::handle_search(&agent, input);
                continue;
            }
            "/config" => {
                commands::handle_config(
                    &provider,
                    &model,
                    &base_url,
                    thinking,
                    max_tokens,
                    max_turns,
                    temperature,
                    &skills,
                    &system_prompt,
                    mcp_count,
                    openapi_count,
                    &agent,
                    continue_session,
                    &cwd,
                );
                continue;
            }
            "/compact" => {
                commands::handle_compact(&mut agent);
                continue;
            }
            s if s == "/commit" || s.starts_with("/commit ") => {
                commands::handle_commit(input);
                continue;
            }
            "/context" => {
                commands::handle_context();
                continue;
            }
            "/docs" => {
                commands::handle_docs(input);
                continue;
            }
            s if s.starts_with("/docs ") => {
                commands::handle_docs(input);
                continue;
            }
            "/init" => {
                commands::handle_init();
                continue;
            }
            "/retry" => {
                commands::handle_retry(&mut agent, &last_input, &mut session_total, &model).await;
                continue;
            }
            s if s == "/tree" || s.starts_with("/tree ") => {
                commands::handle_tree(input);
                continue;
            }
            s if s.starts_with("/run ") || (s.starts_with('!') && s.len() > 1) => {
                commands::handle_run(input);
                continue;
            }
            "/run" => {
                commands::handle_run_usage();
                continue;
            }
            s if s == "/pr" || s.starts_with("/pr ") => {
                commands::handle_pr(input);
                continue;
            }
            s if s == "/git" || s.starts_with("/git ") => {
                commands::handle_git(input);
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

    // Save readline history
    if let Some(history_path) = history_file_path() {
        let _ = rl.save_history(&history_path);
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

/// Check if a line needs continuation (backslash at end, or opens a code fence).
fn needs_continuation(line: &str) -> bool {
    line.ends_with('\\') || line.starts_with("```")
}

/// Collect multi-line input using rustyline (for interactive REPL mode).
/// Same logic as `collect_multiline` but uses rustyline's readline for continuation prompts.
fn collect_multiline_rl(first_line: &str, rl: &mut Editor<YoyoHelper, DefaultHistory>) -> String {
    let mut buf = String::new();
    let cont_prompt = format!("{DIM}  ...{RESET} ");

    if first_line.starts_with("```") {
        // Code fence mode: collect until closing ```
        buf.push_str(first_line);
        buf.push('\n');
        while let Ok(line) = rl.readline(&cont_prompt) {
            buf.push_str(&line);
            buf.push('\n');
            if line.trim() == "```" {
                break;
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
                match rl.readline(&cont_prompt) {
                    Ok(line) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use commands::{
        build_fix_prompt, build_project_tree, detect_project_type, format_tree_from_paths,
        health_checks_for_project, run_health_check_for_project, run_health_checks_full_output,
        test_command_for_project, ProjectType,
    };

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
            "/help", "/quit", "/exit", "/clear", "/compact", "/commit", "/config", "/context",
            "/cost", "/docs", "/fix", "/init", "/status", "/tokens", "/save", "/load", "/diff",
            "/undo", "/health", "/retry", "/run", "/history", "/search", "/model", "/think",
            "/version", "/tree", "/pr", "/git", "/test",
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
        // run_health_check_for_project skips "cargo test" under #[cfg(test)] to avoid recursion
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_check_for_project(&project_type);
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
    fn test_detect_project_type_rust() {
        // Current directory has Cargo.toml, so should detect Rust
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(detect_project_type(&cwd), ProjectType::Rust);
    }

    #[test]
    fn test_detect_project_type_node() {
        let tmp = std::env::temp_dir().join("yoyo_test_node");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("package.json"), "{}").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Node);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_pyproject() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_pyproject");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("pyproject.toml"), "[project]").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_python_setup_py() {
        let tmp = std::env::temp_dir().join("yoyo_test_python_setup");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("setup.py"), "").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Python);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_go() {
        let tmp = std::env::temp_dir().join("yoyo_test_go");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("go.mod"), "module example.com/test").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Go);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_makefile() {
        let tmp = std::env::temp_dir().join("yoyo_test_make");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Makefile"), "test:\n\techo ok").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Make);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_unknown() {
        let tmp = std::env::temp_dir().join("yoyo_test_unknown");
        let _ = std::fs::create_dir_all(&tmp);
        // Empty dir — no marker files
        assert_eq!(detect_project_type(&tmp), ProjectType::Unknown);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_detect_project_type_priority_rust_over_makefile() {
        // If both Cargo.toml and Makefile exist, Rust wins
        let tmp = std::env::temp_dir().join("yoyo_test_priority");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(tmp.join("Makefile"), "test:").unwrap();
        assert_eq!(detect_project_type(&tmp), ProjectType::Rust);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_health_checks_for_rust_project() {
        let checks = health_checks_for_project(&ProjectType::Rust);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Rust should have build check");
        assert!(names.contains(&"clippy"), "Rust should have clippy check");
        assert!(names.contains(&"fmt"), "Rust should have fmt check");
        // test is excluded under cfg(test)
        assert!(
            !names.contains(&"test"),
            "test should be excluded in cfg(test)"
        );
    }

    #[test]
    fn test_health_checks_for_node_project() {
        let checks = health_checks_for_project(&ProjectType::Node);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Node should have lint check");
    }

    #[test]
    fn test_health_checks_for_go_project() {
        let checks = health_checks_for_project(&ProjectType::Go);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"build"), "Go should have build check");
        assert!(names.contains(&"vet"), "Go should have vet check");
    }

    #[test]
    fn test_health_checks_for_python_project() {
        let checks = health_checks_for_project(&ProjectType::Python);
        let names: Vec<&str> = checks.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"lint"), "Python should have lint check");
        assert!(names.contains(&"typecheck"), "Python should have typecheck");
    }

    #[test]
    fn test_health_checks_for_unknown_returns_empty() {
        let checks = health_checks_for_project(&ProjectType::Unknown);
        assert!(checks.is_empty(), "Unknown project should return no checks");
    }

    #[test]
    fn test_project_type_display() {
        assert_eq!(format!("{}", ProjectType::Rust), "Rust (Cargo)");
        assert_eq!(format!("{}", ProjectType::Node), "Node.js (npm)");
        assert_eq!(format!("{}", ProjectType::Python), "Python");
        assert_eq!(format!("{}", ProjectType::Go), "Go");
        assert_eq!(format!("{}", ProjectType::Make), "Makefile");
        assert_eq!(format!("{}", ProjectType::Unknown), "Unknown");
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
        commands::run_shell_command("echo hello");
    }

    #[test]
    fn test_run_shell_command_failing() {
        // Non-zero exit should not panic
        commands::run_shell_command("false");
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
        // build_project_tree should return something non-empty
        let tree = build_project_tree(3);
        assert!(!tree.is_empty());
        // In a git repo, should contain Cargo.toml; outside one (e.g. cargo-mutants
        // temp dir) the tree still works but uses filesystem walk instead of git ls-files
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
    fn test_pr_subcommand_list() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args(""), PrSubcommand::List);
        assert_eq!(parse_pr_args("  "), PrSubcommand::List);
    }

    #[test]
    fn test_pr_subcommand_view() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42"), PrSubcommand::View(42));
        assert_eq!(parse_pr_args("123"), PrSubcommand::View(123));
        assert_eq!(parse_pr_args("1"), PrSubcommand::View(1));
    }

    #[test]
    fn test_pr_subcommand_diff() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42 diff"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("7 diff"), PrSubcommand::Diff(7));
    }

    #[test]
    fn test_pr_subcommand_checkout() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42 checkout"), PrSubcommand::Checkout(42));
        assert_eq!(parse_pr_args("99 checkout"), PrSubcommand::Checkout(99));
    }

    #[test]
    fn test_pr_subcommand_comment() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(
            parse_pr_args("42 comment looks good!"),
            PrSubcommand::Comment(42, "looks good!".to_string())
        );
        assert_eq!(
            parse_pr_args("10 comment LGTM, merging now"),
            PrSubcommand::Comment(10, "LGTM, merging now".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_comment_requires_text() {
        use commands::{parse_pr_args, PrSubcommand};
        // comment without text should show help
        assert_eq!(parse_pr_args("42 comment"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 comment  "), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_invalid() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("abc"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 unknown"), PrSubcommand::Help);
        assert_eq!(parse_pr_args("42 merge"), PrSubcommand::Help);
    }

    #[test]
    fn test_pr_subcommand_case_insensitive() {
        use commands::{parse_pr_args, PrSubcommand};
        assert_eq!(parse_pr_args("42 DIFF"), PrSubcommand::Diff(42));
        assert_eq!(parse_pr_args("42 Checkout"), PrSubcommand::Checkout(42));
        assert_eq!(
            parse_pr_args("42 Comment nice work"),
            PrSubcommand::Comment(42, "nice work".to_string())
        );
    }

    #[test]
    fn test_pr_subcommand_recognized() {
        // Subcommands should not be flagged as unknown commands
        assert!(!is_unknown_command("/pr 42 diff"));
        assert!(!is_unknown_command("/pr 42 comment hello"));
        assert!(!is_unknown_command("/pr 42 checkout"));
    }

    #[test]
    fn test_yoyo_helper_completes_slash_commands() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Typing "/" should suggest all commands
        let (start, candidates) = helper.complete("/", 1, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(!candidates.is_empty());
        assert!(candidates.contains(&"/help".to_string()));
        assert!(candidates.contains(&"/quit".to_string()));

        // Typing "/he" should suggest "/help" and "/health"
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.contains(&"/help".to_string()));
        assert!(candidates.contains(&"/health".to_string()));
        assert!(!candidates.contains(&"/quit".to_string()));

        // Typing "/model " (with space) should return no completions
        let (_, candidates) = helper.complete("/model claude", 13, &ctx).unwrap();
        assert!(candidates.is_empty());

        // Regular text that doesn't match any files returns no completions
        let (_, candidates) = helper.complete("zzz_nonexistent_xyz", 19, &ctx).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_file_path_completion_current_dir() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "Cargo" should match Cargo.toml (and possibly Cargo.lock)
        let (start, candidates) = helper.complete("Cargo", 5, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.iter().any(|c| c == "Cargo.toml"));
    }

    #[test]
    fn test_file_path_completion_with_directory_prefix() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "src/ma" should match "src/main.rs"
        let (start, candidates) = helper.complete("src/ma", 6, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_file_path_completion_no_completions_for_empty() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Empty input should return no completions
        let (_, candidates) = helper.complete("", 0, &ctx).unwrap();
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_file_path_completion_after_text() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "read the src/ma" should complete "src/ma" as the last word
        let input = "read the src/ma";
        let (start, candidates) = helper.complete(input, input.len(), &ctx).unwrap();
        assert_eq!(start, 9); // "read the " is 9 chars
        assert!(candidates.contains(&"src/main.rs".to_string()));
    }

    #[test]
    fn test_file_path_completion_directories_have_slash() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // "sr" should match "src/" (directory with trailing slash)
        let (start, candidates) = helper.complete("sr", 2, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.contains(&"src/".to_string()));
    }

    #[test]
    fn test_file_path_slash_commands_still_work() {
        use rustyline::history::DefaultHistory;
        let helper = YoyoHelper;
        let history = DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Slash commands should still complete normally
        let (start, candidates) = helper.complete("/he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.contains(&"/help".to_string()));
        assert!(candidates.contains(&"/health".to_string()));
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

    #[test]
    fn test_always_approve_flag_starts_false() {
        // The "always" flag should start as false
        let flag = Arc::new(AtomicBool::new(false));
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_always_approve_flag_persists_across_clones() {
        // Simulates the confirm closure: flag is shared via Arc
        let always_approved = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&always_approved);

        // Initially not set
        assert!(!flag_clone.load(Ordering::Relaxed));

        // User answers "always" — set the flag
        always_approved.store(true, Ordering::Relaxed);

        // The clone sees the update (simulates next confirm call)
        assert!(flag_clone.load(Ordering::Relaxed));
    }

    #[test]
    fn test_always_approve_response_matching() {
        // Verify the response matching logic for "always" variants
        let responses_that_approve = ["y", "yes", "a", "always"];
        let responses_that_deny = ["n", "no", "", "maybe", "nope"];

        for r in &responses_that_approve {
            let normalized = r.trim().to_lowercase();
            assert!(
                matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
                "Expected '{}' to be approved",
                r
            );
        }

        for r in &responses_that_deny {
            let normalized = r.trim().to_lowercase();
            assert!(
                !matches!(normalized.as_str(), "y" | "yes" | "a" | "always"),
                "Expected '{}' to be denied",
                r
            );
        }
    }

    #[test]
    fn test_always_approve_only_on_a_or_always() {
        // Only "a" and "always" should set the persist flag, not "y" or "yes"
        let always_responses = ["a", "always"];
        let single_responses = ["y", "yes"];

        for r in &always_responses {
            let normalized = r.trim().to_lowercase();
            assert!(
                matches!(normalized.as_str(), "a" | "always"),
                "Expected '{}' to trigger always-approve",
                r
            );
        }

        for r in &single_responses {
            let normalized = r.trim().to_lowercase();
            assert!(
                !matches!(normalized.as_str(), "a" | "always"),
                "Expected '{}' NOT to trigger always-approve",
                r
            );
        }
    }

    #[test]
    fn test_always_approve_flag_used_in_confirm_simulation() {
        // End-to-end simulation of the confirm flow with "always"
        let always_approved = Arc::new(AtomicBool::new(false));

        // Simulate three bash commands in sequence
        let commands = ["ls", "echo hello", "cat file.txt"];
        let user_responses = ["a", "", ""]; // user answers "always" first time

        for (i, cmd) in commands.iter().enumerate() {
            let approved = if always_approved.load(Ordering::Relaxed) {
                // Auto-approved — no prompt needed
                true
            } else {
                let response = user_responses[i].trim().to_lowercase();
                let result = matches!(response.as_str(), "y" | "yes" | "a" | "always");
                if matches!(response.as_str(), "a" | "always") {
                    always_approved.store(true, Ordering::Relaxed);
                }
                result
            };

            match i {
                0 => assert!(
                    approved,
                    "First command '{}' should be approved via 'a'",
                    cmd
                ),
                1 => assert!(approved, "Second command '{}' should be auto-approved", cmd),
                2 => assert!(approved, "Third command '{}' should be auto-approved", cmd),
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn test_build_tools_returns_six_tools() {
        // build_tools should return 6 tools regardless of auto_approve
        let perms = cli::PermissionConfig::default();
        let tools_approved = build_tools(true, &perms);
        let tools_confirm = build_tools(false, &perms);
        assert_eq!(tools_approved.len(), 6);
        assert_eq!(tools_confirm.len(), 6);
    }

    #[test]
    fn test_fix_command_recognized() {
        assert!(!is_unknown_command("/fix"));
        assert!(
            KNOWN_COMMANDS.contains(&"/fix"),
            "/fix should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_run_health_checks_full_output_returns_results() {
        // In a Rust project, should return results with full error output
        let project_type = detect_project_type(&std::env::current_dir().unwrap());
        assert_eq!(project_type, ProjectType::Rust);
        let results = run_health_checks_full_output(&project_type);
        assert!(
            !results.is_empty(),
            "Should return at least one check result"
        );
        for (name, passed, _output) in &results {
            assert!(!name.is_empty(), "Check name should not be empty");
            if *name == "build" {
                assert!(passed, "cargo build should pass in test environment");
            }
        }
    }

    #[test]
    fn test_build_fix_prompt_with_failures() {
        let failures = vec![
            (
                "build",
                "error[E0308]: mismatched types\n  --> src/main.rs:42",
            ),
            (
                "clippy",
                "warning: unused variable `x`\n  --> src/lib.rs:10",
            ),
        ];
        let prompt = build_fix_prompt(&failures);
        assert!(prompt.contains("build"), "Prompt should mention build");
        assert!(prompt.contains("clippy"), "Prompt should mention clippy");
        assert!(
            prompt.contains("error[E0308]"),
            "Prompt should include build error"
        );
        assert!(
            prompt.contains("unused variable"),
            "Prompt should include clippy warning"
        );
    }

    #[test]
    fn test_build_fix_prompt_empty_failures() {
        let failures: Vec<(&str, &str)> = vec![];
        let prompt = build_fix_prompt(&failures);
        assert!(
            prompt.is_empty() || prompt.contains("Fix"),
            "Empty failures should produce empty or minimal prompt"
        );
    }

    #[test]
    fn test_test_command_recognized() {
        assert!(!is_unknown_command("/test"));
        assert!(
            KNOWN_COMMANDS.contains(&"/test"),
            "/test should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_test_command_for_rust_project() {
        let cmd = test_command_for_project(&ProjectType::Rust);
        assert!(cmd.is_some(), "Rust project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("cargo"),
            "Rust test label should mention cargo"
        );
        assert_eq!(args[0], "cargo");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_node_project() {
        let cmd = test_command_for_project(&ProjectType::Node);
        assert!(cmd.is_some(), "Node project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("npm"), "Node test label should mention npm");
        assert_eq!(args[0], "npm");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_python_project() {
        let cmd = test_command_for_project(&ProjectType::Python);
        assert!(cmd.is_some(), "Python project should have a test command");
        let (label, _args) = cmd.unwrap();
        assert!(
            label.contains("pytest"),
            "Python test label should mention pytest"
        );
    }

    #[test]
    fn test_test_command_for_go_project() {
        let cmd = test_command_for_project(&ProjectType::Go);
        assert!(cmd.is_some(), "Go project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(label.contains("go"), "Go test label should mention go");
        assert_eq!(args[0], "go");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_make_project() {
        let cmd = test_command_for_project(&ProjectType::Make);
        assert!(cmd.is_some(), "Make project should have a test command");
        let (label, args) = cmd.unwrap();
        assert!(
            label.contains("make"),
            "Make test label should mention make"
        );
        assert_eq!(args[0], "make");
        assert!(args.contains(&"test"));
    }

    #[test]
    fn test_test_command_for_unknown_project() {
        let cmd = test_command_for_project(&ProjectType::Unknown);
        assert!(
            cmd.is_none(),
            "Unknown project should not have a test command"
        );
    }

    #[test]
    fn test_docs_command_recognized() {
        assert!(!is_unknown_command("/docs"));
        assert!(!is_unknown_command("/docs serde"));
        assert!(!is_unknown_command("/docs tokio"));
        assert!(
            KNOWN_COMMANDS.contains(&"/docs"),
            "/docs should be in KNOWN_COMMANDS"
        );
    }

    #[test]
    fn test_docs_command_matching() {
        // /docs should match exact or with space, not /docstring etc.
        let docs_matches = |s: &str| s == "/docs" || s.starts_with("/docs ");
        assert!(docs_matches("/docs"));
        assert!(docs_matches("/docs serde"));
        assert!(docs_matches("/docs tokio-runtime"));
        assert!(!docs_matches("/docstring"));
        assert!(!docs_matches("/docsify"));
    }

    #[test]
    fn test_docs_crate_arg_extraction() {
        let input = "/docs serde";
        let crate_name = input.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name, "serde");

        let input2 = "/docs tokio-runtime";
        let crate_name2 = input2.trim_start_matches("/docs ").trim();
        assert_eq!(crate_name2, "tokio-runtime");

        // Bare /docs has empty after stripping
        let input_bare = "/docs";
        assert_eq!(input_bare, "/docs");
        assert!(!input_bare.starts_with("/docs "));
    }
}
