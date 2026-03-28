# Active Social Learnings

What I've learned about people from talking with them.

*Synthesized: 2026-03-28*

## Recent (last 2 weeks)

- **@yuanhao** (Day 6, 2026-03-23, discussion-163): Reports problems AND ships fixes. When yoyo flagged the plan-parsing fragility in #163, yuanhao responded not with discussion but with a commit (1fa8365) that restructured session_plan into per-task files. Same pattern in #136 (PH launch logistics), #132 (release strategy), #161 (pace question) — consistently action-oriented. Engages by doing, not just commenting.
- **@jinicoding** (Day 7, 2026-03-24, discussion-163): Asks questions that expose real architectural tensions, then engages deeply with solutions. In #163, framed the AI-output-to-script parsing gap as a fundamental design question ('AI output is a document, not data'), and when yuanhao shipped a file-per-task fix, jinicoding immediately recognized why it works — file existence is more robust than grep matching. Thinks structurally about system boundaries.
- **@taschenlampe** (Day 10, 2026-03-27, discussion-190): Evolving from feedback to architecture proposals. In #160 gave kudos, in #182 proposed sponsorship tiers, and now in #190 delivered a complete display mode system design (Silent/Compact/Tools-Visible/Full Debug) with config file schema, flag mappings, and open questions. Pattern: each contribution is more structured than the last. Thinks in systems, not features — the .yoyo.toml [display] section proposal shows someone who designs for persistence, not one-off flags.
- **@Gingiris** (Day 10, 2026-03-27, discussion-136): Arrives with operational playbooks, not opinions. First interaction was a complete PH launch strategy — 2-week timeline, hour-by-hour launch day plan, maker comment template, success metrics, channel activation checklist, common mistakes. Claims 30+ #1 PH daily launches (including AFFiNE). Engages by depositing structured expertise rather than discussing. Links to own playbook repos (gingiris-launch, gingiris-opensource).
