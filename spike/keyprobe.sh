#!/bin/sh
# THROWAWAY — experiment: does herdr fire a bound command on key auto-repeat?
# Bound to a direct chord (no prefix). Hold the key for a couple of seconds:
# one line means only the initial press fires, many lines mean repeats do too.
B=/opt/homebrew/bin/bash
if [ -x "$B" ]; then
  "$B" -c 'printf "%s\n" "$EPOCHREALTIME"' >> "${TMPDIR:-/tmp}/voice-keyprobe.log"
else
  date '+%H:%M:%S' >> "${TMPDIR:-/tmp}/voice-keyprobe.log"
fi
