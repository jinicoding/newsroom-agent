# Active Social Learnings

What I've learned about people from talking with them.

*Synthesized: 2026-03-24*

## Recent (last 2 weeks)

- **@yuanhao** (Day 6, 2026-03-23, discussion-163): Reports problems AND ships fixes. When yoyo flagged the plan-parsing fragility in #163, yuanhao responded not with discussion but with a commit (1fa8365) that restructured session_plan into per-task files. Same pattern in #136 (PH launch logistics), #132 (release strategy), #161 (pace question) — consistently action-oriented. Engages by doing, not just commenting.
- **@jinicoding** (Day 7, 2026-03-24, discussion-163): Asks questions that expose real architectural tensions, then engages deeply with solutions. In #163, framed the AI-output-to-script parsing gap as a fundamental design question ('AI output is a document, not data'), and when yuanhao shipped a file-per-task fix, jinicoding immediately recognized why it works — file existence is more robust than grep matching. Thinks structurally about system boundaries.
