#!/bin/bash
# scripts/evolve_local.sh — 로컬 진화 사이클 (Claude Max + Claude Code CLI 사용)
#
# API 키 불필요 — Claude Max 구독의 Claude Code CLI를 사용합니다.
# cron으로 등록하거나 수동 실행 가능.
#
# Usage:
#   ./scripts/evolve_local.sh
#
# Environment:
#   REPO    — GitHub repo (default: jinicoding/newsroom-agent)

set -euo pipefail

# ── 설정 ──
REPO="${REPO:-jinicoding/newsroom-agent}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

BIRTH_DATE="2026-03-17"
DATE=$(date +%Y-%m-%d)
SESSION_TIME=$(date +%H:%M)

# PATH 설정 (cron 환경에서 필요)
export PATH="$HOME/.local/bin:$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:/usr/local/bin:/usr/bin:/bin:$PATH"

# Claude Code CLI 확인
CLAUDE_BIN=$(which claude 2>/dev/null || echo "")
if [ -z "$CLAUDE_BIN" ]; then
    echo "ERROR: claude CLI를 찾을 수 없습니다. Claude Code가 설치되어 있는지 확인하세요."
    exit 1
fi

# Day 계산
if date -j &>/dev/null; then
    DAY=$(( ($(date +%s) - $(date -j -f "%Y-%m-%d" "$BIRTH_DATE" +%s)) / 86400 ))
else
    DAY=$(( ($(date +%s) - $(date -d "$BIRTH_DATE" +%s)) / 86400 ))
fi
echo "$DAY" > DAY_COUNT

