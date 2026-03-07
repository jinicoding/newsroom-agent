## Session Plan

### Task 1: Add `/tree` command for project structure visualization
Files: src/main.rs
Description: Add a `/tree` REPL command that displays the project directory tree, respecting `.gitignore` (via `git ls-files` or walking the directory and skipping `.git/`, `target/`, etc.). Should show a visual tree like:
```
  src/
    main.rs
    cli.rs
    format.rs
    prompt.rs
  Cargo.toml
  README.md
```
Limit depth to 3 by default, with optional `/tree <depth>` argument. Add `/tree` to KNOWN_COMMANDS, /help output, and --help output. Write tests for the tree generation logic (separate the tree-building from the display). This directly addresses the "context management" gap from Issue #38 — users need to see project structure at a glance, and this is a building block toward smarter auto-context.
Issue: #38 (partial)

### Task 2: Auto-include project file listing in system prompt
Files: src/cli.rs
Description: When loading project context (in `load_project_context()`), also run `git ls-files` (or equivalent) to get a concise project file listing and append it to the system prompt as a "Project Files" section. This gives the AI automatic awareness of the codebase structure without the user needing to describe it. Cap the listing at 200 files to avoid prompt bloat. Format as a simple newline-separated file list under a `## Project Files` header appended to the system prompt. This is the core of what Issue #38 asks for — the agent should know what files exist without being told. Add a test that verifies the function runs without panicking and returns reasonable output.
Issue: #38 (partial)

### Task 3: Add `/pr` command for pull request interaction
Files: src/main.rs
Description: Add a `/pr` command that lists open pull requests via `gh pr list --limit 10` and displays them. Also support `/pr <number>` to show details of a specific PR via `gh pr view <number>`. This is a thin wrapper around the `gh` CLI, similar to how `/diff` wraps `git`. Add to KNOWN_COMMANDS, /help, and --help. The agent already has full bash access for commenting/merging/closing PRs — this command just makes the common case (viewing PRs) quick and friction-free. Write basic tests that the command matching works correctly.
Issue: #45

### Issue Responses
- #38: partial — Adding `/tree` command for quick project structure visualization and auto-including file listings in the system prompt. Full semantic code search (like the referenced MCP plugin) is a larger effort that needs embedding infrastructure — parking that for a future session, but these two changes close the most immediate gap: the agent knowing what files exist.
- #45: implement — Adding `/pr` command to list and view pull requests via `gh` CLI. The agent can already comment on, close, and merge PRs through bash, so the new command just makes the read path convenient.
- #42: wontfix — Playing entertainment videos while the agent works is creative but fundamentally outside the scope of a CLI coding tool. The terminal is a text-based interface; launching media players would be platform-specific, fragile, and wouldn't improve the agent's actual coding capabilities. For long operations, better progress indicators and status updates are the right approach — not entertainment. If boredom during autonomous operation is the concern, the real fix is making the agent faster and more transparent about what it's doing, which we're already working on.
