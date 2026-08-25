# Decisions

Everything decided without the repository's owner. One line each, with the reason
it was decided that way and where it happened, so any of it can be read in one
sitting and any of it can be reversed.

Each entry also lives in the `RUN_<issue>.md` of the run that took it, in context.
This file is the list; the run file is the story.

| Decision | Basis | Where |
|---|---|---|
| The Linux check was run now, in a container on the development machine, instead of being handed to the owner as a script to run later | Apple `container` is installed and running there, so a throwaway Debian container was available without installing anything; a result costs the owner nothing to read, and a script left unrun establishes nothing | 2026-08-25, #9 |
| The check is one script, `scripts/linux-check.sh`, that installs its own prerequisites and prints a step table | The value of the exercise is that it can be repeated after issue #8 lands capture and on a machine that is not this one; a sequence of commands typed by hand is not repeatable and its result is not comparable | 2026-08-25, #9 |
| A capture-facing command that exits 69, "not implemented yet", is recorded as pending and never as a pass | The point of a container with no sound hardware is the device-absence behaviour, and it does not exist yet. A step that goes green on "not built" would go green forever | 2026-08-25, #9 |
