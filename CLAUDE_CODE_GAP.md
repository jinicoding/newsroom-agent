# Gap Analysis: yoyo vs Claude Code

Last updated: Day 12 (2026-03-12)

This document tracks the feature gap between yoyo and Claude Code, used to inform development priorities when there are no community issues to address.

## Legend
- ✅ **Implemented** — yoyo has this
- 🟡 **Partial** — yoyo has a basic version, Claude Code's is better
- ❌ **Missing** — yoyo doesn't have this yet

---

## Core Agent Loop

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Streaming text output | ✅ | ✅ | Both stream tokens as they arrive |
| Tool execution | ✅ | ✅ | bash, read_file, write_file, edit_file, search, list_files |
| Multi-turn conversation | ✅ | ✅ | Both maintain conversation history |
| Thinking/reasoning display | ✅ | ✅ | yoyo shows thinking dimmed |
| Error recovery / auto-retry | ✅ | ✅ | yoagent retries 3x with exponential backoff by default |
| Subagent / task spawning | 🟡 | ✅ | Basic `/spawn` runs tasks in separate context; Claude Code has richer orchestration |
| Parallel tool execution | ❌ | ✅ | Claude Code can run multiple tools in parallel |
| Tool output streaming | 🟡 | ✅ | `ToolExecutionUpdate` events handled; no real-time subprocess streaming yet |

## CLI & UX

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Interactive REPL | ✅ | ✅ | |
| Piped/stdin mode | ✅ | ✅ | |
| Single-shot prompt (-p) | ✅ | ✅ | |
| Output to file (-o) | ✅ | ✅ | |
| Model selection | ✅ | ✅ | --model flag and /model command |
| Session save/load | ✅ | ✅ | /save, /load, --continue |
| Git integration | ✅ | ✅ | Branch in prompt, /diff, /undo |
| Readline / line editing | ✅ | ✅ | rustyline: arrow keys, history (~/.local/share/yoyo/history), Ctrl-A/E/K/W |
| Tab completion | 🟡 | ✅ | Slash commands + file paths; no argument-aware completion yet |
| Fuzzy file search | ✅ | ✅ | `/find` with scoring, git-aware file listing, top-10 ranked results (Day 12) |
| Syntax highlighting | ✅ | ✅ | Language-aware ANSI highlighting for Rust, Python, JS/TS, Go, Shell, C/C++, JSON, YAML, TOML |
| Markdown rendering | ✅ | ✅ | Incremental ANSI: headers, bold, code blocks, inline code, syntax-highlighted code blocks |
| Progress indicators | ✅ | ✅ | Braille spinner animation during AI responses (Day 8) |
| Multi-line input | ✅ | ✅ | Backslash continuation and code fences |
| Custom system prompts | ✅ | ✅ | --system and --system-file |
| Extended thinking control | ✅ | ✅ | --thinking flag |
| Color control | ✅ | ✅ | --no-color, NO_COLOR env |

## Context Management

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Auto-compaction | ✅ | ✅ | Triggers at 80% context |
| Manual compaction | ✅ | ✅ | /compact command |
| Token usage display | ✅ | ✅ | /tokens with visual bar |
| Cost estimation | ✅ | ✅ | Per-request and session totals |
| Context window awareness | ✅ | ✅ | 200k token limit tracked |

## Permission System

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Tool approval prompts | ✅ | ✅ | `--yes`/`-y` to auto-approve; `with_confirm` for interactive bash approval |
| Allowlist/blocklist | ✅ | ✅ | `--allow`/`--deny` flags with glob matching; `[permissions]` config section; deny overrides allow |
| Directory restrictions | ❌ | ✅ | Claude Code can restrict file access |
| Auto-approve patterns | ✅ | ✅ | `--allow` glob patterns + config file `allow` array; "always" option during confirm |

## Project Understanding

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Project context files | ✅ | ✅ | yoyo reads YOYO.md, CLAUDE.md, and .yoyo/instructions.md |
| Auto-detect project type | ✅ | ✅ | `detect_project_type` used by `/test`, `/lint`, `/health`, `/fix` (Rust, Node, Python, Go, Make) |
| Git-aware file selection | ✅ | ✅ | `get_recently_changed_files` appended to project context (Day 12) |
| Codebase indexing | ❌ | ✅ | Claude Code indexes for faster search |

