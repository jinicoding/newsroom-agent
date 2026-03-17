## Session Plan

### Task 1: Add unit tests for /sources CRUD operations
Files: src/commands_project.rs
Description: commands_project.rs currently has ZERO tests, which is a serious gap for the journalist workflow commands. Add a #[cfg(test)] module with unit tests covering:
- `load_sources()` / `save_sources()` round-trip with a temp file path
- `sources_add()` parsing (name/org/contact/note from space-separated input)
- `sources_search()` case-insensitive matching across all fields
- Edge cases: empty DB, malformed JSON gracefully handled, missing fields
- Create testable versions of the functions that accept a path parameter (like memory.rs does) rather than hardcoding SOURCES_FILE. This may require refactoring load_sources/save_sources to accept a path, similar to how memory.rs has load_memories_from/save_memories_to.
Issue: none

### Task 2: Fix /research to use working search sources (Google News RSS)
Files: src/commands_project.rs
Description: The current /research command instructs the AI to curl DuckDuckGo lite, which now blocks bots with a CAPTCHA, and Naver News search which returns JS-rendered pages unusable with curl. Replace these with working alternatives:
1. Google News RSS: `curl -s "https://news.google.com/rss/search?q={encoded_query}&hl=ko&gl=KR&ceid=KR:ko"` — confirmed working, returns Korean news headlines with sources
2. Keep a fallback mention of DuckDuckGo HTML but make Google News RSS the primary search tool
3. The prompt should instruct the AI to parse XML title tags from the RSS feed
4. Update the research prompt template to reflect these working search methods
Issue: none

### Task 3: Add /sources remove subcommand
Files: src/commands_project.rs, src/commands.rs
Description: Reporters need to remove outdated sources from their DB. Add a `/sources remove <number>` subcommand that:
1. Takes a 1-based index number (matching the display from `/sources list`)
2. Shows the source being removed and asks for confirmation (or just removes it directly for simplicity)
3. Saves the updated DB
4. Add "remove" to SOURCES_SUBCOMMANDS in commands.rs for tab-completion
5. Add tests for the remove operation
Issue: none

### Task 4: Add /brief command for daily news briefing
Files: src/commands_project.rs, src/commands.rs, src/repl.rs
Description: Add a `/brief [topic]` command that gives reporters a quick news briefing. This is a high-value daily workflow:
1. Register "/brief" in KNOWN_COMMANDS in commands.rs
2. Add dispatch in repl.rs
3. Implementation in commands_project.rs:
   - If topic provided: fetch Google News RSS for that topic (Korean), extract top 10 headlines, and ask AI to summarize key trends
   - If no topic: show usage help suggesting common beats (정치, 경제, 사회, IT/과학, 국제)
4. The prompt should instruct the AI to:
   - Use `curl` to fetch Google News RSS with the encoded Korean query
   - Extract and list the top headlines with sources
   - Provide a 3-5 sentence briefing summary
   - Suggest 2-3 follow-up story angles
5. Add to help text in the journalism section
Issue: none

### Issue Responses
(No community issues today.)
