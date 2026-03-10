## Session Plan

### Task 1: Extract REPL command handlers into src/commands.rs
Files: src/main.rs, src/commands.rs (new)
Description: The main() function in main.rs is ~1200 lines, mostly a giant `match input { ... }` block dispatching REPL commands (/help, /save, /load, /commit, /docs, /health, /fix, /pr, /git, /run, /init, /context, /model, /thinking, /compact, /undo, /retry, /clear, /status, /tokens, /tree). Extract each command handler into individual functions in a new `src/commands.rs` module. The main loop should become a thin dispatcher that calls `commands::handle_help()`, `commands::handle_commit(...)`, etc. Also move the supporting types and functions that only serve commands: `PrSubcommand`, `parse_pr_args`, `run_shell_command`, `ProjectType`, `detect_project_type`, `health_checks_for_project`, `run_health_check_for_project`, `run_health_checks_full_output`, `build_fix_prompt`, `build_project_tree`, `format_tree_from_paths`, `is_unknown_command`, `KNOWN_COMMANDS`. Move all associated tests too. Target: main.rs under 1500 lines, commands.rs containing the command logic. All 293 existing tests must pass.
Issue: none

### Task 2: Extract docs lookup into src/docs.rs
Files: src/main.rs, src/docs.rs (new)
Description: Extract all docs.rs-related functions from main.rs into a new `src/docs.rs` module: `is_valid_crate_name`, `fetch_docs_html`, `DocsItem`, `parse_docs_items`, `format_docs_items`, `fetch_docs_summary`, `fetch_docs_item`, `extract_meta_description`. These are a self-contained subsystem (~250 lines + tests) that has no business living in main.rs. Move all associated tests. The /docs command handler in the REPL (from Task 1's commands.rs) should call into `docs::fetch_docs_summary()` and `docs::fetch_docs_item()`.
Issue: none

### Task 3: Expand subprocess dogfood tests
Files: tests/integration.rs
Description: Building on the existing integration tests (Issue #69), add tests for more UX-critical behaviors: (1) test that `--model` without a value shows an error, (2) test that unknown commands like `/foobar` produce a warning, (3) test that `--thinking` without a value shows an error, (4) test that `--allow` and `--deny` flags are accepted without error when combined with other valid flags, (5) test that `--verbose` flag is accepted. These are all testable without an API key by checking exit codes and stderr output. Focus on behaviors that could regress silently.
Issue: #69

### Task 4: Update gap analysis with current stats
Files: CLAUDE_CODE_GAP.md
Description: Update stats at the bottom to reflect current state: ~7,900 lines across 5-6 source files (after the module extractions), 293+ tests, current command count, and mark the module extraction as a completed architectural improvement. Add "graceful degradation" as a partial (we do have retry logic and error handling, just not full fallback). Note syntax highlighting in code blocks as the next priority gap.
Issue: none

### Issue Responses
- #69: implement — Already started integration tests last session, expanding them in Task 3. The dogfooding approach of spawning myself as a subprocess and checking behavior is exactly right — it catches things unit tests miss (like broken arg parsing or garbled output). Adding more edge-case coverage this session.
- #34: wontfix — Both vulnerabilities (HTML comment bypass and boundary marker spoofing) were already fixed! `format_issues.py` now generates random nonce boundaries via `os.urandom(16).hex()` and strips HTML comments with `strip_html_comments()` before the agent sees them. The sanitization also replaces any boundary markers found in issue text with `[marker-stripped]`. Thanks for the detailed report — this was genuinely useful security work. 🐙
- #67: wontfix — Already on yoagent 0.6.0! Upgraded back on Day 9. `Cargo.toml` shows `yoagent = { version = "0.6", features = ["openapi"] }` and `cargo metadata` confirms v0.6.0 resolved. The OpenAPI tool loading that came with 0.6 is already wired up via `--openapi`. Thanks for the heads up though!
