# RUN_42

Installing the plugin compiles it from source, because the build entries never
fetch the release archive.

Input: GitHub issue #42, read with `gh issue view 42`. The issue body and its
comments are the ticket; there are no comments on it.

Run root: `tasks/42/`. Branch `feat/42-install`, cut from `main` at `c64697c`, in
its own worktree.

## Stages

### S1 Assess

- artifact: `AC_42.md`, 10 criteria
- produced: 2026-09-16

Six things were established by running commands rather than by reading the
repository, because each of them decides what a criterion can ask for.

**Nothing is published.** `gh api repos/:owner/:repo/releases --jq 'length'`
answers `0` and `gh api repos/:owner/:repo/tags --jq 'length'` answers `0`;
`gh run list --workflow release.yml` prints nothing. The issue says "the archives
exist on every tagged release and nothing ever consumes them". The first half is
a property of `.github/workflows/release.yml`, not of the repository: no tag has
ever been cut, so no archive has ever been built. `README.md:38` already says
"Not yet installable. When the first release is tagged".

That leaves the ticket's "Done when" with a precondition it does not state: the
container install it asks for cannot run until something is published. Whether
this issue cuts the first release, or proves the path against a throwaway tag,
is the owner's call on versioning and is recorded in `AC_42.md` under "Out of
scope / noticed" rather than as a criterion. Raised with the orchestrating
session on 2026-09-16.

**A build entry runs only on `install`, never on `link`.** herdr's plugin
documentation for 0.9.0, section "Build commands": "`plugin link` does not run
build commands; local authors build their working tree themselves." So replacing
`cargo build --release` in the manifest cannot break local development, which is
the `link` path — `README.md:63` and `scripts/linux-check.sh:333` both use it.
This was the first thing worth knowing, because without it the obvious objection
to the whole issue is "then a checkout stops building".

**A failed build aborts the install and herdr has no fallback of its own.** Same
section: "If a build command fails, install aborts and the plugin is not
registered." The fallback the ticket asks for has to be inside our own script,
and that script has to exit 0 for the install to survive.

**A build command gets no herdr environment.** Same section: build commands "do
not receive runtime plugin context or Herdr socket env", so `HERDR_PLUGIN_ROOT`
is absent and a script has to resolve its own root from its own location.

**The installed checkout is shallow and carries no tags.** Checked in
`~/.config/herdr/plugins/github/persiyanov.reviewr-e87b654a74f8`, a plugin herdr
installed from GitHub on this machine: `git describe --tags` answers "fatal: No
names found, cannot describe anything", `git tag` prints nothing, `.git/shallow`
exists. So a build script cannot learn which release it belongs to from git. The
manifest's own `version` is the only statement available, which fixes the reading
`AC_42.md` takes: the tag is `v<version>`.

**Two plugins on this machine already solve this, in the two shapes the design
has to choose between.** `ranolp.handsfree` does it in one manifest line with
`curl … | tar xz -C target/release`, with the version and the platform written
into the line and no digest check. `persiyanov.reviewr` calls
`["bash", "herdr/install.sh"]` in its own repository, which derives the tag from
its manifest version, maps `uname` to a target triple, fetches a `.sha256`
sidecar and compares it, and exits with a named message on an unsupported
platform. Both are reachable on disk and are quoted in `AC_42.md`.

One thing that could not be established here and is named as such: whether the
build command's working directory is the checkout. herdr's documentation says a
build failure reports the working directory but does not state what it is, and
`persiyanov.reviewr`'s own comment asserts it is the plugin checkout. Confirming
it needs a real `herdr plugin install`, which belongs to the container check in
AC-6 rather than to this machine, where an install would disturb the plugin the
owner has linked.

Gate, round 1:

```yaml
gate:
  stage: S1
  artifact: AC_42.md
  reviewer: designer
  verdict: READY
  date: 2026-09-16
```

The reviewer spot-checked the as-is claims against the repository and they held.
It added one fact this stage had not: `scripts/check_manifest.py:66` skips build
commands that are not the `herdr-voice` binary, so a build entry that fetches an
archive still passes that check. The check therefore does not constrain what the
build entries may become, and it also did not catch the mismatch this issue is
about.