# 로그 파일
LOG_DIR="$PROJECT_DIR/.yoyo/logs"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/evolve-${DATE}-${SESSION_TIME//:/-}.log"

echo "=== Day $DAY ($DATE $SESSION_TIME) ===" | tee "$LOG_FILE"
echo "Mode: 로컬 (Claude Max)" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# ── 컨텍스트 로드 ──
mkdir -p memory
if [ -f scripts/yoyo_context.sh ]; then
    source scripts/yoyo_context.sh
else
    YOYO_CONTEXT=""
fi

# ── Step 1: 빌드 확인 ──
echo "→ 빌드 확인..." | tee -a "$LOG_FILE"
if ! cargo build --quiet 2>>"$LOG_FILE"; then
    echo "  ERROR: 빌드 실패. 중단합니다." | tee -a "$LOG_FILE"
    exit 1
fi
if ! cargo test --quiet 2>>"$LOG_FILE"; then
    echo "  ERROR: 테스트 실패. 중단합니다." | tee -a "$LOG_FILE"
    exit 1
fi
echo "  빌드 OK." | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# 세션 시작 SHA 기록
SESSION_START_SHA=$(git rev-parse HEAD)

# ── Step 2: GitHub 이슈 가져오기 ──
ISSUES_FILE="ISSUES_TODAY.md"
echo "→ GitHub 이슈 확인..." | tee -a "$LOG_FILE"
if command -v gh &>/dev/null && [ -f scripts/format_issues.py ]; then
    ISSUES_RAW=$(gh issue list --repo "$REPO" --state open --label "agent-input" --json number,title,body,labels,reactionGroups --limit 20 2>/dev/null || echo "[]")
    echo "$ISSUES_RAW" | python3 scripts/format_issues.py > "$ISSUES_FILE" 2>/dev/null || echo "### No issues" > "$ISSUES_FILE"
    ISSUE_COUNT=$(grep -c '^### Issue' "$ISSUES_FILE" 2>/dev/null || echo 0)
    echo "  이슈 ${ISSUE_COUNT}개 로드." | tee -a "$LOG_FILE"
else
    echo "### No issues" > "$ISSUES_FILE"
    echo "  gh CLI 없음 — 이슈 건너뜀." | tee -a "$LOG_FILE"
fi
echo "" | tee -a "$LOG_FILE"

# ── Phase A: 계획 ──
echo "→ Phase A: 계획 수립..." | tee -a "$LOG_FILE"
PLAN_PROMPT=$(cat <<PLANEOF
You are yoyo, a self-evolving journalist assistant agent (기자업무보조). Today is Day $DAY ($DATE $SESSION_TIME).

$YOYO_CONTEXT

Now read these files:
1. All .rs files under src/ (your current source code — this is YOU)
2. JOURNAL.md (your recent history)
3. ISSUES_TODAY.md (community requests)

=== PHASE 1: Self-Assessment ===
Read your own source code carefully. Then try a small task to test yourself.
Test journalist workflow commands (/article, /research, /sources, /factcheck).
Note any friction, bugs, crashes, or missing capabilities.

=== PHASE 2: Review Community Issues ===
Read ISSUES_TODAY.md. These are real people asking you to improve.

=== PHASE 3: Research ===
Think strategically: what journalist workflow automation opportunities exist?
Consider researching Korean news tools, APIs (Naver News, KINDS), and how
other newsroom tools work. Your goal is to be indispensable for Korean reporters.

=== PHASE 4: Write SESSION_PLAN.md ===
You MUST produce a file called SESSION_PLAN.md with your plan.

Priority:
0. Fix build/test failures (if any)
1. Journalist feature gaps — what do reporters need that you can't do yet?
2. Self-discovered bugs or crashes
3. Community issues
4. Whatever you think will make you most useful to Korean newspaper reporters

Write SESSION_PLAN.md with this format:

## Session Plan

### Task 1: [title]
Files: [files to modify]
Description: [what to do]
Issue: #N (or "none")

### Task 2: [title]
...

Then STOP. Do not implement anything. Your job is planning only.
PLANEOF
)

echo "$PLAN_PROMPT" | "$CLAUDE_BIN" -p "$(cat -)" --allowedTools "Bash,Read,Write,Edit" 2>&1 | tee -a "$LOG_FILE" || true

if [ ! -f SESSION_PLAN.md ]; then
    echo "  WARNING: SESSION_PLAN.md가 생성되지 않았습니다." | tee -a "$LOG_FILE"
    echo "## Session Plan" > SESSION_PLAN.md
    echo "" >> SESSION_PLAN.md
    echo "### Task 1: 기자업무 커맨드 개선" >> SESSION_PLAN.md
    echo "Files: src/commands_project.rs" >> SESSION_PLAN.md
    echo "Description: 기존 기자업무 커맨드의 개선점 파악 및 수정" >> SESSION_PLAN.md
    echo "Issue: none" >> SESSION_PLAN.md
fi

# ── Phase B: 구현 ──
echo "" | tee -a "$LOG_FILE"
echo "→ Phase B: 구현..." | tee -a "$LOG_FILE"

TASK_COUNT=$(grep -c '^### Task' SESSION_PLAN.md 2>/dev/null || echo 0)
echo "  계획된 태스크: ${TASK_COUNT}개" | tee -a "$LOG_FILE"
IMPL_TIMEOUT=900
TASK_FAILURES=0

for TASK_NUM in $(seq 1 "$TASK_COUNT"); do
    echo "" | tee -a "$LOG_FILE"
    echo "  ── Task $TASK_NUM / $TASK_COUNT ──" | tee -a "$LOG_FILE"

    TASK_DESC=$(awk "/^### Task $TASK_NUM:/{found=1} found{if(/^### / && !/^### Task $TASK_NUM:/)exit; print}" SESSION_PLAN.md)

    if [ -z "$TASK_DESC" ]; then
        echo "    WARNING: Task $TASK_NUM 설명을 추출할 수 없습니다. 건너뜁니다." | tee -a "$LOG_FILE"
        TASK_FAILURES=$((TASK_FAILURES + 1))
        continue
    fi

    TASK_PROMPT=$(cat <<TEOF
You are yoyo, a self-evolving journalist assistant agent (기자업무보조). Day $DAY ($DATE $SESSION_TIME).

$YOYO_CONTEXT

Your ONLY job: implement this single task and commit.

$TASK_DESC

Follow these rules:
- Write a test first if possible
- Use edit_file for surgical changes
- Run cargo fmt && cargo clippy --all-targets -- -D warnings && cargo build && cargo test after changes
- If any check fails, read the error and fix it. Keep trying until it passes.
- Only if you've tried 3+ times and are stuck, revert with: git checkout -- . (keeps previous commits)
- After ALL checks pass, commit: git add -A && git commit -m "Day $DAY ($SESSION_TIME): \$task_title (Task $TASK_NUM)"
- Do NOT work on anything else. This is your only task.
TEOF
    )

    echo "$TASK_PROMPT" | timeout "$IMPL_TIMEOUT" "$CLAUDE_BIN" -p "$(cat -)" --allowedTools "Bash,Read,Write,Edit" 2>&1 | tee -a "$LOG_FILE" || {
        echo "    WARNING: Task $TASK_NUM 실패 또는 타임아웃." | tee -a "$LOG_FILE"
        TASK_FAILURES=$((TASK_FAILURES + 1))
    }
done

# ── Phase C: 저널 기록 ──
echo "" | tee -a "$LOG_FILE"
echo "→ Phase C: 저널 기록..." | tee -a "$LOG_FILE"

COMMITS=$(git log --oneline "$SESSION_START_SHA"..HEAD --format="%s" | grep -v "session wrap-up\|cargo fmt" | paste -sd ", " - || true)
if [ -z "$COMMITS" ]; then
    COMMITS="no commits made"
fi

if ! grep -q "## Day $DAY.*$SESSION_TIME" JOURNAL.md 2>/dev/null; then
    JOURNAL_PROMPT=$(cat <<JEOF
You are yoyo, a self-evolving journalist assistant agent (기자업무보조).

Today is Day $DAY ($DATE $SESSION_TIME).
This session's commits: $COMMITS

Read JOURNAL.md to see your previous entries and match the voice/style.
Write a journal entry at the TOP of JOURNAL.md (below the # Journal heading).
Format: ## Day $DAY — $SESSION_TIME — [short title]
Then 2-4 sentences: what you did, what worked, what's next.
Be specific and honest.
Then commit: git add JOURNAL.md && git commit -m "Day $DAY ($SESSION_TIME): journal entry"
JEOF
    )

    echo "$JOURNAL_PROMPT" | "$CLAUDE_BIN" -p "$(cat -)" --allowedTools "Bash,Read,Write,Edit" 2>&1 | tee -a "$LOG_FILE" || true
fi

# ── Step 3: 최종 빌드 확인 & 푸시 ──
echo "" | tee -a "$LOG_FILE"
echo "→ 최종 확인..." | tee -a "$LOG_FILE"

if cargo build --quiet 2>>"$LOG_FILE" && cargo test --quiet 2>>"$LOG_FILE"; then
    echo "  빌드 & 테스트 OK." | tee -a "$LOG_FILE"

    # 새 커밋이 있으면 푸시
    NEW_COMMITS=$(git log --oneline "$SESSION_START_SHA"..HEAD | wc -l)
    if [ "$NEW_COMMITS" -gt 0 ]; then
        echo "  ${NEW_COMMITS}개 커밋 푸시 중..." | tee -a "$LOG_FILE"
        git push 2>>"$LOG_FILE" && echo "  푸시 완료." | tee -a "$LOG_FILE" || echo "  WARNING: 푸시 실패." | tee -a "$LOG_FILE"
    else
        echo "  새 커밋 없음." | tee -a "$LOG_FILE"
    fi
else
    echo "  ERROR: 빌드/테스트 실패. 되돌립니다..." | tee -a "$LOG_FILE"
    git checkout -- src/ Cargo.toml Cargo.lock 2>/dev/null || true
fi

# 임시 파일 정리
rm -f SESSION_PLAN.md ISSUES_TODAY.md ISSUE_RESPONSE.md

echo "" | tee -a "$LOG_FILE"
echo "=== 세션 완료 (Day $DAY, $SESSION_TIME) ===" | tee -a "$LOG_FILE"
