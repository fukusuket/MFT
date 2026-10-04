#!/usr/bin/env bash
# PreToolUse guard (AGENTS.md "Human checkpoints", ADR 0007).
# - Internet writes are human-only: push, publish, PR/issue/comment, uploads, non-GET requests,
#   publishing artifacts, MCP tools that create/update/send.
# - Commits and merges on main are human-only.
# - Commits must go through the pre-commit scan (.githooks): hooks enabled, no --no-verify.
# - Edits to agent/CI/security config need human confirmation.
input=$(cat)
tool=$(printf '%s' "$input" | jq -r '.tool_name // ""')
cwd=$(printf '%s' "$input" | jq -r '.cwd // "."')

decide() {
  jq -cn --arg d "$1" --arg r "$2" '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:$d,permissionDecisionReason:$r}}'
  exit 0
}
deny() { decide deny "$1"; }
ask() { decide ask "$1"; }

# Agent instructions, hooks, CI and security config (ADR 0007 P6).
protected='(^|/|[[:space:]"'"'"'])(\.claude/|AGENTS\.md|CLAUDE\.md|\.github/|\.githooks/|deny\.toml|\.gitleaks\.toml)'
protected_msg="Changes to agent, CI or security config (.claude/, AGENTS.md, CLAUDE.md, .github/, .githooks/, deny.toml, .gitleaks.toml) need human confirmation (ADR 0007 P6)."

case "$tool" in
  Bash)
    cmd=$(printf '%s' "$input" | jq -r '.tool_input.command // ""')
    # Ignore quoted text (commit messages, echo) unless it is a script run via `sh -c`/`eval`.
    scan="$cmd"
    if ! printf '%s' "$cmd" | grep -Eq '(^|[;&|(` ])((ba|z)?sh|eval)[[:space:]]+(-[a-z]*c|[^-])'; then
      scan=$(printf '%s' "$cmd" | perl -0777 -pe "s/'[^']*'//g; s/\"(?:\\\\.|[^\"\\\\])*\"//g")
    fi
    net_write='(^|[;&|(`"'"'"' ])(git[[:space:]]+push|cargo[[:space:]]+(publish|yank|owner|login)|(npm|pnpm|yarn)[[:space:]]+(publish|unpublish)|twine[[:space:]]+upload|docker[[:space:]]+push|scp|sftp)([[:space:]`;&|)"'"'"']|$)'
    gh_write='(^|[;&|(`"'"'"' ])gh[[:space:]]+[a-z-]+[[:space:]]+(create|merge|comment|close|reopen|edit|delete|review|ready|upload|run|sync|fork|transfer|archive)([[:space:]`;&|)"'"'"']|$)'
    gh_api_write='(^|[;&|(`"'"'"' ])gh[[:space:]]+api[[:space:]].*(-X|--method|-f[[:space:]]|-F[[:space:]]|--field|--raw-field|--input)'
    http_write='(^|[;&|(`"'"'"' ])(curl|wget)[[:space:]].*(-X[[:space:]]*(POST|PUT|PATCH|DELETE)|--request[[:space:]]+(POST|PUT|PATCH|DELETE)|-d[[:space:]]|--data|-F[[:space:]]|--form|-T[[:space:]]|--upload-file|--json|--post-data|--post-file|--method=(POST|PUT|PATCH|DELETE))'
    rsync_remote='(^|[;&|(`"'"'"' ])rsync[[:space:]].*[^[:space:]]+:'
    if printf '%s' "$scan" | grep -Eq "$net_write|$gh_write|$gh_api_write|$http_write|$rsync_remote"; then
      deny "Internet writes (push, publish, PR/issue/comment, upload, non-GET requests) are human-only. Prepare the change locally and hand it over."
    fi

    # Hooks must not be disabled or redirected.
    if printf '%s' "$scan" | grep -Eq -- '--no-verify' ||
       { printf '%s' "$scan" | grep -Eq 'core\.hooksPath' && ! printf '%s' "$scan" | grep -Eq 'config[[:space:]]+(--get|--get-all|--list|-l)[[:space:]]'; }; then
      deny "Changing core.hooksPath or using --no-verify bypasses the commit scan (ADR 0007 P1). Ask a human."
    fi
    if printf '%s' "$scan" | grep -Eq '(^|[;&|(` ])git[[:space:]]+commit([[:space:]][^;&|]*)?[[:space:]]-[a-zA-Z]*n[a-zA-Z]*([[:space:]]|$)'; then
      deny "git commit -n skips the pre-commit scan (ADR 0007 P1)."
    fi

    case "$scan" in
      *"git commit"*|*"git merge"*|*"git cherry-pick"*|*"git rebase"*)
        if [ "$(git -C "$cwd" branch --show-current 2>/dev/null)" = "main" ]; then
          deny "On main: commits and merges to main are human-only (H3). Create a branch first."
        fi
        if [ "$(git -C "$cwd" config --get core.hooksPath 2>/dev/null)" != ".githooks" ]; then
          deny "Pre-commit scan is not enabled. A human must run: git config core.hooksPath .githooks (docs/security.md)."
        fi
        ;;
    esac

    # Shell writes to protected config (sed -i, redirects, mv/cp/rm/tee, scripts).
    if printf '%s' "$cmd" | grep -Eq "$protected" &&
       printf '%s' "$cmd" | grep -Eq '(sed[[:space:]]+-[a-zA-Z]*i|perl[[:space:]]+-[a-zA-Z]*i|(^|[^0-9&>])>|tee[[:space:]]|(^|[;&|[:space:]])(mv|cp|rm|chmod|ln|install|python3?|ruby|node|jq)[[:space:]])'; then
      ask "$protected_msg"
    fi
    ;;
  Edit|Write|NotebookEdit)
    path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // .tool_input.notebook_path // ""')
    if printf '%s' "$path" | grep -Eq "$protected"; then
      ask "$protected_msg"
    fi
    ;;
  Artifact)
    action=$(printf '%s' "$input" | jq -r '.tool_input.action // "publish"')
    case "$action" in
      read|list|open|quickstart) ;;
      *) deny "Publishing or changing artifacts is an internet write and human-only." ;;
    esac
    ;;
  mcp__*)
    if printf '%s' "$tool" | grep -Eiq '(create|update|delete|batch|send|publish|post|upload|write|edit|comment|share|move|copy|reply|draft|label|archive)'; then
      deny "MCP tools that write to external services are human-only."
    fi
    ;;
esac
exit 0
