#!/usr/bin/env bash
# PreToolUse guard (AGENTS.md "Human checkpoints").
# - Internet writes are human-only: push, publish, PR/issue/comment, uploads, non-GET requests,
#   publishing artifacts, MCP tools that create/update/send.
# - Commits and merges on main are human-only.
input=$(cat)
tool=$(printf '%s' "$input" | jq -r '.tool_name // ""')

deny() {
  jq -cn --arg r "$1" '{hookSpecificOutput:{hookEventName:"PreToolUse",permissionDecision:"deny",permissionDecisionReason:$r}}'
  exit 0
}

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
    case "$cmd" in
      *"git commit"*|*"git merge"*|*"git cherry-pick"*|*"git rebase"*)
        cwd=$(printf '%s' "$input" | jq -r '.cwd // "."')
        if [ "$(git -C "$cwd" branch --show-current 2>/dev/null)" = "main" ]; then
          deny "On main: commits and merges to main are human-only (H3). Create a branch first."
        fi
        ;;
    esac
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
