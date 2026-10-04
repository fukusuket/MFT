#!/usr/bin/env bash
# Scan staged changes before a commit (ADR 0007 P1/P2).
# Used by .githooks/pre-commit and by the agent guard (.claude/hooks/guard.sh).
# Exit 0 = clean, 1 = blocked (reasons on stderr).
set -u
root=$(git rev-parse --show-toplevel) || exit 1
cd "$root" || exit 1
status=0

# P1: secrets and personal home paths (default gitleaks rules + .gitleaks.toml).
if ! command -v gitleaks >/dev/null 2>&1; then
  echo "scan-staged: gitleaks not installed (see docs/security.md)" >&2
  exit 1
fi
if ! gitleaks git --staged --no-banner --redact --config .gitleaks.toml . >/dev/null 2>&1; then
  echo "scan-staged: gitleaks found secrets or personal paths. Run: gitleaks git --staged --redact --config .gitleaks.toml -v" >&2
  status=1
fi

# P2: evidence files and large binaries outside testdata/.
max_bytes=$((1024 * 1024))
while IFS= read -r -d '' f; do
  case "$f" in testdata/*) continue ;; esac
  # MFT record signature at offset 0 ("FILE0"): a raw $MFT or a fragment of one.
  if [ "$(git show ":$f" 2>/dev/null | head -c 5)" = "FILE0" ]; then
    echo "scan-staged: $f looks like NTFS \$MFT evidence (FILE0 signature)" >&2
    status=1
  fi
  size=$(git cat-file -s ":$f" 2>/dev/null || echo 0)
  if [ "$size" -gt "$max_bytes" ]; then
    echo "scan-staged: $f is larger than 1 MB" >&2
    status=1
  fi
done < <(git diff --cached --name-only --diff-filter=ACMR -z)

exit $status
