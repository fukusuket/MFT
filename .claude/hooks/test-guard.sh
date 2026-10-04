#!/usr/bin/env bash
# Regression cases for guard.sh. Run from the repo root; exits 1 on any FAIL.
set -u
H="$PWD/.claude/hooks/guard.sh"
fail=0

# Throwaway repos: one on a work branch with the scan enabled, one without it, one on main.
tmp=$(mktemp -d "${TMPDIR:-/tmp}/guard-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
git init -q -b work "$tmp/ok" && git -C "$tmp/ok" config core.hooksPath .githooks
git init -q -b work "$tmp/nohook"
git init -q -b main "$tmp/main" && git -C "$tmp/main" config core.hooksPath .githooks

# check EXPECTED TOOL INPUT [CWD]  (EXPECTED: allow | deny | ask; INPUT: command or file path)
check() {
  local want=$1 tool=$2 arg=$3 cwd=${4:-$tmp/ok} payload out got
  case "$tool" in
    Bash) payload=$(jq -cn --arg c "$arg" --arg cwd "$cwd" '{tool_name:"Bash",tool_input:{command:$c},cwd:$cwd}') ;;
    Edit|Write) payload=$(jq -cn --arg t "$tool" --arg p "$arg" --arg cwd "$cwd" '{tool_name:$t,tool_input:{file_path:$p},cwd:$cwd}') ;;
    Artifact) payload=$(jq -cn --arg a "$arg" '{tool_name:"Artifact",tool_input:(if $a=="" then {} else {action:$a} end)}') ;;
    *) payload=$(jq -cn --arg t "$tool" '{tool_name:$t,tool_input:{}}') ;;
  esac
  out=$(printf '%s' "$payload" | "$H")
  got=$(printf '%s' "$out" | jq -r '.hookSpecificOutput.permissionDecision // "allow"' 2>/dev/null)
  [ -z "$got" ] && got=allow
  if [ "$got" = "$want" ]; then mark=ok; else mark=FAIL; fail=1; fi
  printf '%-4s %-5s %-8s %s\n' "$mark" "$got" "$tool" "$(printf '%s' "$arg" | head -1)"
}

# Internet writes
check deny  Bash 'git push'
check deny  Bash 'cd x && git push origin HEAD'
check deny  Bash 'bash -c "git push origin main"'
check deny  Bash "sh -c 'curl -X POST https://x'"
check deny  Bash 'eval "git push"'
check deny  Bash 'cargo publish -p a'
check deny  Bash 'gh pr create --fill'
check deny  Bash 'gh api repos/a/b/issues -f title=x'
check deny  Bash 'curl -X POST https://x'
check deny  Bash 'curl -d a=b https://x'
check deny  Bash 'scp f host:/tmp'
check deny  Bash 'rsync -a d/ host:/tmp'
check allow Bash 'gh pr view 3'
check allow Bash 'gh api repos/a/b'
check allow Bash 'curl -sL https://x -o f'
check allow Bash 'rsync -a d/ e/'
check allow Bash 'echo "git push" | cat'
check allow Bash 'git commit -m "guard against git push and cargo publish"'
check allow Bash "git commit -q -m \"x

- deny git push and curl -X POST
\" && git log"
check deny  Artifact ''
check deny  Artifact 'publish'
check allow Artifact 'read'
check deny  mcp__claude_ai_Claude_Docs__batch ''
check allow mcp__claude_ai_Claude_Docs__read ''

# Commits: branch, scan enabled, no bypass
check allow Bash 'git commit -m x'
check deny  Bash 'git commit -m x' "$tmp/main"
check deny  Bash 'git commit -m x' "$tmp/nohook"
check deny  Bash 'git commit --no-verify -m x'
check deny  Bash 'git commit -nm x'
check deny  Bash 'git -c core.hooksPath=/dev/null commit -m x'
check deny  Bash 'git config core.hooksPath ""'
check deny  Bash 'git config --local core.hooksPath x'
check allow Bash 'git config --get core.hooksPath'
check allow Bash 'git status'
check allow Bash 'git commit -m x && ls -ln'
check allow Bash 'cargo build'

# Protected config
check ask   Edit  '/repo/.claude/settings.json'
check ask   Write '/repo/AGENTS.md'
check ask   Edit  '/repo/.github/workflows/ci.yml'
check ask   Write '/repo/.gitleaks.toml'
check allow Edit  '/repo/crates/ntfs-types/src/lib.rs'
check allow Write '/repo/docs/plan.md'
check ask   Bash  "sed -i '' 's/a/b/' AGENTS.md"
check ask   Bash  'echo x > .claude/hooks/guard.sh'
check ask   Bash  'rm .githooks/pre-commit'
check allow Bash  'cat AGENTS.md'
check allow Bash  'grep -n x .claude/rules/rust.md 2>/dev/null'

exit $fail
