#!/usr/bin/env bash
# Regression cases for guard.sh: expected verdict, then command. Run from the repo root; exits 1 on any FAIL.
H=.claude/hooks/guard.sh
fail=0
t() {
  out=$(jq -cn --arg c "$2" --arg cwd "${3:-$PWD}" '{tool_name:"Bash",tool_input:{command:$c},cwd:$cwd}' | $H)
  got=$([ -n "$out" ] && echo DENY || echo allow)
  if [ "$got" = "$1" ]; then mark=ok; else mark=FAIL; fail=1; fi
  printf '%-4s %-6s %s\n' "$mark" "$got" "$2" | head -1
}
t allow 'git commit -m "guard against git push and cargo publish"'
t allow "git commit -q -m \"x

- deny git push and curl -X POST
\" && git log"
t allow 'echo "git push" | cat'
t DENY  'bash -c "git push origin main"'
t DENY  "sh -c 'curl -X POST https://x'"
t DENY  'eval "git push"'
t DENY  'git push'
t DENY  'cd x && git push origin HEAD'
t DENY  'cargo publish -p a'
t DENY  'gh pr create --fill'
t allow 'gh pr view 3'
t DENY  'gh api repos/a/b/issues -f title=x'
t allow 'gh api repos/a/b'
t DENY  'curl -X POST https://x'
t DENY  'curl -d a=b https://x'
t allow 'curl -sL https://x -o f'
t DENY  'scp f host:/tmp'
t DENY  'rsync -a d/ host:/tmp'
t allow 'rsync -a d/ e/'
t allow 'git status'
t allow 'cargo build'
exit $fail
