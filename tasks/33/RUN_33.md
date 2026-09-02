# Run 33

Input: GitHub issue #33, read with `gh issue view 33`.

## Stages

S1 to S3 skipped. The shape was fixed by the failure itself: PR #32 (issue #21)
failed the required `secrets and employer identifiers` check on a false
positive, and the fix is narrow and mechanical — exclude the domains RFC 2606
reserves for documentation and testing from one rule.

## What changed

`.gitleaks.toml`, the `corporate-mail` rule gains an allowlist excluding
`@example.com`, `@example.net` and `@example.org`, matched against the source
line rather than the extracted secret.

## Why the line, not the secret

The rule's own regex, under RE2's leftmost-match semantics (gitleaks uses
`go-re2`, which does not backtrack the way PCRE does), sometimes reports only a
trailing fragment as the matched secret — `com` rather than `test@example.com` —
because of the optional non-capturing group at the end of the pattern. An
allowlist regex matched against that fragment alone could never see the `@example.`
prefix it needs to recognise. Matching against the whole source line sidesteps
what the rule's own match boundary happens to be.

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
