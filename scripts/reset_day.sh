#!/bin/bash
# scripts/reset_day.sh — Reset the day counter after a failed evolution run.
#
# Usage:
#   ./scripts/reset_day.sh        # decrement by 1
#   ./scripts/reset_day.sh 5      # set to specific day

set -euo pipefail

CURRENT=$(cat DAY_COUNT 2>/dev/null || echo 1)

if [ -n "${1:-}" ]; then
    NEW="$1"
else
    NEW=$((CURRENT - 1))
    if [ "$NEW" -lt 0 ]; then
        NEW=0
    fi
fi

echo "$NEW" > DAY_COUNT
python3 scripts/build_site.py 2>/dev/null || true
echo "DAY_COUNT: $CURRENT → $NEW"
