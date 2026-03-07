# REPL Commands

All commands start with `/`. Type `/help` inside yoyo to see the full list.

## Navigation

| Command | Description |
|---------|-------------|
| `/quit`, `/exit` | Exit yoyo |
| `/help` | Show available commands |

## Conversation

| Command | Description |
|---------|-------------|
| `/clear` | Clear conversation history and start fresh |
| `/compact` | Compress conversation to save context space (see [Context Management](../features/context.md)) |
| `/retry` | Re-send your last input — useful when a response gets cut off or you want to try again |
| `/history` | Show a summary of all messages in the conversation |
| `/search <query>` | Search conversation history for messages containing the query (case-insensitive) |

## Model & Thinking

| Command | Description |
|---------|-------------|
| `/model <name>` | Switch to a different model (preserves conversation) |
| `/think [level]` | Show or change thinking level: `off`, `minimal`, `low`, `medium`, `high` |

Examples:
```
/model claude-sonnet-4-20250514
/think high
/think off
```

The `/think` command preserves your conversation history — only the thinking level changes. The `/model` command also preserves conversation.

## Session

| Command | Description |
|---------|-------------|
| `/save [path]` | Save conversation to a file (default: `yoyo-session.json`) |
| `/load [path]` | Load conversation from a file (default: `yoyo-session.json`) |

See [Session Persistence](../features/sessions.md) for details.

## Information

| Command | Description |
|---------|-------------|
| `/status` | Show current model, git branch, working directory, and session token totals |
| `/tokens` | Show detailed token usage: context window fill level, session totals, and estimated cost |
| `/cost` | Show estimated session cost |
| `/config` | Show all current settings |
| `/version` | Show yoyo version |

The `/tokens` command shows a visual progress bar of your context window:

```
  Context window:
    messages:    12
    context:     45.2k / 200.0k tokens
    █████████░░░░░░░░░░░ 23%
```

## Shell

| Command | Description |
|---------|-------------|
| `/run <cmd>` | Run a shell command directly — no AI, no tokens used |
| `!<cmd>` | Shortcut for `/run` |

The `/run` command (or `!` shortcut) lets you execute shell commands without going through the AI model. Useful for quick checks (e.g., `!git log --oneline -5`) without burning API tokens.

```
/run ls -la src/
/run cargo test
/run git status
```

## Git

| Command | Description |
|---------|-------------|
| `/diff` | Show `git status --short` and `git diff --stat` of uncommitted changes |
| `/undo` | Revert all uncommitted changes (`git checkout -- .` and `git clean -fd`) |
| `/pr [number]` | List open PRs (`gh pr list`), or view a specific PR (`gh pr view <number>`) |
| `/health` | Run health checks: build, test, clippy, fmt — reports pass/fail with timing |

The `/undo` command shows you what will be reverted before doing it.

The `/pr` command is a quick wrapper around the [GitHub CLI](https://cli.github.com). Use `/pr` to list the 10 most recent open pull requests, or `/pr 42` to see details of PR #42. For commenting, merging, or closing PRs, use `/run gh pr ...` or ask the agent directly — it has full bash access.

The `/health` command runs `cargo build`, `cargo test`, `cargo clippy`, and `cargo fmt --check`, reporting each result with timing. Useful for verifying the agent hasn't broken itself.

## Project Context

| Command | Description |
|---------|-------------|
| `/context` | Show which project context files are loaded (YOYO.md, CLAUDE.md, etc.) |
| `/init` | Create a starter YOYO.md project context file |
| `/tree [depth]` | Show project directory tree (default depth: 3, respects `.gitignore`) |

The `/tree` command uses `git ls-files` to show tracked files in a visual tree structure, automatically respecting your `.gitignore`. You can specify a depth limit:

```
/tree        # default: 3 levels deep
/tree 1      # just top-level directories and their files
/tree 5      # deeper view
```

Example output:
```
src/
  cli.rs
  format.rs
  main.rs
  prompt.rs
Cargo.toml
README.md
```

## Unknown commands

If you type a `/command` that yoyo doesn't recognize, it will tell you:

```
  unknown command: /foo
  type /help for available commands
```

Note: lines starting with `/` that contain spaces (like `/model name`) are treated as command arguments, not unknown commands.
