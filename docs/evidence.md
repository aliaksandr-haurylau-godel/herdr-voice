# Evidence

Measurements the design rests on. Every number here was produced on one machine —
Apple silicon, macOS 25.6, herdr 0.8.2 — with the shell prototype kept in `spike/`.
Nothing here has been reproduced on Linux or Windows.

## Key auto-repeat through herdr

A direct chord (no prefix) was bound to a command that appended a high-resolution
timestamp, then held down. Three holds were recorded in one log.

| hold | events | duration | gap before the 2nd event | median gap after |
|---|---|---|---|---|
| 1 | 62 | 5.05 s | 44 ms | 84 ms |
| 2 | 1 | single tap | — | — |
| 3 | 50 | 4.10 s | 99 ms | 85 ms |

Conclusions: herdr invokes a bound command on every auto-repeat, at roughly twelve
per second; there is no half-second pause before repeats begin; and a single tap
is distinguishable from a hold by producing exactly one event.

## Context and its effect on the transcript

Target pane: an agent working in a notes repository. Spoken phrase, in Russian,
containing one English term: "what is this worklog about, tell me more".

| stage | time | result |
|---|---|---|
| whisper, no context | 2 s | the English term came out as an unrelated Russian word, no punctuation |
| whisper, context in `--prompt` | 1 s | punctuation and capitalization appeared, the term stayed wrong |
| rewrite with context | 7 s | term restored, punctuation and word forms corrected |

Conclusion: biasing recognition improves form but does not restore terms; the
repair happens in the rewrite stage. The term was missing from the bias string
because only file basenames were collected and the term was a directory name —
fixed by including path components.

## Rewrite engines

Same phrase and context for every row.

| engine | time | outcome |
|---|---|---|
| agent CLI, mid-tier model, MCP servers disabled | 4.6 s | correct |
| agent CLI, top-tier model, MCP servers disabled | 5.4 s | correct |
| agent CLI, mid-tier model, MCP servers loaded | 7.3 s | wrong term |
| agent CLI, small model, MCP servers disabled | 11.2 s | not corrected |
| local model, 9.6 GB | 16 s | not corrected |
| local model, 18 GB | 49 s | wrong term, word forms corrected |

Conclusions: on this machine local models are both slower and worse than an agent
command-line tool; and disabling the agent's tool servers saves almost three
seconds, which is more than the difference between two model tiers.

## Prompt size limit

whisper.cpp accepts an initial prompt of about `n_text_ctx / 2` tokens, on the
order of 224. A full repository file listing produced 7277 characters — beyond the
limit, and alphabetically ordered, so the surviving prefix was noise. Replaced by
recently touched names capped at 600 characters.

## Pane screen versus conversation transcript

For a pane running a full-screen agent, 80 screen lines reduced to 59 characters
after dropping lines without letters or digits: a frame and a status line. The
same agent's session transcript yielded about 2.6 KB of content over six turns.

## Silent recording

A recording made from the wrong input measured a mean volume of −91 dB over 4.37
seconds, and recognition returned a single period. A real take of comparable
length measured −46.9 dB. Hence the loudness check before transcription.

## Daemon, client and doctor

Verified by hand on macOS 25.6, Apple silicon, herdr 0.8.2, with the release build
of the plugin. Nothing here was reproduced on Linux or Windows.

| what | command | result |
|---|---|---|
| `doctor` on a machine with no configuration file | `herdr-voice doctor` | five lines; `config default`, `model missing`, `rewrite ok found "claude" in PATH`; exit 1 |
| a client with no daemon | `herdr-voice cancel` | one line naming the socket and how to start the daemon; exit 1, no hang |
| the daemon stays alive | `herdr-voice daemon &` | the process is still there a second later; `doctor` reports `daemon ok` and the path |
| a second start | `herdr-voice daemon` | "a daemon is already listening at …", exit 0, still one process |
| a request carrying a context | `HERDR_PLUGIN_CONTEXT_JSON=… herdr-voice cancel` | exit 0; the daemon recorded `request command=cancel entrypoint=cancel context=41 bytes` |
| a request with no context | `env -u HERDR_PLUGIN_CONTEXT_JSON herdr-voice cancel` | exit 0, and the daemon recorded the context as unreadable on its own line |
| a leftover socket after `kill -9` | `kill -9`, then `herdr-voice daemon` | the socket file survived the kill; the next daemon reclaimed it and `doctor` reported `daemon ok` |
| an action through herdr | `herdr plugin link .`, `herdr plugin action invoke cancel` | exit 0; herdr logged `status: succeeded, exit_code: 0`; the daemon recorded a request of 375 context bytes |

Three things this exercise established that the design had assumed:

**A liveness probe is not a malformed request.** `doctor` and a second `daemon`
start both connect and close without sending a frame, and the first version
reported each one as `connection failed: malformed header: ""`. Three of those
lines appeared in a log that had answered two real requests. The frame reader now
distinguishes a peer that sent nothing from one that sent something wrong, and the
daemon stays quiet about the former. A log that cries failure over the normal case
is the same defect as a log that says nothing about a real one.

**herdr does not set `HERDR_PLUGIN_ENTRYPOINT_ID` for an action invoked from the
command line.** The recorded line read `entrypoint=-`. The variable is documented
and the frame carries it, but on this path it does not arrive, so the `-` that
stands for an absent token earned its place on the first real invocation rather
than in a test.

**The configuration directory the plugin derives is the one herdr computes.**
`herdr plugin list` printed
`config: ~/.config/herdr/plugins/config/haurylau.voice` for the linked plugin, which
is what `config::directory` produces from `HERDR_PLUGIN_CONFIG_DIR` or, without it,
from `$HOME`.

Not verified, and why: the daemon's own log line reaching
`herdr plugin log list` requires herdr to have started the daemon through the
manifest's `startup` entry, which means restarting herdr. The daemon was started by
hand instead, so its line was read from its own standard error. The plugin was
linked for the action check and unlinked afterwards; `herdr plugin list` was
identical before and after.