It attached a note rather than a question: AC-6 cannot be executed until some
archive is published, which does not stop S2 — the design can specify the
container check and the reference it installs from — but becomes a hard
precondition at S5. The owner's answer on whether this run cuts `v0.1.0` is
still outstanding.

#### The owner's two decisions, and the second gate they forced

Answered on 2026-09-16, after round 1 had closed.

**A disposable prerelease tagged `v0.0.0`, and no version moves.**
`herdr-plugin.toml:3` and `Cargo.toml:2` both stay `0.0.0`. The first real
release of the plugin is a separate issue this run opens. The tag has to equal
the manifest's version, and the reason is a property of the mechanism rather than
a rule inherited from anywhere: `scripts/check_manifest.py:32-35` compares the
manifest with the crate and never looks at a tag, and `release.yml` takes
`GITHUB_REF_NAME` for the archive name at `:36` and the release name at `:61`
and checks it against nothing, so a disagreeing tag would pass every gate. It is
the fetch path that ties them, because a shallow tagless clone leaves the
manifest as the only place the script can learn what to ask for.

**Only a 404 falls back to a source build**, and a bounded retry sits on top of
it. A timeout, a name-resolution failure or a 5xx stops the install and says to
retry, because falling back there starts the `candle` compile this issue exists
to avoid. The retry is a second rule, not the same one: GitHub answers 404 for
some minutes after a release publishes, so without it a CDN that has not caught
up is mistaken for a missing target and answered with an hour-long build. The
retry decides whether we know; the 404 rule decides what we do once we know.

Two things were established before relying on the `v0.0.0` shape, because the
instruction to "mark it as a prerelease" assumed a step that does not exist:

- **`release.yml` would publish `v0.0.0` as the repository's latest release.**
  `gh release create` at `:61` passes neither `--prerelease` nor `--latest=false`.
  Both flags exist — `gh release create --help` lists `-p, --prerelease` and
  `--latest=false`. So marking the prerelease is a change to the workflow or a
  hand edit afterwards, and design chooses which.
- **`--generate-notes` copes with a repository that has never had a release.**
  `gh api -X POST repos/:owner/:repo/releases/generate-notes -f tag_name=v0.0.0
  -f target_commitish=main` returned notes covering every merged pull request
  from #5 to #63, plus a New Contributors section. Nothing had to be worked
  around.

Those decisions moved publishing an archive out of "noticed" and into this
issue's scope, which changed the criteria: AC-4a and AC-4b for the retry and the
network rule, AC-5a for what a person sees when a build entry fails and the
plugin ends up unregistered, AC-6a and AC-6b for publishing the prerelease and
keeping it alive until the evidence is written. A changed artifact is a new
gate, so round 1's verdict does not carry.

Gate, round 2:

```yaml
gate:
  stage: S1
  artifact: AC_42.md
  reviewer: designer
  verdict: READY
  date: 2026-09-16
```

The reviewer was asked to look for the neighbour problem — a section edited and
its neighbours left describing the old answer — and found one. The bullet about
`README.md:38` kept a conclusion that still holds while its reason had stopped
being true: it said the sentence "Not yet installable" belongs to whatever cuts
the release, "when a release is tagged, not when this issue lands", which AC-6a
had just made false. The bullet now says why the sentence stays anyway — a
disposable prerelease marked not-latest exists to prove the fetch path, and
rewriting the README around it would advertise the test tag as a way to install.

Gate, round 3:

```yaml
gate:
  stage: S1
  artifact: AC_42.md
  reviewer: designer
  verdict: READY
  date: 2026-09-17
```

Round 3 was forced from downstream rather than from a revision: the S2 planner
gate found that AC-4 demanded something the mechanism cannot produce, and that a
reachable state had no criterion at all.

**AC-4 asked for a distinction that does not exist.** It required the fallback
message to name which of two things happened — no release carries the tag, or
that release has no build for this platform. Both answer 404 on the same URL, and
the script fetches nothing else that would separate them. Telling them apart needs
a release-level request to the GitHub API, which brings a second network
dependency, its own retry, its own failure mode, and an unauthenticated limit of
sixty requests an hour per address that a container or a runner behind a shared
address can exhaust. That is a fifth way for the install to fail, bought for one
clause in one message. AC-4 now asks for a message naming the URL, the tag, the
target and both readings.

