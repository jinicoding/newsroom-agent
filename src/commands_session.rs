//! Session-related command handlers: /save, /load, /compact, /history, /search,
//! /mark, /jump, /marks, /spawn.

use crate::format::*;
use crate::prompt::*;

use std::collections::HashMap;
use yoagent::agent::Agent;
use yoagent::context::{compact_messages, total_tokens, ContextConfig};
use yoagent::*;

use crate::cli::{
    AUTO_COMPACT_THRESHOLD, AUTO_SAVE_SESSION_PATH, DEFAULT_SESSION_PATH, MAX_CONTEXT_TOKENS,
};

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

// ── auto-save ────────────────────────────────────────────────────────────

/// Check whether a previous auto-saved session exists at `.yoyo/last-session.json`.
pub fn last_session_exists() -> bool {
    std::path::Path::new(AUTO_SAVE_SESSION_PATH).exists()
}

/// Auto-save the current conversation to `.yoyo/last-session.json`.
/// Creates the `.yoyo/` directory if it doesn't exist.
/// Silently ignores errors (best-effort crash recovery).
pub fn auto_save_on_exit(agent: &Agent) {
    if agent.messages().is_empty() {
        return;
    }
    if let Ok(json) = agent.save_messages() {
        // Ensure .yoyo/ directory exists
        let _ = std::fs::create_dir_all(".yoyo");
        if std::fs::write(AUTO_SAVE_SESSION_PATH, &json).is_ok() {
            eprintln!(
                "{DIM}  session auto-saved to {AUTO_SAVE_SESSION_PATH} ({} messages){RESET}",
                agent.messages().len()
            );
        }
    }
}

