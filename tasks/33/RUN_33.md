# Run 33

Input: GitHub issue #33, read with `gh issue view 33`.

## Stages

S1 to S3 skipped. The shape was fixed by the failure itself: PR #32 (issue #21)
failed the required `secrets and employer identifiers` check on a false
positive, and the fix is narrow and mechanical — exclude the domains RFC 2606
reserves for documentation and testing from one rule.

## What changed

`.gitleaks.toml`, the `corporate-mail` rule gains an allowlist excluding
`@example.com`, `@example.net` and `@example.org`, matched against each match's
own span (`regexTarget = "match"`) rather than the extracted secret or the whole
source line.

## Why "match", and not the secret or the line

The rule reports the wrong text as its "secret": gitleaks defaults to the
regex's first capture group when no `secretGroup` is set, and this rule's group
1 is only `(com|net|org)` — so the reported secret is a bare `com`, never the
`@example.` prefix an allowlist keyed on the secret would need to see. This is
plain default behaviour, not RE2 backtracking or match-order — an earlier
version of this fix stated the wrong mechanism and was corrected during the S4
gate.

The first attempt worked around it by matching the allowlist against the whole
source line instead. That is unsafe on its own: a real corporate address
sharing a line with an unrelated `example.com` mention is cleared along with
it — confirmed by running gitleaks against a line pairing an `example.com`
fixture address with a company-shaped one, not reproduced here, which produces
no findings at all under `regexTarget = "line"`. `regexTarget = "match"`
compares the allowlist against each match's own span, so it excludes the
`example.com` match without touching a different match on the same line.

## Verified by running it, against the exact failure

`gitleaks` 8.30.1 installed locally. Confirmed the failure first: run against the
precise commit range GitHub scanned for PR #32
(`0040532eb451dd01801473b92f8fda142e988eb2^..8cc1cd917e39eb1fd22c498e33574576dbed73f0`,
50 commits) with the unmodified config reproduces `leaks found: 2`, both
`test@example.com` and `t@example.com` in `tasks/21/PLAN_21.md`.

The same range with the fixed config: `no leaks found`.

The opposite case, so the fix does not just make the rule silent: a scratch
repository outside this checkout, never committed here, with one file
containing four lines — an address at a domain shaped like a real company's,
that same address embedded in a home path, an internal-looking host URL, and an
`example.com` fixture. The fixed config still flags the company-shaped address
under `corporate-mail`, still flags the home path under `personal-home-path`,
still flags the internal host under `internal-hosts`, and only the RFC-reserved
fixture is silent. (The probe file itself is not reproduced here, since the
point of this rule is exactly to keep a realistic-looking address out of the
repository.)

The line-sharing case the S4 gate raised: a one-line fixture pairing an
`example.com` fixture address with a company-shaped one on the same line, in the
scratch repository described above and not reproduced here, finds nothing under
`regexTarget = "line"` — the whole line is cleared, company-shaped address
included — and correctly flags only the company-shaped one under `regexTarget =
"match"`. The `example.com` half stays excluded either way. Re-run against the
exact PR #32 commit range with `regexTarget = "match"`: `no leaks found`, same
as `"line"` — the line-sharing gap does not affect this particular history, but
it would affect a future one, which is why the target was changed rather than
left as it stood after the first pass.

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff of fix/33-leak-gate-example-domain against main
  reviewer: superpowers:requesting-code-review
  verdict: QUESTIONS
  date: 2026-09-02
  blocker: null
```

One finding, and it changed the fix rather than adding a caveat to it.
`regexTarget = "line"` clears a real corporate address whenever it shares a
line with an unrelated `example.com` mention — confirmed by the gate with a
plain two-address fixture, no contrivance needed. `regexTarget = "match"` was
available, untried, and closes the gap while still solving the truncated-secret
problem. The gate also corrected the stated mechanism: the truncation is
gitleaks defaulting to the rule's first capture group as the secret, not RE2
leftmost-match semantics — the wrong explanation happened to point at a working
fix, but not the safest one available.

Also confirmed by the gate, independently, and requiring no change: the
allowlist regex itself does not sweep in `notexample.com`-style domains; a real
employer registered under an RFC 2606 reserved domain is not a real risk;
`personal-home-path` and `internal-hosts` are untouched.

## Gate S4, second pass

```yaml
gate:
  stage: S4
  artifact: the diff of fix/33-leak-gate-example-domain against main
  reviewer: superpowers:requesting-code-review
  verdict: QUESTIONS
  date: 2026-09-02
  blocker: null
```

The first finding is fixed and reproduced independently: the gate built its own
line-sharing fixture, confirmed the old target cleared it entirely and the new
one catches the real address while excluding the fixture. Its own read of the
JSON report's `Secret` field also confirmed the corrected mechanism directly.

One new finding, mine rather than the gate's own review target: a scratch file
`.gitleaks-match-test.toml`, made while testing the fix by hand, was swept into
the commit by `git add -A` and left in the tree — referenced nowhere, and still
carrying the retracted explanation the main file's comment had already been
rewritten to correct. Removed by amending the commit rather than adding a third
one on top, since it introduced the file in the first place.

Re-verified after the amendment: the tree has no leaks by content scan, the full
range from `main` has no leaks by history scan, and the exact commit range that
failed PR #32's check still passes.
