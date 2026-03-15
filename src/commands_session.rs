//! Session-related command handlers: /save, /load, /compact, /history, /search,
//! /mark, /jump, /marks, /spawn.

use crate::format::*;
use crate::prompt::*;

use std::collections::HashMap;
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::*;

use crate::cli::{AUTO_COMPACT_THRESHOLD, DEFAULT_SESSION_PATH, MAX_CONTEXT_TOKENS};

// ── compact ──────────────────────────────────────────────────────────────

/// Compact the agent's conversation and return (before_count, before_tokens, after_count, after_tokens).
/// Returns None if nothing changed.
pub fn compact_agent(agent: &mut Agent) -> Option<(usize, u64, usize, u64)> {
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
pub fn auto_compact_if_needed(agent: &mut Agent) {
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

pub fn handle_compact(agent: &mut Agent) {
    let messages = agent.messages();
    let before_count = messages.len();
    let before_tokens = total_tokens(messages) as u64;
    match compact_agent(agent) {
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
}

// ── /save ────────────────────────────────────────────────────────────────

pub fn handle_save(agent: &Agent, input: &str) {
    let path = input.strip_prefix("/save").unwrap_or("").trim();
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
}

// ── /load ────────────────────────────────────────────────────────────────

pub fn handle_load(agent: &mut Agent, input: &str) {
    let path = input.strip_prefix("/load").unwrap_or("").trim();
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
}

// ── /history ─────────────────────────────────────────────────────────────

pub fn handle_history(agent: &Agent) {
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
}

// ── /search ──────────────────────────────────────────────────────────────

pub fn handle_search(agent: &Agent, input: &str) {
    if input == "/search" {
        println!("{DIM}  usage: /search <query>");
        println!("  Search conversation history for messages containing <query>.{RESET}\n");
        return;
    }
    let query = input.trim_start_matches("/search ").trim();
    if query.is_empty() {
        println!("{DIM}  usage: /search <query>{RESET}\n");
        return;
    }
    let messages = agent.messages();
    if messages.is_empty() {
        println!("{DIM}  (no messages to search){RESET}\n");
        return;
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
}

// ── /mark, /jump, /marks (bookmarks) ─────────────────────────────────────

/// Storage for conversation bookmarks: named snapshots of the message list.
pub type Bookmarks = HashMap<String, String>;

/// Parse the bookmark name from `/mark <name>` input.
/// Returns None if no name is provided.
pub fn parse_bookmark_name(input: &str, prefix: &str) -> Option<String> {
    let name = input.strip_prefix(prefix).unwrap_or("").trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Handle `/mark <name>`: save the current conversation state as a named bookmark.
pub fn handle_mark(agent: &Agent, input: &str, bookmarks: &mut Bookmarks) {
    let name = match parse_bookmark_name(input, "/mark") {
        Some(n) => n,
        None => {
            println!("{DIM}  usage: /mark <name>");
            println!("  Save a bookmark at the current point in the conversation.");
            println!("  Use /jump <name> to return to this point later.{RESET}\n");
            return;
        }
    };

    match agent.save_messages() {
        Ok(json) => {
            let msg_count = agent.messages().len();
            let overwriting = bookmarks.contains_key(&name);
            bookmarks.insert(name.clone(), json);
            if overwriting {
                println!("{GREEN}  ✓ bookmark '{name}' updated ({msg_count} messages){RESET}\n");
            } else {
                println!("{GREEN}  ✓ bookmark '{name}' saved ({msg_count} messages){RESET}\n");
            }
        }
        Err(e) => eprintln!("{RED}  error saving bookmark: {e}{RESET}\n"),
    }
}

/// Handle `/jump <name>`: restore conversation to a previously saved bookmark.
pub fn handle_jump(agent: &mut Agent, input: &str, bookmarks: &Bookmarks) {
    let name = match parse_bookmark_name(input, "/jump") {
        Some(n) => n,
        None => {
            println!("{DIM}  usage: /jump <name>");
            println!("  Restore the conversation to a previously saved bookmark.");
            println!("  Messages added after the bookmark will be discarded.{RESET}\n");
            return;
        }
    };

    match bookmarks.get(&name) {
        Some(json) => match agent.restore_messages(json) {
            Ok(_) => {
                let msg_count = agent.messages().len();
                println!("{GREEN}  ✓ jumped to bookmark '{name}' ({msg_count} messages){RESET}\n");
            }
            Err(e) => eprintln!("{RED}  error restoring bookmark: {e}{RESET}\n"),
        },
        None => {
            let available: Vec<&str> = bookmarks.keys().map(|k| k.as_str()).collect();
            if available.is_empty() {
                eprintln!("{RED}  bookmark '{name}' not found — no bookmarks saved yet.");
                eprintln!("  Use /mark <name> to save one.{RESET}\n");
            } else {
                eprintln!("{RED}  bookmark '{name}' not found.");
                eprintln!("{DIM}  available: {}{RESET}\n", available.join(", "));
            }
        }
    }
}

/// Handle `/marks`: list all saved bookmarks.
pub fn handle_marks(bookmarks: &Bookmarks) {
    if bookmarks.is_empty() {
        println!("{DIM}  (no bookmarks saved)");
        println!("  Use /mark <name> to save a bookmark.{RESET}\n");
    } else {
        println!("{DIM}  Saved bookmarks:");
        let mut names: Vec<&String> = bookmarks.keys().collect();
        names.sort();
        for name in names {
            println!("    • {name}");
        }
        println!("{RESET}");
    }
}

// ── /spawn ────────────────────────────────────────────────────────────────

/// Parse the task from a `/spawn <task>` input.
/// Returns None if no task is provided.
pub fn parse_spawn_task(input: &str) -> Option<String> {
    let task = input
        .strip_prefix("/spawn")
        .unwrap_or("")
        .trim()
        .to_string();
    if task.is_empty() {
        None
    } else {
        Some(task)
    }
}

/// Handle the /spawn command: create a fresh subagent, run a task, and return the result.
/// The subagent gets its own independent context window so complex tasks don't pollute
/// the main conversation.
/// Returns Some(context_msg) to be injected back into the main conversation, or None.
pub async fn handle_spawn(
    input: &str,
    agent_config: &crate::AgentConfig,
    session_total: &mut Usage,
    model: &str,
) -> Option<String> {
    let task = match parse_spawn_task(input) {
        Some(t) => t,
        None => {
            println!("{DIM}  usage: /spawn <task>");
            println!("  Spawn a subagent with a fresh context to handle a task.");
            println!("  The result is summarized back into your main conversation.");
            println!("  Example: /spawn read src/main.rs and summarize the architecture{RESET}\n");
            return None;
        }
    };

    println!("{CYAN}  🐙 spawning subagent...{RESET}");
    println!(
        "{DIM}  task: {}{RESET}",
        crate::format::truncate_with_ellipsis(&task, 100)
    );

    // Build a fresh agent with the same config but independent context
    let mut sub_agent = agent_config.build_agent();

    // Run the task as a single prompt on the subagent
    let response = run_prompt(&mut sub_agent, &task, session_total, model).await;

    println!("\n{GREEN}  ✓ subagent completed{RESET}");
    println!("{DIM}  injecting result into main conversation...{RESET}\n");

    // Build a context message for the main agent summarizing what the subagent did
    let result_text = if response.trim().is_empty() {
        "(no output)".to_string()
    } else {
        response.trim().to_string()
    };

    let context_msg = format!(
        "A subagent just completed a task. Here is its result:\n\n**Task:** {task}\n\n**Result:**\n{result_text}"
    );

    Some(context_msg)
}
