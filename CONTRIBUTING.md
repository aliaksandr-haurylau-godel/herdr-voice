# Contributing

## The one rule that is not about code

This repository is public, and its author works on client projects. Nothing that
identifies an employer, a client, an internal system or a private machine may
enter it — not in code, not in comments, not in examples, not in fixtures, not in
commit messages, not in issue text.

Two gates enforce it:

```sh
git config core.hooksPath .githooks   # once per clone
cp .leakwords.example .leakwords      # then fill in the names that must never be committed
brew install gitleaks                 # optional; without it the hook checks fewer patterns
```

`.gitleaks.toml` is tracked and generic. `.leakwords` is untracked, because a list
of employer and client names inside a public repository would disclose what it is
meant to protect.

The hook runs before a commit exists; the workflow runs on every push and pull
request. A public repository keeps history after a revert, so the local hook is the
one that actually protects you.

## How work is organised

Work starts from a GitHub issue and moves through four stages, each producing one
artifact under `tasks/<issue-number>/`: acceptance criteria, design, plan, code.
Each artifact is reviewed by whoever consumes it next before the stage closes, and
the review returns `READY`, `QUESTIONS` or `BLOCKED`. The details are in
[CLAUDE.md](CLAUDE.md).

If you are contributing a single small fix, you do not need the full run: open an
issue, say what is wrong, and send a pull request that links it.

## What a change has to satisfy

- English everywhere: code, comments, output strings, commits, issues.
- Tests next to the code. The pipeline is split by interfaces so that recognition,
  rewrite and delivery can be tested with fakes — no microphone, no model, no live
  multiplexer.
- Failures are visible. A path that can fail must leave a trace: an indicator, a
  toast, or a line in the run journal. A silent failure is treated as a defect of
  the same weight as a wrong transcript.
- The tab label and the sidebar token are restored on every exit path, including
  errors and cancellation.
- Devices are selected by name, never by index.
- Anything verified by hand — real capture, a real multiplexer, another platform —
  is written into `docs/evidence.md` together with the platform it was verified on.

## Verification on platforms we do not have

macOS is exercised; Linux and Windows are not. If you run one of them, the most
useful contribution is a verification issue answered with real output: whether key
auto-repeat reaches the plugin, whether audio capture picks the right device, and
whether the plugin installs from a release archive.