**AC-3a is new.** The archive arriving while its digest does not was reachable and
named nowhere, while AC-2 forbids unpacking anything unverified — so whoever
implemented the verification would have decided it silently and no test would have
had anything to assert. It stops: nothing unpacked, no fallback, and a message
saying the release is missing that archive's digest.

After the verdict, one wording change that narrows nothing: AC-3a said "its digest
sidecar", which pre-decided a form the criteria elsewhere leave to design. It now
says "the digest published for it — whatever form that digest takes". The
reviewer raised it as the author's choice rather than as a question.

Gate, round 2:

```yaml
gate:
  stage: S3
  artifact: PLAN_42.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-17
```

The reviewer traced the test script symbol by symbol across tasks 2, 3 and 4 in
insertion order and found no forward reference, checked each stub against what
`hv_main` actually calls it with, and confirmed every line range cited against
the repository. It did not re-raise the trap finding.

One thing it caught: task 5 said to replace `herdr-plugin.toml:7-17`, and line 7
is the blank separator after `platforms` — the comment starts at 8. Corrected,
with the blank line named so it is not eaten.

### Between S1 and S2: seventeen hours of waiting that looked like work

S2's design was presented for approval on 2026-09-16 at 16:53 and the answer
arrived on 2026-09-17 at 11:04. Two questions were open: whether the design shape
held, and which reference the container check installs from. Nothing was blocked
on a permission prompt and no subagent was outstanding — the question had simply
been put up in a place the owner was not looking, and a question panel raises no
signal anywhere else.

The answer came through the orchestrating session, which is the path that does
reach him. The rule taken from it: when a question goes up, it also goes to the
orchestrator in a message, because the panel alone stalls silently.

It is the same defect this repository keeps recording in other forms — a wait
that is indistinguishable from progress. `docs/evidence.md` has the recording
version of it, where a working take and a hung one looked alike for eight hundred
milliseconds, and #40 exists because of it. This one cost seventeen hours.

### S2 Design

- artifact: `DESIGN_42.md`
- produced: 2026-09-17

The owner approved the shape before it was written: a repository script per
platform family rather than a manifest line, `.sha256` sidecars rather than one
`SHA256SUMS`, the prerelease rule inside `release.yml` rather than a hand edit
after the run, a separate `scripts/install-check.sh` rather than an extension of
`linux-check.sh`, and the shell tests as a job in `check.yml`.

He also chose the reference the container check installs from. The design as
presented used the branch, and the weakness surfaced while the sequencing was
being written down: a branch head moves on after the tag is cut, so the evidence
could describe a tree the published archives did not come from. `--ref v0.0.0`
pins one tree, and the manifest the script reads and the archives it fetches
cannot then disagree.

Gate, round 1:

```yaml
gate:
  stage: S2
  artifact: DESIGN_42.md
  reviewer: planner
  verdict: QUESTIONS
  date: 2026-09-17
  questions:
    - "Section 4 required the fallback message to name which of two things
       happened — no release with that tag, or a release without that target —
       and nothing the script fetches carries that distinction: both answer 404
       on the same URL. No step created the extra information, and section 8
       asked for a different message than section 4 did."
    - "The archive arriving while its digest sidecar does not was named by no
       outcome and no assertion, while AC-2 forbids unpacking anything
       unverified. An implementing task would have taken the decision silently
       and a test task would have had nothing to assert."
    - "Section 10 never named the step that creates `scripts/install-check.sh`;
       step 7 only ran it. The write-the-container-check task could not be placed
       in the dependency graph."
```

All three were real and all three were of the shape this repository's gates catch
most often: something a later part needs that no earlier part creates.

The first cost a criterion. Separating "no release with that tag" from "no build
for this platform" needs a release-level request to the GitHub API, and that buys
one clause in one message in exchange for a second network dependency with its own
retry, its own failure mode and a sixty-an-hour unauthenticated rate limit that a
shared address can exhaust. The distinction was dropped rather than bought: the
message now names the URL, the tag, the target and both readings, and AC-4 was
corrected to match, which is what forced S1's third round.

The second became a fifth outcome. An archive in hand with no digest means the
release exists and this platform is built, and only the means to check the bytes
is missing — so it stops, unpacking nothing and falling back to nothing. Falling
back would answer a broken release with an hour of compiling and never mention
that the release is broken.

