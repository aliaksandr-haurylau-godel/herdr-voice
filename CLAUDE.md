# CLAUDE.md

Voice dictation plugin for herdr. Read `docs/design.md` before touching anything:
it states how the plugin is built and why. `docs/evidence.md` holds the
measurements the design rests on.

## Language

Everything inside the repository is written in English: code, comments, output
strings, commits, issues, pull requests, documents. Conversation with the author
may happen in another language, but nothing from that conversation is copied into
the repository verbatim.

## The public-repository rule

This repository is public and the author works on client projects. Nothing that
identifies an employer, a client, an internal system or a private machine may
enter it — not in code, not in comments, not in examples, not in test fixtures,
not in commit messages.

Two gates enforce it:

- `.githooks/pre-commit` — before the commit exists. Enable once per clone with
  `git config core.hooksPath .githooks`.
- `.github/workflows/leak-gate.yml` — on every push and pull request.

The rules are split on purpose. `.gitleaks.toml` is tracked and holds generic
rules: secrets, personal home paths, internal hosts. The names of employers and
clients live in `.leakwords`, which is never committed — a list of those names
inside a public repository would disclose exactly what it is meant to protect.
Copy `.leakwords.example` and fill it in per clone.

Cite paths **relative to the repository root**, with a line number when it helps:
`spike/README.md:3`. An absolute path drags a home directory and an account name
into a public repository, and the leak gate rejects it.

When an example needs a path, use a generic one. When an example needs a project
name, invent a neutral one. History in a public repository survives a revert, so
the check that matters is the one that runs before the commit.

## How work is run

Work starts from a GitHub issue and moves through stages. Each stage produces one
artifact, and the artifact is reviewed by whoever consumes it next before the
stage closes. The verdict format lives in the octoflow shared files; the stages
that apply here are the five below and no others.

| Stage | Produces | Skill | Reviewed by |
|---|---|---|---|
| S1 Assess | `AC_<issue>.md` | `octoflow-assess` | designer, via `octoflow-gate` |
| S2 Design | `DESIGN_<issue>.md` | `superpowers:brainstorming` | planner, via `octoflow-gate` |
| S3 Plan | `PLAN_<issue>.md` | `superpowers:writing-plans` | implementer, via `octoflow-gate` |
| S4 Implement | code | `superpowers:executing-plans` with `superpowers:test-driven-development` | `superpowers:requesting-code-review`, before a pull request exists |
| S5 Verify | a section in `docs/evidence.md` | `superpowers:verification-before-completion` | the run records the verdict |

Gate verdicts are `READY`, `QUESTIONS` or `BLOCKED`. `QUESTIONS` returns the
artifact to its author; the reviewer never edits what it reviews. Every verdict,
including S4's and S5's, is recorded in `RUN_<issue>.md`.

**S4 closes on a review of the diff, not on a pull request.** `/code-review`
runs only against an open pull request and answers with a comment rather than a
verdict, so it cannot close a stage; use it after the pull request is open, as a
second pass. The gate exists because two defects went through S4 unreviewed on
2026-08-25 — a `cancel` that stops nothing, and a reply that arrives truncated
along with the level and the target pane — and both were found by hand afterwards,
since nothing looked at the code between "the tasks are done" and "the pull
request is open".

**S5 is a step that can fail, not a request.** Run the thing, read the whole
output, and write what happened into `docs/evidence.md` with the platform it was
verified on. A negative result, recorded, passes this stage; a claim with no
command and no output beside it does not.

The octoflow shared definitions also describe stages whose skills are not on
disk — `octoflow-take`, `octoflow-design`, `octoflow-plan`, `octoflow-implement`,
`octoflow-verify` and `octoflow-ship`. They do not apply here, and nothing
outside this repository is edited to make that so: the same reason the stage
reviewers are project agents in `.claude/agents/` rather than additions to
anyone's global set.

### Trigger and run root

The trigger is a GitHub issue, not a ticket tracker. Read it with
`gh issue view <number>`; treat the issue body and its comments as the ticket, and
record in `RUN_<issue>.md` that the input came from GitHub.

The run root is `tasks/<issue-number>/`. Every artifact of that run lives there:
`RUN_<issue>.md`, `AC_<issue>.md`, `DESIGN_<issue>.md`, `PLAN_<issue>.md`.

### Branches and pull requests

One branch per issue, named `feat/<issue>-<short-slug>` or `fix/<issue>-<short-slug>`.
Never commit to `main` directly. The pull request links the issue and carries the
checklist from the pull request template.

## Rules for the code

- No panic paths in the daemon. A failed stage reports through the indicator and
  the run journal and leaves the tab label and the sidebar token restored.
- Every user-visible failure names what to do next. "Silent failure" is a defect
  of the same weight as a wrong transcript: the shell prototype spent a whole
  morning looking like a hang because a parse error produced no output at all.
- Device selection by name, never by index.
- Platform differences are declared in the manifest through `platforms`, not
  branched inside the binary, wherever the manifest can express them.
- Configuration keys have defaults; an absent configuration file is a valid state.

## Local development

```sh
cargo test                          # unit tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
python3 scripts/check_manifest.py   # the manifest must name commands the binary accepts
herdr plugin link .                 # install this checkout as a plugin
herdr plugin log list --plugin haurylau.voice   # what herdr ran and what it returned
```

One worktree per line of work. Two agents must not share a checkout: a branch
switch in a shared working directory makes the other one's files vanish mid-edit.
Use `git worktree add` for a second line of work, and read other branches with
`git show` rather than by checking them out.

`scripts/check_manifest.py` exists because the manifest is the only contract with
herdr: a command named there but rejected by the binary produces a plugin that
installs and then does nothing when its action is invoked. CI runs the same check.

Infrastructure runs may skip S1 to S3 when the shape is already fixed by the
design document, and say so in `RUN_<issue>.md`. Anything that changes behaviour
goes through every stage. S4 and S5 are never skipped: a run that writes no code
still verifies what it did, and a run that writes code has its diff reviewed
before the pull request.

## Testing

Unit tests next to the code. The pipeline stages are separated by interfaces
precisely so they can be tested without a microphone, a model or a live herdr:
audio in from a file, transcription and rewrite through a fake engine, delivery
through a recorded call.

Anything that cannot be covered that way — real capture, real herdr, Windows — is
verified by hand and written down in `docs/evidence.md` with the platform it was
verified on.

## The prototype

`spike/` holds the shell prototype the design came from. It is not built, not
shipped and not maintained; it is kept because every number in `docs/evidence.md`
was produced with it. Do not extend it — new behaviour goes into the plugin.