## Developer Workflow

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Run tests | ✅ | ✅ | `/test` auto-detects project type and runs tests (Day 12) |
| Auto-fix lint errors | ✅ | ✅ | `/lint` auto-detects and runs linter; `/fix` sends failures to AI (Day 9+12) |
| PR description generation | ❌ | ✅ | Claude Code generates PR descriptions |
| Commit message generation | ✅ | ✅ | `/commit` with heuristic-based message generation from staged diff (Day 8) |
| Multi-file refactoring | 🟡 | ✅ | yoyo can via tools; Claude Code is better at coordinating |

## Configuration

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| Config file | ✅ | ✅ | yoyo reads .yoyo.toml and ~/.config/yoyo/config.toml |
| Per-project settings | ✅ | ✅ | .yoyo.toml in project directory |
| Custom tool definitions | ✅ | ✅ | yoyo supports MCP servers via `--mcp` (stdio transport) |
| Multi-provider support | ✅ | ❌ | yoyo supports 10+ providers via `--provider` (anthropic, openai, google, ollama, etc.) |
| Skills/plugins | ✅ | ✅ | yoyo has --skills; Claude Code has MCP |
| OpenAPI tool support | ✅ | ❌ | `--openapi <spec>` loads OpenAPI specs and registers API tools (Day 9) |

## Error Handling

| Feature | yoyo | Claude Code | Notes |
|---------|------|-------------|-------|
| API error display | ✅ | ✅ | Shows error messages |
| Network retry | ✅ | ✅ | yoagent handles 3 retries with exponential backoff by default |
| Rate limit handling | ✅ | ✅ | yoagent respects retry-after headers on 429s |
| Graceful degradation | 🟡 | ✅ | yoyo has retry logic and error handling; not yet full fallback on partial failures |
| Ctrl+C handling | ✅ | ✅ | Both handle interrupts |

---

## Priority Queue (what to build next)

Based on this analysis, the highest-impact missing features are:

1. **Parallel tool execution** — Speed up multi-tool workflows
2. **Argument-aware tab completion** — Complete --model values, file args for /load, etc.
3. **Codebase indexing** — Index project files for faster search
4. **Directory restrictions** — Restrict file access to specific directories

Recently completed:
- ✅ Fuzzy file search (Day 12) — `/find` with scoring, git-aware file listing, ranked results
- ✅ Git-aware context (Day 12) — `get_recently_changed_files` appended to project context
- ✅ Syntax highlighting (Day 12) — language-aware ANSI highlighting for 8+ languages
- ✅ REPL module extraction (Day 12) — extracted repl.rs from main.rs for cleaner separation
- ✅ AgentConfig extraction (Day 12) — centralized config into AgentConfig struct in main.rs
- ✅ `/spawn` subagent support (Day 12) — run tasks in separate context with `/spawn <task>`
- ✅ `/test` command (Day 12) — auto-detect project type and run tests
- ✅ `/lint` command (Day 12) — auto-detect project type and run linter
- ✅ Conversation search highlighting (Day 12) — `/search` highlights matches in results
- ✅ Module extraction (Day 10+12) — split main.rs into 8 focused modules: cli, commands, docs, format, git, main, prompt, repl
- ✅ OpenAPI tool support (Day 9) — `--openapi <spec>` loads specs and registers API tools
- ✅ yoagent 0.6.0 upgrade (Day 9) — updated to yoagent 0.6 with OpenAPI feature
- ✅ Permission system (Day 9) — `--allow`/`--deny` glob flags, `[permissions]` config, deny-overrides-allow
- ✅ Auto-fix lint errors (Day 9) — `/fix` command runs checks and sends failures to AI
- ✅ Project type detection (Day 9) — `detect_project_type` for Rust, Node, Python, Go, Make
- ✅ Commit message generation (Day 8) — `/commit` with heuristic-based message generation
- ✅ Progress indicators (Day 8) — braille spinner animation during AI responses
- ✅ Multi-provider support (Day 8) — 10+ providers via `--provider` flag
- ✅ MCP server support (Day 8) — connect to MCP servers via `--mcp`
- ✅ Markdown rendering (Day 8) — incremental ANSI formatting for streamed output
- ✅ Tab completion (Day 8) — slash commands + file path completion

## Stats

- yoyo: ~10,000 lines of Rust across 8 source files
- 438 tests passing (376 unit + 62 integration)
- 33 REPL commands (including /spawn, /find, /docs, /fix, /lint)
- 23 CLI flags (+ short aliases)
- 10+ provider backends
- MCP server support
- OpenAPI tool loading
- Config file support (.yoyo.toml)
- Permission system (allow/deny globs)
- Subagent spawning (/spawn)
- Fuzzy file search (/find)
- Git-aware project context
- Syntax highlighting for 8+ languages
