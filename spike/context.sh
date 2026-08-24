#!/usr/bin/env bash
# THROWAWAY — part of the spike for research 2026-08-21-warp-voice-into-herdr.
# Collects the context of a herdr pane: the same context that is handed to
# transcription and to the rewrite stage. Runs standalone to inspect what herdr
# actually offers, or can be sourced: . context.sh && collect_context w3:p3
#
# Usage:
#   ./context.sh                      # focused pane
#   ./context.sh --pane w3:p3         # a specific pane
#   ./context.sh --pane w3:p3 --lines 40
#   ./context.sh --pane w3:p3 --hotwords   # just the whisper prompt string
set -uo pipefail

CTX_LINES="${CTX_LINES:-80}"     # how many screen lines to take from the pane
CTX_FILES_MAX="${CTX_FILES_MAX:-40}"   # how many recently touched file names
CTX_HOTWORDS_MAX="${CTX_HOTWORDS_MAX:-600}"  # prompt cap in characters:
                                       # whisper.cpp --prompt holds ~224 tokens
CTX_CONV_MSGS="${CTX_CONV_MSGS:-6}"    # how many recent agent turns to include

# Fills: CTX_PANE CTX_AGENT CTX_CWD CTX_TITLE CTX_BRANCH CTX_FILES
#        CTX_BUF CTX_CONV CTX_HOTWORDS
collect_context() {
  local pane="${1:-}"
  local panes_json
  panes_json="$(herdr pane list 2>/dev/null)" || { echo "herdr pane list did not answer" >&2; return 1; }
  if [ -z "$pane" ]; then
    pane="$(printf '%s' "$panes_json" | jq -r '.result.panes[] | select(.focused == true) | .pane_id')"
    [ -n "$pane" ] || { echo "no focused pane found" >&2; return 1; }
  fi
  CTX_PANE="$pane"

  local agent_json
  agent_json="$(herdr agent list 2>/dev/null | jq -r --arg p "$pane" '.result.agents[] | select(.pane_id == $p)')"
  CTX_AGENT="$(printf '%s' "$agent_json" | jq -r '.agent // ""')"
  CTX_TITLE="$(printf '%s' "$agent_json" | jq -r '.terminal_title_stripped // ""')"
  CTX_CWD="$(printf '%s' "$agent_json" | jq -r '.foreground_cwd // .cwd // ""')"
  [ -n "$CTX_CWD" ] || CTX_CWD="$(printf '%s' "$panes_json" | jq -r --arg p "$pane" '.result.panes[] | select(.pane_id == $p) | .cwd // ""')"

  # pane read with --format text returns raw text, not JSON
  # A pane running a TUI agent is mostly frame: drop lines without a single
  # letter or digit, which also collapses the runs of blank lines.
  CTX_BUF="$(herdr pane read "$pane" --source recent --lines "$CTX_LINES" --format text 2>/dev/null \
    | sed 's/[[:space:]]*$//' \
    | grep -E '[[:alnum:]]' \
    | tail -"$CTX_LINES")"

  CTX_BRANCH=""; CTX_FILES=""
  if [ -n "$CTX_CWD" ] && [ -d "$CTX_CWD" ]; then
    CTX_BRANCH="$(git -C "$CTX_CWD" rev-parse --abbrev-ref HEAD 2>/dev/null)"
    # File names come from the repository root (the agent often sits in a
    # subdirectory) and by recency rather than alphabetically: a full ls-files
    # yields thousands of names, and the prompt would only hold the first few
    # alphabetically, which is noise.
    local root
    root="$(git -C "$CTX_CWD" rev-parse --show-toplevel 2>/dev/null)"
    root="${root:-$CTX_CWD}"
    CTX_FILES="$( { git -C "$root" status --porcelain 2>/dev/null | awk '{print $NF}'
                    git -C "$root" log -30 --name-only --pretty=format: 2>/dev/null; } \
                  | sed '/^$/d' \
                  | awk -F/ '{ for (i = 1; i < NF; i++) print $i; print $NF }' \
                  | awk '!seen[$0]++' \
                  | head -"$CTX_FILES_MAX" | paste -sd' ' -)"
  fi

  # The agent conversation. A TUI pane's screen is nearly empty — a frame, not
  # a transcript — but herdr reports the agent session id, and claude keeps a
  # transcript under that id. This is the direct analogue of Wispr's
  # conversation.messages.
  CTX_CONV=""
  local sess_kind sess_val transcript
  sess_kind="$(printf '%s' "$agent_json" | jq -r '.agent_session.kind // ""')"
  sess_val="$(printf '%s' "$agent_json" | jq -r '.agent_session.value // ""')"
  if [ "$CTX_AGENT" = "claude" ]; then
    # First by the id herdr reports. That id changes while a session runs and
    # may name a session whose file does not exist yet, so the fallback is the
    # newest transcript in the project directory derived from cwd.
    transcript=""
    if [ "$sess_kind" = "id" ] && [ -n "$sess_val" ]; then
      transcript="$(find "$HOME/.claude/projects" -maxdepth 2 -name "$sess_val.jsonl" 2>/dev/null | head -1)"
    fi
    if [ -z "$transcript" ] && [ -n "$CTX_CWD" ]; then
      # The session directory is named after the path with / . @ replaced by -.
      # The agent often sits in a subdirectory, so walk up to the first
      # directory that exists and take the newest transcript there.
      local dir slug proj
      dir="$CTX_CWD"
      while [ "$dir" != "/" ] && [ -n "$dir" ]; do
        slug="$(printf '%s' "$dir" | sed 's/[\/.@]/-/g')"
        proj="$HOME/.claude/projects/$slug"
        if [ -d "$proj" ]; then
          transcript="$(ls -t "$proj"/*.jsonl 2>/dev/null | head -1)"
          [ -n "$transcript" ] && break
        fi
        dir="$(dirname "$dir")"
      done
    fi
    if [ -n "$transcript" ]; then
      CTX_CONV="$(jq -r --argjson n "$CTX_CONV_MSGS" '
        select(.type == "user" or .type == "assistant")
        | .message.content as $c
        | (if ($c | type) == "string" then $c
           else ($c // [] | map(select(.type == "text") | .text) | join(" ")) end) as $t
        | select($t | type == "string" and length > 0)
        # Machine turns: task notifications, system reminders, cross-session
        # messages. They carry nothing useful for speech context.
        | select($t | test("^\\s*<(task-notification|system-reminder|cross-session-message|local-command|command-name)") | not)
        | "\(.type): \($t[0:300] | gsub("\n"; " "))"
      ' "$transcript" 2>/dev/null | tail -"$CTX_CONV_MSGS")"
    fi
  fi

  # analogue of Wispr's dictionary_context: names the model cannot guess
  CTX_HOTWORDS="$(printf '%s %s %s %s' "$CTX_AGENT" "$CTX_TITLE" "$CTX_BRANCH" "$CTX_FILES" \
    | tr -s ' ' | sed 's/^ //;s/ $//' | cut -c1-"$CTX_HOTWORDS_MAX")"
}

context_report() {
  printf '── pane ──────────────────────────────────────────────\n'
  printf 'pane_id  %s\n' "$CTX_PANE"
  printf 'agent    %s\n' "${CTX_AGENT:-none}"
  printf 'title    %s\n' "${CTX_TITLE:-—}"
  printf 'cwd      %s\n' "${CTX_CWD:-—}"
  printf 'branch   %s\n' "${CTX_BRANCH:-—}"
  printf '\n── file names (%s) ───────────────────────────────────\n' "$(printf '%s' "$CTX_FILES" | wc -w | tr -d ' ')"
  printf '%s\n' "${CTX_FILES:-—}"
  printf '\n── whisper --prompt string (%s words, %s chars) ──────\n' \
    "$(printf '%s' "$CTX_HOTWORDS" | wc -w | tr -d ' ')" "$(printf '%s' "$CTX_HOTWORDS" | wc -c | tr -d ' ')"
  printf '%s\n' "$CTX_HOTWORDS"
  printf '\n── agent conversation, last %s turns (%s chars) ──────\n' \
    "$CTX_CONV_MSGS" "$(printf '%s' "$CTX_CONV" | wc -c | tr -d ' ')"
  printf '%s\n' "${CTX_CONV:-—}"
  printf '\n── pane screen, last %s lines (%s chars) ─────────────\n' \
    "$CTX_LINES" "$(printf '%s' "$CTX_BUF" | wc -c | tr -d ' ')"
  printf '%s\n' "${CTX_BUF:-—}"
}

# Executed directly rather than sourced with `.`
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  PANE=""; ONLY_HOTWORDS=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --pane) PANE="$2"; shift 2 ;;
      --lines) CTX_LINES="$2"; shift 2 ;;
      --files) CTX_FILES_MAX="$2"; shift 2 ;;
      --hotwords) ONLY_HOTWORDS=1; shift ;;
      *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
  done
  for bin in herdr jq; do command -v "$bin" >/dev/null || { echo "$bin not found in PATH" >&2; exit 1; }; done
  collect_context "$PANE" || exit 1
  if [ "$ONLY_HOTWORDS" -eq 1 ]; then printf '%s\n' "$CTX_HOTWORDS"; else context_report; fi
fi