The third was an ordering hole. `scripts/install-check.sh` is code in the tree, so
it is written with the rest of it and is part of what the four gates pass and what
the tag pins; only running it needs a published release.

Adding a row to the failure table falsified three counts stated in prose
elsewhere — "four outcomes", "four different things", "two distinct named failure
messages". That is the neighbour problem this repository warns about, met in its
usual form.

Gate, round 2:

```yaml
gate:
  stage: S2
  artifact: DESIGN_42.md
  reviewer: planner
  verdict: READY
  date: 2026-09-17
```

Two things were corrected after the verdict, neither of which changes what is
required. A fourth stale count survived in a motivation paragraph — section 8 said
the script decides "which of four things happened" above a list of five. And the
Windows half of the test job was stated as behaviour in section 8 while section
10 named no step that produces it; step 3 now names it, leaving to the plan
whether those assertions live inline in `check.yml` or in a
`scripts/test-install.ps1`.

One risk the reviewer named and did not treat as a planning gap: the design assumes
a build command's working directory is the checkout, which is the one thing S1
could not establish and which the container check in step 8 is what exposes.

### S3 Plan

- artifact: `PLAN_42.md`, seven tasks
- produced: 2026-09-17

Four defects were found in the plan by running its shell mechanics before the
gate reported, rather than by reasoning about them. Two mattered.

**The retry test asserted the opposite of the rule it exists to protect.** The
stub counted calls in a shell variable, but `hv_fetch` calls `hv_fetch_once`
inside `$( )` and the suite calls `hv_fetch` inside `$( )` again — two subshell
layers, so the counter never advanced and every attempt read the first answer.
Run directly, the "404 that later recovers" case returned `missing` where the
rule requires `ok`. That test would have passed only by never exercising
recovery, and would have blessed a script that compiles for an hour whenever
GitHub's content delivery network lags — the exact hazard the rule was written
against. The counter now lives in a file; verified as `ok` after three calls.

**The exit-code capture could never run.** `run_main` ended a subshell with
`echo "$?" >file`, but `hv_die` calls `exit`, so that line is unreachable and
every failure assertion would have read a stale code from the case before it.

The other two: the missing-`cargo` test called `builtin_command`, which does not
exist, and emptying `PATH` instead would not work either, because `hv_main` needs
`mktemp` before it reaches the fallback — so `install.sh` now asks through an
`hv_have_cargo` function the suite can replace. And task 7's `PLUGIN_ROOT` had a
`sed` whose output was discarded sitting in front of the `find` that did the work.

Gate, round 1:

```yaml
gate:
  stage: S3
  artifact: PLAN_42.md
  reviewer: implementer
  verdict: QUESTIONS
  date: 2026-09-17
  questions:
    - "The EXIT trap set in task 2 is inherited by every later subshell, so each
       `$( )` re-runs `rm -rf` on the fixture directory, and task 4's `cp` of the
       fixture manifest then fails under `set -eu`."
    - "`run_main` cannot capture `hv_die`'s exit code: under `set -e` the subshell
       ends at the failing command and the line writing the code never runs."
```

The second was right, and was already fixed before the verdict arrived.

**The first is wrong, and was established as wrong by running it.** An EXIT trap
is not inherited by subshells: a trap set in the parent does not fire when a
`$( )` or a `( )` subshell terminates. Checked on 2026-09-17 with a fixture
holding a marker file, after a command substitution, after an explicit subshell,
and after a subshell exiting 1 — the marker survived all three in `/bin/sh`, in
`/bin/bash` and in `dash`, which is the shell this suite runs under on the Linux
CI runner. The reasoning in the finding is the common belief about traps and
subshells, and it does not hold; the fixture lives until the script itself ends,
which is what the plan relies on.

Nothing was changed for that finding. Recorded here because a verdict that was
answered by disproving it is worth more later than one that was quietly dropped.

**The plan was then dry-run rather than argued about.** `scripts/install.sh` and
`scripts/test-install.sh` were assembled exactly as tasks 2, 3 and 4 build them,
against a throwaway checkout holding a copy of this repository's manifest, and
executed. All 34 assertions pass, under `/bin/sh`, under `dash` — the shell the
suite runs under on the Linux CI runner — and under `bash`, with no network and
no release: the fetches are stubbed and the archive is a real tarball built in
the fixture. The fixture was still present at the end of every run, which is the
first finding disproved a second time, on the actual code rather than on a
reduction of it.

