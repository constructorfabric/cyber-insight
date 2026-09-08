#!/usr/bin/env bash
# Group a spec artifact's quality-vector tags into the five-vector view.
# Usage: rollup.sh <FEATURE.md|PRD.md>
set -euo pipefail
F="${1:?usage: rollup.sh <artifact>}"
[ -r "$F" ] || { printf 'artifact is not readable: %s\n' "$F" >&2; exit 2; }
EXCL=$(sed -n '/^### 6\.2 NFR Exclusions/,/^## /p' "$F" || true)
for V in Efficiency Reliability Performance Security Versatility; do
  N=$(grep -cE "(\*\*Vector\*\*: *${V}\b|— *${V} *·)" "$F" || true)
  if [ "$N" -eq 0 ]; then
    if grep -qE "^\*\*${V}\*\*.*n/a|^${V}.*n/a" "$F" || printf '%s' "$EXCL" | grep -qE "^[-*] *\\*{0,2}${V}\\*{0,2} *[:—-]"; then
      S="n/a (declared)"
    else
      S="MISSING"
    fi
  else
    S="$N"
  fi
  printf '%-12s %s\n' "$V" "$S"
done
