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

When an example needs a path, use a generic one. When an example needs a project
name, invent a neutral one. History in a public repository survives a revert, so
the check that matters is the one that runs before the commit.

## How work is run

Work starts from a GitHub issue and moves through stages. Each stage produces one
artifact, and the artifact is reviewed by whoever consumes it next before the
stage closes. Stage definitions and verdict format live in the octoflow shared
files; this file states only what is specific to this repository.

| Stage | Produces | Skill | Reviewed by |
|---|---|---|---|
| S1 Assess | `AC_<issue>.md` | `octoflow-assess` | designer, via `octoflow-gate` |
| S2 Design | `DESIGN_<issue>.md` | `superpowers:brainstorming` | planner, via `octoflow-gate` |
| S3 Plan | `PLAN_<issue>.md` | `superpowers:writing-plans` | implementer, via `octoflow-gate` |
| S4 Implement | code | `superpowers:executing-plans` with `superpowers:test-driven-development` | `/code-review` |

Gate verdicts are `READY`, `QUESTIONS` or `BLOCKED`. `QUESTIONS` returns the
artifact to its author; the reviewer never edits what it reviews.

Before claiming that anything is done, run `superpowers:verification-before-completion`:
evidence first, assertions after.

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
