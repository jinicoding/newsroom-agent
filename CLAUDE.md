# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A self-evolving coding agent CLI built on [yoagent](https://github.com/yologdev/yoagent). The entire agent lives in `src/main.rs` (~230 lines of Rust). A GitHub Actions cron job (`scripts/evolve.sh`) runs the agent every 8 hours, which reads its own source, picks improvements, implements them, and commits — if tests pass.

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo clippy --all-targets -- -D warnings   # Lint (CI treats warnings as errors)
cargo fmt -- --check     # Format check
cargo fmt                # Auto-format
```

CI runs all four checks (build, test, clippy with -D warnings, fmt check) on push/PR to main.

To run the agent interactively:
```bash
ANTHROPIC_API_KEY=sk-... cargo run
ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6 --skills ./skills
```

To trigger a full evolution cycle:
```bash
ANTHROPIC_API_KEY=sk-... ./scripts/evolve.sh
```

## Architecture

**Single-file agent**: `src/main.rs` is the entire application — a REPL that uses `yoagent::Agent` with `AnthropicProvider`, `default_tools()`, and an optional `SkillSet`. It handles streaming `AgentEvent`s (tool execution, text deltas, agent end) and renders them with ANSI colors.

**Evolution loop** (`scripts/evolve.sh`): Verifies build → fetches GitHub issues (via `gh` CLI + `scripts/format_issues.py`) → pipes a structured prompt into the agent → verifies build after changes → commits or reverts → posts issue responses → pushes.

**Skills** (`skills/`): Markdown files with YAML frontmatter loaded via `--skills ./skills`. Three skills define the agent's evolution workflow:
- `self-assess` — read own code, try tasks, find bugs/gaps
- `evolve` — safely modify source, test, revert on failure
- `communicate` — write journal entries and issue responses

**State files** (read/written by the agent during evolution):
- `IDENTITY.md` — the agent's constitution and rules (DO NOT MODIFY)
- `JOURNAL.md` — chronological log of evolution sessions (append at top, never delete)
- `LEARNINGS.md` — cached knowledge from internet lookups
- `DAY_COUNT` — integer tracking current evolution day
- `ISSUES_TODAY.md` — ephemeral, generated during evolution from GitHub issues (gitignored)
- `ISSUE_RESPONSE.md` — ephemeral, agent writes this to respond to issues (gitignored)

## Safety Rules

These are enforced by the `evolve` skill and `evolve.sh`:
- Never modify `IDENTITY.md`, `scripts/evolve.sh`, or `.github/workflows/`
- Every code change must pass `cargo build && cargo test`
- If build fails after changes, revert with `git checkout -- src/`
- Never delete existing tests
- One improvement per evolution session — small, focused changes only
- Write tests before adding features