/// Return the path to load for `--continue`: use `.yoyo/last-session.json` if it exists,
/// otherwise fall back to the legacy `yoyo-session.json`.
pub fn continue_session_path() -> &'static str {
    if last_session_exists() {
        AUTO_SAVE_SESSION_PATH
    } else {
        DEFAULT_SESSION_PATH
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::AUTO_SAVE_SESSION_PATH;

    #[test]
    fn test_auto_save_session_path_constant() {
        assert_eq!(AUTO_SAVE_SESSION_PATH, ".yoyo/last-session.json");
    }

    #[test]
    fn test_continue_session_path_fallback() {
        // When .yoyo/last-session.json doesn't exist, should fall back to yoyo-session.json
        // (In CI, .yoyo/last-session.json won't exist unless created by a prior test)
        let path = continue_session_path();
        // Should be one of the two valid paths
        assert!(
            path == AUTO_SAVE_SESSION_PATH || path == DEFAULT_SESSION_PATH,
            "continue_session_path should return a valid session path, got: {path}"
        );
    }

    #[test]
    fn test_last_session_exists_returns_bool() {
        // Should not panic regardless of whether the file exists
        let _exists = last_session_exists();
    }

    #[test]
    fn test_auto_save_creates_directory_and_file() {
        use yoagent::agent::Agent;
        use yoagent::provider::AnthropicProvider;

        // Use a temp directory to avoid polluting the project
        let tmp_dir = std::env::temp_dir().join("yoyo_test_autosave");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();

        // Change to temp directory
        std::env::set_current_dir(&tmp_dir).unwrap();

        // Create an agent with an empty conversation — should NOT save
        let agent = Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key");
        auto_save_on_exit(&agent);
        assert!(
            !std::path::Path::new(AUTO_SAVE_SESSION_PATH).exists(),
            "Should not save empty conversations"
        );

        // Restore directory
        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_continue_session_path_prefers_auto_save() {
        // Create a temp directory with .yoyo/last-session.json
        let tmp_dir = std::env::temp_dir().join("yoyo_test_continue_path");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(tmp_dir.join(".yoyo")).unwrap();
        std::fs::write(tmp_dir.join(".yoyo/last-session.json"), "[]").unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp_dir).unwrap();

        let path = continue_session_path();
        assert_eq!(
            path, AUTO_SAVE_SESSION_PATH,
            "Should prefer .yoyo/last-session.json when it exists"
        );

        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_continue_session_path_falls_back_to_default() {
        // Create a temp directory WITHOUT .yoyo/last-session.json
        let tmp_dir = std::env::temp_dir().join("yoyo_test_continue_fallback");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp_dir).unwrap();

        let path = continue_session_path();
        assert_eq!(
            path, DEFAULT_SESSION_PATH,
            "Should fall back to yoyo-session.json when .yoyo/last-session.json doesn't exist"
        );

        std::env::set_current_dir(&original_dir).unwrap();
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // ── Helper ──────────────────────────────────────────────────────────

    fn make_test_agent() -> Agent {
        use yoagent::provider::AnthropicProvider;
        Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key")
    }

    fn make_agent_with_messages(texts: &[&str]) -> Agent {
        use yoagent::provider::AnthropicProvider;
        let msgs: Vec<AgentMessage> = texts
            .iter()
            .map(|t| AgentMessage::Llm(yoagent::Message::user(*t)))
            .collect();
        Agent::new(AnthropicProvider)
            .with_system_prompt("test")
            .with_model("test-model")
            .with_api_key("test-key")
            .with_messages(msgs)
    }

    // ── parse_bookmark_name tests ───────────────────────────────────────

    #[test]
    fn test_parse_bookmark_name_with_name() {
        assert_eq!(
            parse_bookmark_name("/mark checkpoint1", "/mark"),
            Some("checkpoint1".to_string())
        );
    }

    #[test]
    fn test_parse_bookmark_name_empty() {
        assert_eq!(parse_bookmark_name("/mark", "/mark"), None);
        assert_eq!(parse_bookmark_name("/mark   ", "/mark"), None);
    }

    #[test]
    fn test_parse_bookmark_name_with_spaces() {
        assert_eq!(
            parse_bookmark_name("/mark  my bookmark ", "/mark"),
            Some("my bookmark".to_string())
        );
    }

    #[test]
    fn test_parse_bookmark_name_jump_prefix() {
        assert_eq!(
            parse_bookmark_name("/jump checkpoint1", "/jump"),
            Some("checkpoint1".to_string())
        );
    }

    // ── parse_spawn_task tests ──────────────────────────────────────────

    #[test]
    fn test_parse_spawn_task_with_task() {
        assert_eq!(
            parse_spawn_task("/spawn read src/main.rs"),
            Some("read src/main.rs".to_string())
        );
    }

    #[test]
    fn test_parse_spawn_task_empty() {
        assert_eq!(parse_spawn_task("/spawn"), None);
        assert_eq!(parse_spawn_task("/spawn   "), None);
    }

    #[test]
    fn test_parse_spawn_task_preserves_content() {
        let task = parse_spawn_task("/spawn summarize the architecture of this project");
        assert_eq!(
            task,
            Some("summarize the architecture of this project".to_string())
        );
    }

    // ── bookmark CRUD (handle_mark / handle_jump / handle_marks) ────────

    #[test]
    fn test_handle_mark_saves_bookmark() {
        let agent = make_agent_with_messages(&["hello", "world"]);
        let mut bookmarks = Bookmarks::new();
        handle_mark(&agent, "/mark test_bm", &mut bookmarks);
        assert!(bookmarks.contains_key("test_bm"));
    }

    #[test]
    fn test_handle_mark_overwrites_existing() {
        let agent1 = make_agent_with_messages(&["hello"]);
        let agent2 = make_agent_with_messages(&["hello", "world", "foo"]);
        let mut bookmarks = Bookmarks::new();
        handle_mark(&agent1, "/mark bm", &mut bookmarks);
        let snap1 = bookmarks.get("bm").unwrap().clone();
        handle_mark(&agent2, "/mark bm", &mut bookmarks);
        let snap2 = bookmarks.get("bm").unwrap().clone();
        assert_ne!(snap1, snap2, "Bookmark should be updated on overwrite");
    }

    #[test]
    fn test_handle_mark_no_name_does_not_crash() {
        let agent = make_test_agent();
        let mut bookmarks = Bookmarks::new();
        handle_mark(&agent, "/mark", &mut bookmarks);
        assert!(bookmarks.is_empty());
    }

    #[test]
    fn test_handle_jump_restores_bookmark() {
        let mut agent = make_agent_with_messages(&["hello", "world"]);
        let mut bookmarks = Bookmarks::new();
        handle_mark(&agent, "/mark snap", &mut bookmarks);
        let saved_len = agent.messages().len();

        // Add more messages
        agent.append_message(AgentMessage::Llm(yoagent::Message::user("extra")));
        assert_eq!(agent.messages().len(), saved_len + 1);

        // Jump back
        handle_jump(&mut agent, "/jump snap", &bookmarks);
        assert_eq!(agent.messages().len(), saved_len);
    }

    #[test]
    fn test_handle_jump_nonexistent_does_not_crash() {
        let mut agent = make_test_agent();
        let bookmarks = Bookmarks::new();
        handle_jump(&mut agent, "/jump nonexistent", &bookmarks);
        // Should not panic
    }

    #[test]
    fn test_handle_jump_no_name_does_not_crash() {
        let mut agent = make_test_agent();
        let bookmarks = Bookmarks::new();
        handle_jump(&mut agent, "/jump", &bookmarks);
    }

    #[test]
    fn test_handle_marks_empty() {
        let bookmarks = Bookmarks::new();
        handle_marks(&bookmarks); // Should not panic
    }

    #[test]
    fn test_handle_marks_lists_all() {
        let agent = make_agent_with_messages(&["hello"]);
        let mut bookmarks = Bookmarks::new();
        handle_mark(&agent, "/mark alpha", &mut bookmarks);
        handle_mark(&agent, "/mark beta", &mut bookmarks);
        assert_eq!(bookmarks.len(), 2);
        handle_marks(&bookmarks); // Should not panic, prints both
    }

    // ── compact_agent tests ─────────────────────────────────────────────

    #[test]
    fn test_compact_agent_empty_returns_none() {
        let mut agent = make_test_agent();
        assert!(compact_agent(&mut agent).is_none());
    }

    #[test]
    fn test_compact_agent_single_message_returns_none() {
        let mut agent = make_agent_with_messages(&["hello"]);
        // A single short message likely won't change after compaction
        let result = compact_agent(&mut agent);
        // With only 1 message, compaction typically does nothing
        assert!(result.is_none());
    }

    // ── auto_compact_if_needed tests ────────────────────────────────────

    #[test]
    fn test_auto_compact_below_threshold_does_nothing() {
        let mut agent = make_agent_with_messages(&["short message"]);
        let before_len = agent.messages().len();
        auto_compact_if_needed(&mut agent);
        // Below threshold, nothing should change
        assert_eq!(agent.messages().len(), before_len);
    }

    // ── handle_search tests ─────────────────────────────────────────────

    #[test]
    fn test_handle_search_no_query_does_not_crash() {
        let agent = make_test_agent();
        handle_search(&agent, "/search");
    }

    #[test]
    fn test_handle_search_empty_conversation() {
        let agent = make_test_agent();
        handle_search(&agent, "/search hello");
    }

    #[test]
    fn test_handle_search_finds_match() {
        let agent = make_agent_with_messages(&["the quick brown fox", "lazy dog"]);
        // This just tests it doesn't crash; output goes to stdout
        handle_search(&agent, "/search fox");
    }

    #[test]
    fn test_handle_search_no_match() {
        let agent = make_agent_with_messages(&["hello world"]);
        handle_search(&agent, "/search nonexistent");
    }

    // ── handle_save / handle_load path tests ────────────────────────────

    #[test]
    fn test_handle_save_custom_path() {
        let tmp_dir = std::env::temp_dir().join("yoyo_test_save_custom");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let save_path = tmp_dir.join("my-session.json");
        let save_path_str = save_path.to_str().unwrap();

        let agent = make_agent_with_messages(&["test message"]);
        let cmd = format!("/save {save_path_str}");
        handle_save(&agent, &cmd);
        assert!(save_path.exists(), "Should save to custom path");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_handle_load_roundtrip() {
        let tmp_dir = std::env::temp_dir().join("yoyo_test_load_roundtrip");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let save_path = tmp_dir.join("roundtrip.json");
        let save_path_str = save_path.to_str().unwrap();

        let agent = make_agent_with_messages(&["message one", "message two"]);
        let saved_len = agent.messages().len();
        let cmd = format!("/save {save_path_str}");
        handle_save(&agent, &cmd);

        let mut agent2 = make_test_agent();
        assert_eq!(agent2.messages().len(), 0);
        let cmd = format!("/load {save_path_str}");
        handle_load(&mut agent2, &cmd);
        assert_eq!(agent2.messages().len(), saved_len);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_handle_load_nonexistent_does_not_crash() {
        let mut agent = make_test_agent();
        handle_load(&mut agent, "/load /tmp/yoyo_nonexistent_12345.json");
    }

    #[test]
    fn test_handle_load_custom_path() {
        let tmp_dir = std::env::temp_dir().join("yoyo_test_load_custom");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let save_path = tmp_dir.join("custom.json");
        let save_path_str = save_path.to_str().unwrap();

        let agent = make_agent_with_messages(&["custom data"]);
        let cmd = format!("/save {save_path_str}");
        handle_save(&agent, &cmd);

        let mut agent2 = make_test_agent();
        let cmd = format!("/load {save_path_str}");
        handle_load(&mut agent2, &cmd);
        assert_eq!(agent2.messages().len(), agent.messages().len());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    // ── handle_history tests ────────────────────────────────────────────

    #[test]
    fn test_handle_history_empty() {
        let agent = make_test_agent();
        handle_history(&agent); // Should print "(no messages)"
    }

    #[test]
    fn test_handle_history_with_messages() {
        let agent = make_agent_with_messages(&["first", "second", "third"]);
        handle_history(&agent); // Should list 3 messages
    }

    // ── handle_compact tests ────────────────────────────────────────────

    #[test]
    fn test_handle_compact_empty() {
        let mut agent = make_test_agent();
        handle_compact(&mut agent); // Should print "(nothing to compact)"
    }

    #[test]
    fn test_handle_compact_with_messages() {
        let mut agent = make_agent_with_messages(&["hello"]);
        handle_compact(&mut agent); // Should not panic
    }
}
