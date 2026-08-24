#!/bin/sh
# THROWAWAY — launcher for spike.sh, kept deliberately tiny and POSIX-clean.
#
# herdr starts a detached `type = "shell"` binding with a minimal environment:
# PATH has no /opt/homebrew/bin, so `env bash` resolves to macOS bash 3.2, which
# cannot even parse spike.sh. A parse error leaves no log line and no toast, so
# the keypress looks like nothing happened. This launcher repairs HOME and PATH
# and hands over to a modern bash.
[ -n "${HOME:-}" ] || HOME="/Users/$(id -un)"
export HOME
PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
export PATH
HERE=$(dirname "$0")

for candidate in /opt/homebrew/bin/bash /usr/local/bin/bash; do
  if [ -x "$candidate" ]; then
    exec "$candidate" "$HERE/spike.sh" "$@"
  fi
done

printf '%s no bash >= 4 found; spike.sh needs one\n' "$(date '+%H:%M:%S')" \
  >> "${TMPDIR:-/tmp}/voice-spike-boot.log"
herdr notification show "Dictation error" --body "no modern bash found" --sound request >/dev/null 2>&1
exit 1
