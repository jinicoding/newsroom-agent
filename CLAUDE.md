# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

A self-evolving journalist assistant agent (기자업무보조 에이전트) CLI built on [yoagent](https://github.com/yologdev/yoagent). Helps Korean newspaper reporters with research, article writing, fact-checking, and source management. Evolution runs via `scripts/evolve.sh` (currently local-only; GitHub Actions schedule is disabled) using a 3-phase pipeline (plan → implement → respond), which reads its own source, picks improvements, implements them, and commits — if tests pass.

## Build & Test Commands

```bash
cargo build              # Build
cargo test               # Run tests
cargo test test_name     # Run a single test
cargo clippy --all-targets -- -D warnings   # Lint (CI treats warnings as errors)
cargo fmt -- --check     # Format check
cargo fmt                # Auto-format
```

CI runs all four checks (build, test, clippy with -D warnings, fmt check) on PR to main. A separate Pages workflow builds and deploys the website on push to main.

To run the agent interactively:
```bash
ANTHROPIC_API_KEY=sk-... cargo run
ANTHROPIC_API_KEY=sk-... cargo run -- --model claude-opus-4-6 --skills ./skills
```

Key CLI flags: `--model`, `--provider`, `--base-url`, `--thinking`, `--max-tokens`, `--skills`, `--system`/`--system-file`, `--prompt`/`-p`, `--output`/`-o`, `--mcp`, `--openapi`, `--allow`/`--deny`, `--continue`/`-c`, `--verbose`/`-v`, `--yes`/`-y`. Config file: `.yoyo.toml` or `~/.config/yoyo/config.toml`.

To trigger a full evolution cycle (local):
```bash
ANTHROPIC_API_KEY=sk-... ./scripts/evolve.sh
```

## Architecture

### Context file resolution

The agent looks for project instructions in order: `YOYO.md` → `CLAUDE.md` → `.yoyo/instructions.md`. `YOYO.md` is the canonical name; `CLAUDE.md` is a compatibility alias.

### Source (`src/`) — 12 files, ~30k lines

**Kernel layer** (agent core + interaction):
- `main.rs` — agent initialization, multi-provider setup (11 providers: Anthropic/OpenAI/Google/Ollama/OpenRouter/xAI/Groq/DeepSeek/Mistral/Cerebras/Custom), streaming event handling, REPL dispatch. `GuardedTool` wrapper enforces `--allow`/`--deny` directory restrictions on file tools
- `repl.rs` — rustyline-based interactive loop with tab-completion (slash commands, file paths, command arguments, model names, provider names)
- `cli.rs` — CLI argument parsing, config file support (`.yoyo.toml` / `~/.config/yoyo/config.toml`), `DirectoryRestrictions` struct for path-based permission patterns
- `prompt.rs` — streaming execution with automatic retries, exponential backoff, error classification

**Command dispatcher** (66 slash commands split by domain):
- `commands.rs` — central hub: `KNOWN_COMMANDS` registry (66 entries), auto-compact trigger, tab-completion routing, common handlers
- `commands_git.rs` — `/diff`, `/commit` (AI-generated conventional commits), `/pr`, `/review`, `/undo`
- `commands_project.rs` (~16k lines, largest file) — journalism-specific commands (`/article`, `/research`, `/sources`, `/factcheck`, `/briefing`, `/interview`, `/timeline`, `/headline`, `/rewrite`, `/draft`, `/proofread`, `/legal`, `/embargo`, `/anonymize`, `/performance`, `/network`, `/autopitch`, etc.) plus dev commands (`/health`, `/fix`, `/test`, `/lint`, `/init`, `/docs`, `/find`, `/tree`, `/index`)
- `commands_session.rs` — `/save`, `/load`, `/compact`, `/search`, `/mark`/`/jump`, `/spawn` (subagents)

**Support modules**:
- `memory.rs` — project-local memory (`.yoyo/memory.json`), `/remember`, `/memories`, `/forget`. Separate from evolution memory in `memory/`
- `git.rs` — low-level git operations: branch detection, staged diffs, conventional commit message generation
- `docs.rs` — docs.rs crate documentation fetcher: HTML parsing, item extraction (struct/enum/trait/fn/module)
- `format.rs` — ANSI colors (respects NO_COLOR), syntax highlighting, token/cost formatting

Uses `yoagent::Agent` with `AnthropicProvider` (default), `default_tools()`, and an optional `SkillSet`. Context window is 200k tokens with auto-compact at 80%. Provider switchable mid-session via `/provider`.

### Evolution loop (`scripts/evolve.sh`)

3-phase pipeline:
1. Verifies build → fetches GitHub issues via `gh` CLI + `scripts/format_issues.py` → scans for pending replies
2. **Phase A** (Planning): Agent reads everything, writes `SESSION_PLAN.md`
3. **Phase B** (Implementation): Per-task agents execute (15 min timeout each), test independently
4. **Phase C** (Communication): Extracts issue responses from plan, posts via `gh`
5. Verifies build, fixes or reverts → pushes

### Skills (`skills/`)

Markdown files with YAML frontmatter loaded via `--skills ./skills`. Six skills (immutable):
- `self-assess` — read own code, try tasks, find bugs/gaps
- `evolve` — safely modify source, test, revert on failure
- `communicate` — write journal entries and issue responses
- `research` — internet lookups and knowledge caching
- `release` — release procedures
- `social` — community interaction and learning

### Memory system (`memory/`)

Two-layer architecture — append-only JSONL archives (source of truth, never compressed) and active context markdown (regenerated daily by `.github/workflows/synthesize.yml` with time-weighted compression):
- `memory/learnings.jsonl` — self-reflection archive. Format: `{"type":"lesson","day":N,"ts":"ISO8601","source":"...","title":"...","context":"...","takeaway":"..."}`
- `memory/social_learnings.jsonl` — social insight archive. Format: `{"type":"social","day":N,"ts":"ISO8601","source":"...","who":"@user","insight":"..."}`
- `memory/active_learnings.md`, `memory/active_social_learnings.md` — synthesized prompt context
- Archives appended via `python3` with `json.dumps()` (never `echo` — prevents quote-breaking)
- Context loaded centrally by `scripts/yoyo_context.sh` → `$YOYO_CONTEXT`

### State files

- `IDENTITY.md` — the agent's constitution and rules (DO NOT MODIFY)
- `PERSONALITY.md` — voice and values (DO NOT MODIFY)
- `JOURNAL.md` — chronological log of evolution sessions (append at top, never delete)
- `DAY_COUNT` — integer tracking current evolution day
- `SESSION_PLAN.md`, `ISSUES_TODAY.md`, `ISSUE_RESPONSE.md` — ephemeral, gitignored

### Documentation (`docs/`)

mdbook source in `docs/src/`, config in `docs/book.toml`. Output goes to `site/book/` (gitignored). Journal homepage (`site/index.html`) built by `scripts/build_site.py`. Both deployed by the Pages workflow.

### Local scripts (`scripts/`)

- `evolve.sh` / `evolve_local.sh` — full 3-phase evolution pipeline (immutable, do not modify)
- `daily_diary.sh` — daily journal entry generation
- `social.sh` / `social_local.sh` — GitHub discussions → social learning ingestion
- `synthesize_local.sh` — local run of memory archive → active context compression
- `yoyo_context.sh` — central context loader, exports `$YOYO_CONTEXT` for evolution agents
- `build_site.py` — journal homepage builder (immutable)
- `format_issues.py` / `format_discussions.py` — GitHub data formatters (immutable)

### CI/CD Workflows (`.github/workflows/`)

- `ci.yml` — PR gate: build, test, clippy, fmt
- `evolve.yml` — evolution pipeline (schedule currently disabled; use `evolve.sh` or `evolve_local.sh` locally)
- `pages.yml` — builds mdbook + journal homepage to GitHub Pages on push to main
- `synthesize.yml` — daily cron: compresses memory archives into active context
- `social.yml` — processes GitHub discussions for social learning

## Safety Rules

Enforced by the `evolve` skill and `evolve.sh`:
- Never modify `IDENTITY.md`, `PERSONALITY.md`, `scripts/evolve.sh`, `scripts/format_issues.py`, `scripts/build_site.py`, or `.github/workflows/`
- Every code change must pass `cargo build && cargo test`
- If build fails after changes, revert with `git checkout -- src/ Cargo.toml Cargo.lock`
- Never delete existing tests
- Multiple tasks per evolution session, each verified independently
- Write tests before adding features
