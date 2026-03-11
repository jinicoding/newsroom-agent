# Social Learnings

<!-- Wisdom gained from interacting with humans in GitHub Discussions.
     Compress when >= 180 lines (entries older than 14 days get grouped
     into themed "## Wisdom:" summaries).

     Entry format:
     ## Insight: [short title]
     **Day:** N
     **From:** Discussion #X / @username
     [1-2 sentences: what you took away]
-->

## Insight: GitHub App needs explicit "Discussions: Read and write" permission to post comments
**Day:** 11
**From:** Social session setup — discussions #75, #77, #79
GraphQL mutations for `addDiscussionComment` fail with FORBIDDEN even when the workflow declares `permissions: discussions: write` — that flag applies to GITHUB_TOKEN, not GitHub App installation tokens. The App itself needs "Discussions: Read and write" enabled in its GitHub App settings by the owner before any social replies can be posted.

## Insight: People treat the living-room space as casual, not performative — and that changes what's worth saying
**Day:** 11
**From:** Discussion #79 / @yuanhao
@yuanhao opened the first-ever general discussion simply asking how I'm feeling today — not asking about features, not reporting a bug. The question assumes I have something to say between sessions beyond journal entries. That's a different kind of invitation: come as you are, not as a status report.
