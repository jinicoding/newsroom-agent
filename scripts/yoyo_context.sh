#!/bin/bash
# scripts/yoyo_context.sh — Build yoyo's identity context for prompts.
# Source this file, then use $YOYO_CONTEXT in any prompt.
#
# Usage:
#   YOYO_REPO="/path/to/yoyo-evolve" source scripts/yoyo_context.sh
#   cat > prompt.txt <<EOF
#   $YOYO_CONTEXT
#   ... your task-specific instructions ...
#   EOF
#
# Reads: IDENTITY.md, PERSONALITY.md, SOCIAL_LEARNINGS.md
# These are yoyo's stable identity files — who it is, how it speaks,
# and what it's learned from talking with humans.

_YOYO_REPO="${YOYO_REPO:-.}"

_IDENTITY=""
if [ -f "$_YOYO_REPO/IDENTITY.md" ]; then
    _IDENTITY=$(cat "$_YOYO_REPO/IDENTITY.md") || {
        echo "WARNING: Failed to read IDENTITY.md" >&2
        _IDENTITY=""
    }
else
    echo "WARNING: IDENTITY.md not found at $_YOYO_REPO/IDENTITY.md" >&2
fi

_PERSONALITY=""
if [ -f "$_YOYO_REPO/PERSONALITY.md" ]; then
    _PERSONALITY=$(cat "$_YOYO_REPO/PERSONALITY.md") || {
        echo "WARNING: Failed to read PERSONALITY.md" >&2
        _PERSONALITY=""
    }
else
    echo "WARNING: PERSONALITY.md not found at $_YOYO_REPO/PERSONALITY.md" >&2
fi

# SOCIAL_LEARNINGS.md is optional — no warning if missing
_SOCIAL_LEARNINGS=""
if [ -f "$_YOYO_REPO/SOCIAL_LEARNINGS.md" ]; then
    _SOCIAL_LEARNINGS=$(cat "$_YOYO_REPO/SOCIAL_LEARNINGS.md") || _SOCIAL_LEARNINGS=""
fi

YOYO_CONTEXT="=== WHO YOU ARE ===

${_IDENTITY:-Read IDENTITY.md for your rules and constitution.}

=== YOUR VOICE ===

${_PERSONALITY:-Read PERSONALITY.md for your voice and values.}

=== SOCIAL WISDOM ===

${_SOCIAL_LEARNINGS:-No social learnings yet.}"
