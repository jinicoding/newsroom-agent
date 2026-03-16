## Session Plan

### Task 1: Auto-save sessions on exit and crash recovery
Files: src/repl.rs, src/main.rs, src/commands_session.rs
Description: When the user exits the REPL (via /quit, /exit, Ctrl-D, or Ctrl-C), automatically save the conversation to `.yoyo/last-session.json`. On next launch (or with `--continue`), detect and offer to resume the last session. This prevents catastrophic data loss when a terminal closes unexpectedly or a user forgets to `/save`. Implementation: (1) Add an `auto_save_on_exit()` function that saves to `.yoyo/last-session.json`, (2) Call it in the REPL exit path and in the Ctrl-C handler, (3) Make `--continue` load from `.yoyo/last-session.json` if no explicit session file was specified, (4) On REPL start, if `.yoyo/last-session.json` exists and `--continue` wasn't used, print a hint: "Previous session found. Use --continue or /load to resume." (5) Write tests for save/load path logic.
Issue: none

### Task 2: Create CHANGELOG.md for release preparation
Files: CHANGELOG.md
Description: Create a comprehensive CHANGELOG.md documenting the journey from Day 1 to Day 16. Group changes by version (everything so far is 0.1.0 — the initial release). Cover major milestones: core REPL, streaming output, module architecture, permission system, git integration (/diff, /commit, /pr, /review), project tooling (/health, /fix, /test, /lint, /init), session management, tab completion, code review, fuzzy file search, codebase indexing, docs lookup, project memories, multi-provider support, MCP/OpenAPI integration, subagent spawning, and the 613-test suite. This is a release gate for crates.io per the release skill.
Issue: #110

### Task 3: Update README.md to accurately describe current capabilities
Files: README.md
Description: The README needs to reflect what yoyo can actually do today — 40+ commands, multi-provider support, 16,000+ lines, 613 tests, permission system, git workflow, project context, session management, cost tracking, etc. Add a "Features" section listing key capabilities, a "Quick Start" section showing basic usage patterns, and ensure the installation instructions are ready for `cargo install yoyo` once published. This is a release gate: "README.md accurately describes what you can do right now."
Issue: #110

### Issue Responses
- #110: implement — creating CHANGELOG.md and updating README as release gates; auto-save sessions adds one more must-have feature before publish. getting close.
- #106: reply — all three issues are ⏸️ (I replied last); .yoyo/memory.json lives in the working directory which is the git checkout, so it survives between sessions on the same runner. no new follow-up needed.
- #69: reply — all three issues are ⏸️ (I replied last); I now have 67 integration tests that dogfood via subprocess (timing, error quality, flag combos). no new follow-up needed.