That is why the two defects above were found before the gate reported them. A
plan whose central risk is shell semantics can be run, and running it is cheaper
than reasoning about it and more reliable than either.

### S4 Implement

- produced: code, in eight commits on `feat/42-install`
- date: 2026-09-17

All seven tasks executed as planned, the four gates green before each commit, and
the shell suites run under `sh`, `dash` and `bash`, because `dash` is what the
Linux runner uses.

Two deviations, both deliberate. `install.ps1` asks through a `Test-HvCargo`
function rather than an inline `Get-Command`, mirroring the `hv_have_cargo` seam
the shell script has. And the Windows CI job runs the suite under `shell: pwsh`
directly rather than the plan's `pwsh -File` nested inside `shell: pwsh`, which
would have started a second PowerShell for nothing. `pwsh` turned out to be on
this machine, so the Windows tests were run rather than deferred to CI as the
plan allowed.

#### The review of the diff

```yaml
gate:
  stage: S4
  artifact: the diff c64697c..2b1b5d0
  reviewer: code
  verdict: QUESTIONS
  date: 2026-09-17
  findings: 7 important, 0 critical
```

No finding was a wrong answer. Most were a right answer that could not be relied
on, which is the shape worth recording.

**`curl` had no transfer timeout.** The design promises that a timeout stops the
install, and curl has no maximum transfer time by default — so a connection that
opens and then stalls would hang the build entry indefinitely with nothing on
screen. Everything about that rule was right except that it was not implemented.
`--connect-timeout 20 --speed-limit 1024 --speed-time 30` ends a stalled transfer
without capping a slow but living one.

**The container check rested on something nobody had established.** It read the
install log for "verified against its published digest". herdr reports a build
command's output when the build *fails*, and nothing in the contract says it
echoes a successful one — so against a herdr that stays quiet, the check would
have failed a working install and told the person the build entry had not run.
Rather than establish herdr's behaviour and depend on it, both scripts now write
what they did to `target/release/.herdr-voice-install` and the check reads that.

**The Windows script was tested under the wrong interpreter, and barely tested.**
The manifest runs `powershell`, which is Windows PowerShell 5.1; CI ran `pwsh`,
which is PowerShell 7. They differ in exactly the uncovered code:
`Invoke-WebRequest` raises `WebException` in one and `HttpResponseException` in
the other, so the status extraction the fetch depends on is a different type in
each. CI now runs both. Of the five outcomes, the shell script covered all five
and the PowerShell script covered none.

Writing that suite found a defect, which is the argument for having written it:
**PowerShell resolves a variable a function does not define from its caller's
scope**, so the test's fetch stub was silently reading `Invoke-HvMain`'s own
`$archivePath` and copying the wrong file. The locals are now named so they
cannot collide. Building paths per segment with `Join-Path` rather than as a
literal `target\release` fixed the other half and made the suite runnable off
Windows.

The rest: the temporary directory is removed on Windows as it already was on
Unix; a verified archive that will not unpack says so rather than ending on
`tar`'s own complaint; a failed source build says nothing was installed; digests
compare case-insensitively; and `scripts/check_manifest.py` now fails when a
build entry names a script that is not in the tree — the same class of defect
that script exists to catch, verified by introducing a typo and watching it fail.

Two documentation points from the same review. `docs/design.md` section 8 said
the entries "refuse it if its bytes do not match the published digest" without
saying what that buys: a digest fetched from the release it verifies proves the
archive arrived intact, not who published it. And a fork installs the upstream
archives, because both scripts name this repository.

After the fixes: 39 shell assertions, 23 PowerShell assertions, 6 for the release
rule, and the 440 Rust tests, all passing.

### S5 Verify

Not started. It needs the `v0.0.0` prerelease published, which is outward-facing
and the owner's to authorise. AC-6, AC-6a and AC-6b are open until then; every
other criterion is met and tested.

## Notes

The four local gates for every commit on this branch: `cargo test`,
`cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
`python3 scripts/check_manifest.py`.
