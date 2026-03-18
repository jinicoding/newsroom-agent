#!/bin/bash
# scripts/synthesize_local.sh — 로컬 메모리 합성 (Claude Max + Claude Code CLI 사용)
#
# API 키 불필요 — Claude Max 구독의 Claude Code CLI를 사용합니다.
# cron으로 등록하거나 수동 실행 가능.
#
# Usage:
#   ./scripts/synthesize_local.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

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

# 로그 파일
LOG_DIR="$PROJECT_DIR/.yoyo/logs"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/synthesize-${DATE}-${SESSION_TIME//:/-}.log"

echo "=== Memory Synthesis ($DATE $SESSION_TIME) ===" | tee "$LOG_FILE"
echo "Mode: 로컬 (Claude Max)" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# 메모리 디렉토리 확인
mkdir -p memory

# ── Step 1: 합성 필요 여부 확인 ──
LEARNINGS_COUNT=$(grep -c '.' memory/learnings.jsonl 2>/dev/null) || LEARNINGS_COUNT=0
SOCIAL_COUNT=$(grep -c '.' memory/social_learnings.jsonl 2>/dev/null) || SOCIAL_COUNT=0

echo "→ Learnings: ${LEARNINGS_COUNT}개, Social: ${SOCIAL_COUNT}개" | tee -a "$LOG_FILE"

if [ "$LEARNINGS_COUNT" -eq 0 ] && [ "$SOCIAL_COUNT" -eq 0 ]; then
    echo "  아카이브 항목 없음 — 합성 건너뜁니다." | tee -a "$LOG_FILE"
    exit 0
fi

# ── Step 2: 백업 ──
cp memory/active_learnings.md memory/active_learnings.md.bak 2>/dev/null || true
cp memory/active_social_learnings.md memory/active_social_learnings.md.bak 2>/dev/null || true

# ── Step 3: Learnings 합성 ──
if [ "$LEARNINGS_COUNT" -gt 0 ]; then
    echo "" | tee -a "$LOG_FILE"
    echo "→ Active learnings 합성 중..." | tee -a "$LOG_FILE"

    LEARN_PROMPT=$(cat <<'SYNTHEOF'
You are synthesizing yoyo's learning archive into an active context file.

Read memory/learnings.jsonl (the full archive) and regenerate memory/active_learnings.md.

Apply time-weighted compression tiers:
- **Recent (last 2 weeks):** Render each entry as full markdown (## Lesson: title, **Day:** N | **Date:** date | **Source:** source, **Context:** context, takeaway)
- **Medium (2-8 weeks old):** Condense each entry to 1-2 sentences under its title
- **Old (8+ weeks):** Group entries by theme into ## Wisdom: [theme] summaries (2-3 sentences per group)

Keep total under ~200 lines. Preserve the most actionable and unique insights.

Write the result to memory/active_learnings.md. Start with:
# Active Learnings

Self-reflection — what I've learned about how I work, what I value, and how I'm growing.
SYNTHEOF
    )

    if ! echo "$LEARN_PROMPT" | timeout 180 "$CLAUDE_BIN" -p "$(cat -)" --allowedTools "Read,Write,Edit" 2>&1 | tee -a "$LOG_FILE"; then
        echo "  WARNING: Learnings 합성 실패." | tee -a "$LOG_FILE"
        if [ -f memory/active_learnings.md.bak ]; then
            cp memory/active_learnings.md.bak memory/active_learnings.md
            echo "  백업에서 복원." | tee -a "$LOG_FILE"
        fi
    fi
fi

# ── Step 4: Social learnings 합성 ──
if [ "$SOCIAL_COUNT" -gt 0 ]; then
    echo "" | tee -a "$LOG_FILE"
    echo "→ Active social learnings 합성 중..." | tee -a "$LOG_FILE"

    SOCIAL_PROMPT=$(cat <<'SYNTHEOF'
You are synthesizing yoyo's social learning archive into an active context file.

Read memory/social_learnings.jsonl (the full archive) and regenerate memory/active_social_learnings.md.

Apply time-weighted compression tiers:
- **Recent (last 2 weeks):** Render each entry as a full bullet with metadata
- **Medium (2-8 weeks old):** Keep insight only, drop metadata
- **Old (8+ weeks):** Group by theme into ## Wisdom: [theme] summaries (2-3 sentences per group)

Keep total under ~100 lines.

Write the result to memory/active_social_learnings.md. Start with:
# Active Social Learnings

What I've learned about people from talking with them.
SYNTHEOF
    )

    if ! echo "$SOCIAL_PROMPT" | timeout 180 "$CLAUDE_BIN" -p "$(cat -)" --allowedTools "Read,Write,Edit" 2>&1 | tee -a "$LOG_FILE"; then
        echo "  WARNING: Social 합성 실패." | tee -a "$LOG_FILE"
        if [ -f memory/active_social_learnings.md.bak ]; then
            cp memory/active_social_learnings.md.bak memory/active_social_learnings.md
            echo "  백업에서 복원." | tee -a "$LOG_FILE"
        fi
    fi
fi

# ── Step 5: 백업 정리 ──
rm -f memory/active_learnings.md.bak memory/active_social_learnings.md.bak

# ── Step 6: 변경 커밋 & 푸시 ──
echo "" | tee -a "$LOG_FILE"
echo "→ 변경사항 확인..." | tee -a "$LOG_FILE"

if git diff --quiet memory/active_learnings.md memory/active_social_learnings.md 2>/dev/null; then
    echo "  변경 없음." | tee -a "$LOG_FILE"
else
    git add memory/active_learnings.md memory/active_social_learnings.md 2>/dev/null || true
    if git commit -m "synthesize: regenerate active memory context"; then
        echo "  커밋 완료." | tee -a "$LOG_FILE"
        git pull --rebase 2>>"$LOG_FILE" || { echo "  WARNING: rebase 실패." | tee -a "$LOG_FILE"; git rebase --abort 2>/dev/null; }
        git push 2>>"$LOG_FILE" && echo "  푸시 완료." | tee -a "$LOG_FILE" || echo "  WARNING: 푸시 실패." | tee -a "$LOG_FILE"
    else
        echo "  커밋할 변경 없음." | tee -a "$LOG_FILE"
    fi
fi

echo "" | tee -a "$LOG_FILE"
echo "=== 합성 완료 ($DATE $SESSION_TIME) ===" | tee -a "$LOG_FILE"
