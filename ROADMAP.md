# Roadmap

My evolution path. I work through levels in order. Items come from three sources:
- This planned curriculum
- GitHub issues from the community (marked with issue number)
- Things I discover myself during self-assessment (marked with [self])

## Level 1: Survive (Day 1–7)

Learn to not break. Build trust in my own code.

- [x] Add `--help` flag with usage info
- [x] Add error handling for API failures (bad key, network down, rate limit)
- [x] Add `--version` flag
- [x] Fix any panics — catch all unwrap() calls and handle properly
- [ ] Write tests for existing functionality (REPL loop, command parsing)
- [ ] Handle Ctrl+C gracefully (cancel current turn, don't kill process)

## Level 2: Be Useful (Day 8–20)

Features that make me worth using for real work.

- [ ] Git awareness: detect if we're in a repo, show branch in prompt
- [ ] Auto-commit: commit changes after successful edits (with confirmation)
- [ ] Diff preview: show what changed before applying edits
- [ ] `/undo` command: revert the last file change
- [ ] Conversation persistence: save/restore sessions to disk
- [ ] `/save` and `/load` commands for sessions
- [ ] Multi-line input: support pasting code blocks
- [ ] Token usage tracking across entire session (cumulative)
- [ ] Configurable system prompt via `--system` flag or config file

## Level 3: Be Smart (Day 21–40)

Intelligence improvements. Think before acting.

- [ ] Context management: warn when approaching token limit
- [ ] Smart retry: if a tool fails, try a different approach
- [ ] Permission system: confirm before destructive commands (rm, overwrite)
- [ ] Project detection: read Cargo.toml, package.json, etc. and adapt
- [ ] Auto-test: run project tests after making code changes
- [ ] `/compact` command: summarize old conversation to free context
- [ ] Error recovery: if edit_file fails, suggest alternatives

## Level 4: Be Professional (Day 41–60)

Features that separate a toy from a tool.

- [ ] Multi-provider support: `--provider openai` / `--provider groq` flags
- [ ] Config file: `~/.yoagent.toml` for defaults
- [ ] MCP server connection via `--mcp` flag
- [ ] Session logging: save full sessions with timestamps
- [ ] `/replay` command: re-execute a saved session
- [ ] Performance metrics: report response times per turn
- [ ] Markdown rendering in terminal output
- [ ] `/diff` command: show git diff of all changes made this session

## Boss Level: Prove It

- [ ] Complete a SWE-bench Lite task successfully
- [ ] Complete a Terminal-bench task successfully
- [ ] Build a full project from a single prompt (Rust web API with tests)
- [ ] Refactor a real open-source project's module without breaking tests
