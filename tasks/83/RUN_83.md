# RUN_83

| field | value |
|---|---|
| issue | #83 — The Windows install fails at the digest check where Get-FileHash does not exist |
| input | GitHub issue, read with `gh issue view 83` |
| stage | S1 |
| branch | fix/83-windows-install-tar |
| opened | 2026-09-18 |

## Stages

<!-- One block per stage, appended, never rewritten. -->

### S1 Assess
- artifact: `AC_83.md`
- produced: 2026-09-18

Written directly from the issue body, a re-run of both diagnostic steps the
issue itself prescribes, and a real, twice-repeated reproduction attempt of
`herdr plugin install` on this machine — the same machine the issue was filed
against ($PSVersionTable.PSVersion = 5.1.26100.9444, an exact match to the
issue's own recorded value).

**Missing S1 reviewer, noted rather than worked around.** `CLAUDE.md`'s S1 row
names `octoflow-assess` as the producing skill and "designer" as the gate
reviewer. Neither exists in this environment: `octoflow-assess` is absent
from the installed skills (checked against `~/.claude/skills` and every
installed plugin), and no project agent in `.claude/agents/` is named for a
"designer" role — only `octoflow-reviewer-planner.md` (S2) and
`octoflow-reviewer-implementer.md` (S3) exist. `tasks/36/RUN_36.md`
established the precedent for the missing skill (write the artifact
directly); there is no established precedent in this repository for a
missing S1 *reviewer*, so this run extends the same precedent one step
further — the gate is recorded as self-reviewed, and that gap is stated here
rather than an invented reviewer standing in for one. This is a process gap
worth the owner's attention, not a decision this run can make on the owner's
behalf.

**The issue's own diagnostic does not reproduce on this machine, and the
record says so plainly.** Both of the issue's prescribed steps were re-run
here: Step 1 shows `Get-Command Get-FileHash` resolving to
`Microsoft.PowerShell.Utility`, `$PSModuleAutoLoadingPreference` unset
(default), and `$env:PSModulePath` carrying the System32 WindowsPowerShell
v1.0 Modules directory. Step 2 — the install's own invocation shape,
`powershell -NoProfile -ExecutionPolicy Bypass -Command "if (Get-Command
Get-FileHash -EA SilentlyContinue) {'found'} else {'NOT FOUND'}"` — printed
`found`. The issue's own text is explicit about what that result means: "say
so in the issue and stop; the fix will be different." Per that same text, the
certutil/.NET fallback design is scoped under a heading conditional on `NOT
FOUND`, which this machine did not produce.

**A real install attempt, run twice, found a different and reproducible
defect instead.** `herdr plugin install aliaksandr-haurylau-godel/herdr-voice
--yes` was run for real, twice. Both times `Get-FileHash` resolved and the
digest verified — i.e., the run got past `install.ps1:170` cleanly both
times — and both times failed deterministically two lines later, at line 180's
`tar -xzf $hvArchiveFile -C $tmp`, with:

```
tar (child): Cannot connect to C: resolve failed
gzip: stdin: unexpected end of file
/usr/bin/tar: Child returned status 128
/usr/bin/tar: Error is not recoverable: exiting now
herdr-voice: herdr-voice-v0.1.0-beta.2-x86_64-pc-windows-msvc.tar.gz was verified but could not be unpacked
```

Root cause, confirmed empirically in this session (not only theorized): `where
tar` and `(Get-Command tar -All).Source`, under both plain PowerShell 5.1 and
`pwsh` 7, resolve `C:\programs\PortableGit\usr\bin\tar.exe` — an MSYS/Cygwin
build — before `C:\Windows\System32\tar.exe`, the native bsdtar Windows has
shipped since 1803. MSYS tar treats a colon appearing before any slash in a
filename argument as a `[user@]host:file` remote-archive spec, so an absolute
Windows path such as `C:\Users\...\archive.tar.gz`, passed as the `-f`
argument (`-xzf $hvArchiveFile`), is parsed as "connect to host C" — exactly
the observed error. Reproduced directly in this session with a scratch
archive and the exact `-C <absolute> -czf <absolute>` shape `tar
-xzf $hvArchiveFile -C $tmp` uses; exit code 2, byte-for-byte the same stderr.

Also confirmed directly, by scratch experiment, both needed for S2's fix
shape:

- An absolute path given to `-C` (directory-change) does **not** trigger the
  bug — only the archive path bound to `-f`/`-czf`/`-xzf` does, and only when
  that path is absolute with a drive-letter colon.
- A **relative** filename as the `-f` argument, with the process's working
  directory already set to the right place (`Push-Location` before the call,
  `Pop-Location` after), sidesteps the bug entirely. Verified with a
  create-with-absolute-`-C`-plus-relative-`-f` round trip and a full
  create/extract round trip; both exit 0, both produce the correct file.

**The existing test harness has the identical bug, independently of the
install script.** `scripts/test-install.ps1:77` — `tar -C $stage -czf
$fakeArchive "herdr-voice-v0.4.2-$target"` — passes `$fakeArchive`, an
absolute path (`Join-Path $fixture 'archive.tar.gz'`), as the archive
argument, the same shape that fails in `install.ps1`. Confirmed by running
`scripts/test-install.ps1` directly on this machine: it fails building its
own fixture archive with the identical `Cannot connect to C: resolve failed`
error, before ever reaching the code under test. CI apparently does not hit
this — presumably its `PATH` does not put a Git-for-Windows MSYS `tar` ahead
of System32's — but any contributor with a typical local
PortableGit/Git-for-Windows install on Windows hits it running this suite
locally, which is itself a defect independent of whether #83's Windows
install bug is fixed.

**Scope, as already decided by the repository owner before this run began**
(recorded here, not re-litigated): fix the tar/PATH bug under issue #83
itself, reframing the ticket as "install.ps1 resolves a tool ambiguously via
PATH, and this is one manifestation of that — the Get-FileHash failure was
the first manifestation encountered, tar-vs-PATH is the one that is actually
live and reproducible now." The certutil/.NET `Get-FileHash` fallback the
issue's text pre-designs is **not** implemented — it is explicitly
conditional in the issue's own text on Step 2 saying `NOT FOUND`, and Step 2
said `found` here, so per the issue's own stated branching logic that fix is
out of scope for this run. `scripts/install.sh`/`scripts/test-install.sh`
(the `sh` path) is unaffected and out of bounds per the issue's own "Out of
bounds" section — not touched.

```yaml
gate:
  stage: S1
  artifact: AC_83.md
  reviewer: designer
  verdict: null
  date: null
```

```yaml
gate:
  stage: S1
  artifact: AC_83.md
  reviewer: designer (no project agent exists for this role; self-reviewed
    against the issue body, docs/design.md and the reproduction evidence
    above, per the tasks/36 precedent for a missing S1 skill extended to a
    missing S1 reviewer)
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

Self-review: the as-is/to-be/requirements below are traceable line for line
to either the issue body or a command actually run in this session; the
Get-FileHash non-reproduction is stated plainly rather than glossed over; the
scope boundary (tar/PATH in, certutil/.NET fallback out, `sh` path out)
matches the owner's decision and the issue's own conditional wording exactly.
No open question needs the owner before design can proceed — the fix shape is
already empirically verified in this session, which is a stronger basis than
S2 usually starts from.

S1 is closed. Next is S2 Design.

### S2 Design
- artifact: `DESIGN_83.md`
- produced: 2026-09-18

Written directly. Classified **bounded** under `superpowers:brainstorming` —
a well-scoped fix to two existing, already-read scripts, no new component.
Per this run's own precedent from #36's S2 (`tasks/36/DESIGN_36.md:1`),
substitutes the project's own gate (`octoflow-reviewer-planner`) for
brainstorming's live chat approval, since this session's user is not present
for one. `AC_83.md`'s empirical work (a real repro, a confirmed root cause,
a confirmed working fix shape, all run before design started) already
settled what a bounded design's clarifying questions would normally chase.

```yaml
gate:
  stage: S2
  artifact: DESIGN_83.md
  reviewer: planner
  verdict: null
  date: null
```

```yaml
gate:
  stage: S2
  artifact: DESIGN_83.md
  reviewer: octoflow-reviewer-planner
  verdict: QUESTIONS
  date: 2026-09-18
  questions:
    - "AC-2 asks for a test, and no section of the design produced one. The
       coverage table mapped AC-2 to §1 by inspection argument alone
       ('$tarExitCode is still checked, Stop-Hv still fires with the
       unchanged message'), and the only new test anywhere in the design was
       fixture-creation (AC-3), not extraction-failure (AC-2). AC_83.md's own
       'What S2 has to settle' reserved this shape to S2; writing the task
       without it would mean guessing how a corrupt-but-digest-matching
       archive reaches the extraction path through RunMain's fetch stub,
       which unconditionally copies $fakeArchive."
  blocker: null
  notes:
    - "Every line citation and quoted excerpt in the design verified against
       the real, unmodified scripts/install.ps1 and scripts/test-install.ps1.
       §1 composes correctly with the surrounding code; §4/§5's (then §3/§4's)
       ordering is coherent. Apart from the AC-2 gap, the design was
       plannable as written."
```

### Answered

Added a new §3 to `DESIGN_83.md`: a second, deliberately corrupt fixture
file (`corrupt.tar.gz`, not a valid gzip stream), a new `$TestArchiveFile`
override threaded through `RunMain`'s here-string (defaulting to
`$fakeArchive`, so every existing case is unaffected), and one new test case
— set `$TestArchiveFile` to the corrupt file, compute `$TestDigest` from its
own real hash so the digest check passes and the run reaches `tar`, assert
`RunMain` returns `1`, the stop message names "could not be unpacked", and
nothing is installed. This forces the exact failure AC-2 asks about,
post-fix, by actually running it rather than by inspecting the unchanged
`if` shape. Placed after the existing "a wrong digest stops" case, since the
two are the two ways `tar` can be reached with something unusable. The rest
of the document renumbered (§3→§4, old §4→§5, old §5→§6) and every internal
cross-reference (§4/§5's before/after pair, the coverage table) updated to
match — checked with `grep -n "^## "` against the file.

S2 continues; re-running the gate against the revised artifact.

```yaml
gate:
  stage: S2
  artifact: DESIGN_83.md
  reviewer: octoflow-reviewer-planner
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

Confirmed: the new §3 mechanism verified implementable against the real
`RunMain` structure, every citation re-checked against the unmodified
scripts, no existing test case disturbed, renumbering internally
consistent. Two cosmetic residues noted (a stale "§3 as originally drafted"
phrase now describing §4, and a `$TestNoCargo` reset-on-reuse analogy that
doesn't actually hold in the file) — neither blocks planning; the
instruction they support (restore `$TestArchiveFile` at the end of the new
case) is explicit and sufficient on its own, so left as-is per the
reviewer's own note that these cost nothing.

S2 is closed. Next is S3 Plan.

### S3 Plan
- artifact: `PLAN_83.md`

### S3 Plan
- artifact: `PLAN_83.md`
- produced: 2026-09-18

Written via `superpowers:writing-plans`, saved to `tasks/83/PLAN_83.md` per
`CLAUDE.md`'s run-root convention (overriding the skill's own default
`docs/superpowers/plans/` location, which the skill itself says user/project
preference overrides). Four tasks: fix `test-install.ps1`'s fixture creation
while witnessing the real crash first (1), fix `install.ps1`'s extraction
while witnessing the resulting red `a good release installs` case turn green
(2), add the corrupt-archive test that proves AC-2's `Stop-Hv` path still
fires post-fix (3), the S4 code-review gate (4). Tasks 1 and 2's "confirm the
starting state" steps use real output captured in this session
(`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-install.ps1`
against the unmodified scripts, on this machine), not predicted output.

```yaml
gate:
  stage: S3
  artifact: PLAN_83.md
  reviewer: implementer
  verdict: READY
  date: 2026-09-18
  questions: []
  blocker: null
```

First-pass READY. Every quoted old/new code block checked byte-for-byte
against the real files; composition with surrounding control flow (the
outer try/finally in both scripts, `RunMain`'s here-string escaping,
variable scope) confirmed correct. One non-blocking observation: the
design's §4/§5 name "PLAN_83.md's Task 5" for the red/green sequencing,
which the plan actually folds into Task 1/Task 2's own steps (4 tasks
total) — a stale reference in the design document only, not something the
plan itself needs to guess at.

S3 is closed. Next is S4 Implement.

### S4 Implement

Executed via `superpowers:executing-plans`, task by task, with the TDD
red/green cycle actually witnessed rather than assumed:

- **Task 1** (`scripts/test-install.ps1`'s fixture creation). Ran the
  unmodified suite first: it crashed with an uncaught `Resolve-Path`
  `ItemNotFoundException` from `Get-FileHash $fakeArchive` at `:78`, three
  lines after `tar`'s own `Cannot connect to C: resolve failed` at `:77` —
  the real, observed red state, more precise than the plan's prediction
  (the plan expected the crash to surface at the `tar` call itself; it
  actually surfaces two lines later, at `Get-FileHash`, since this
  machine's native-command stderr did not itself terminate the script the
  way `scripts/test-install.ps1:126-131`'s own comment describes — the
  terminating exception came from `Resolve-Path` finding no such file,
  not from `tar`'s stderr). Applied the fix. Re-ran: the crash was gone,
  `ok    the fixture archive was created` appeared, and the suite now
  reached `RunMain` and reported `FAIL  a good release installs` — plus one
  detail the plan's prediction did not name: a second uncaught crash from
  `Get-Content $installMarker` immediately after, since the still-broken
  `install.ps1` genuinely never wrote the marker file. Not a blocker: this
  crash is inherent to the still-unfixed `install.ps1` and cannot occur in
  the finished, committed diff, where Task 2's fix always lands together
  with Task 1's. Committed as `c1c5721`.
- **Task 2** (`scripts/install.ps1`'s extraction). Applied the fix. Re-ran
  the full suite: all 22 lines `ok`, including `a good release installs`,
  `the binary landed at target\release\herdr-voice.exe` and
  `it recorded that it fetched and verified` — the marker-read crash from
  Task 1's red state did not recur, confirming it really was downstream of
  the bug this task fixes. Committed as `8b82a3b`.
- **Task 3** (the corrupt-archive test for AC-2). Sanity-checked the corrupt
  fixture by hand first, twice: once with an absolute path (which hit the
  PATH bug itself, `exit 2`, "Cannot connect to C" — the wrong reason, and
  not what Task 3's own case needed to prove), then with a relative filename
  and the working directory set the same way the now-fixed `install.ps1`
  calls `tar` (`exit 2`, "gzip: stdin: not in gzip format" — the right
  reason, content corruption, not a PATH artifact). Wired
  `$TestArchiveFile` through `RunMain` and added the new case. Ran the full
  suite: all 26 lines `ok`, including the three new ones
  (`a corrupt archive fails to unpack`, `it named the archive as unpacked`,
  `a corrupt archive is not installed`). Committed as `5a5c5e6`.

A stray `bash.exe.stackdump` (an MSYS crash artifact from an earlier
scratch `tar` invocation in this session, unrelated to any tracked file)
appeared in the working tree during Task 3 and was deleted before staging;
`git status --short` was clean of it before every commit above.

Each of the three commits was verified individually: `git diff` for each
was reviewed before staging to confirm it contained exactly its own task's
change and nothing from a neighboring task (Task 1's commit was
reconstructed by reverting `scripts/test-install.ps1` to its committed
original and reapplying only Task 1's edit; Task 3's edit was then
reapplied on top and diffed byte-for-byte against the original combined
edit to confirm no drift).

## Gate S4

```yaml
gate:
  stage: S4
  artifact: the diff on fix/83-windows-install-tar since d83570c
    (c1c5721, 8b82a3b, 5a5c5e6)
  reviewer: general-purpose subagent, per superpowers:requesting-code-review
  verdict: With fixes (process only, not code)
  date: 2026-09-18
```

No Critical or code-level Important issues. The reviewer independently
verified the work rather than trusting the commit messages: ran
`scripts/test-install.ps1` under both PowerShell 5.1 and `pwsh` 7 on this
machine (27/27 `ok`, exit 0 both times), and separately reproduced both the
original bug and the fix's correctness by invoking the MSYS `tar` binary
directly by hand (absolute path fails with the documented error, relative
path plus `Push-Location`/`Pop-Location` succeeds) — independent of
whether the automated suite's own PATH happens to resolve MSYS `tar` first.
Confirmed each of the three commits touches only its own task's lines, `git
diff -- scripts/install.sh scripts/test-install.sh` is empty (AC-6), and no
leak-gate-relevant content is in the diff. The reviewer also noted and
cleaned up its own stray `bash.exe.stackdump` from a manual MSYS-tar
reproduction it ran — the same class of artifact this run's own S4 notes
above already flagged and removed once.

One Important finding, both parts already known and neither a code defect:
S5 had not yet been recorded (true at review time — this run's next
section closes it), and the automated suite's own regression guard for
AC-5 is only as strong as whichever machine runs it, since CI's `PATH`
order (and, the reviewer found, this review session's own `PATH` order —
`where.exe tar` resolves System32's native `tar` first there) may never
actually exercise the MSYS-tar branch of the bug at all. `AC_83.md` and
`DESIGN_83.md` already state this limitation; the reviewer asked that
`docs/evidence.md`'s S5 entry name which `tar` resolved first on whatever
machine records it, so "the suite passed" is unambiguous. Applied below.

S4 is closed. Next is S5 Verify.

### S5 Verify
- artifact: `docs/evidence.md`, "The Windows install script's tar/PATH fix, for issue #83"
- produced: 2026-09-18

Run via `superpowers:verification-before-completion` — every command below
was actually executed in this session, fresh, with full output read before
any claim was made.

```yaml
gate:
  stage: S5
  artifact: docs/evidence.md, "The Windows install script's tar/PATH fix,
    for issue #83"
  verdict: pass, with one gate genuinely inapplicable and said so
  date: 2026-09-18
  platform: Windows 11 Home, the same machine issue #83 was filed against
    ($PSVersionTable.PSVersion 5.1.26100.9444)
```

- `cargo test` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt
  --check`: **not run.** This machine has no Rust toolchain at all —
  confirmed by `cargo`/`rustc` resolving to nothing on `PATH` and neither
  `~/.cargo` nor `~/.rustup` existing, matching `docs/evidence.md`'s own
  pre-existing "the `x86_64-pc-windows-msvc` target is not installed on the
  development machine and `rustup` is absent." Not glossed over: this
  issue's full diff (`git diff --stat d83570c..HEAD`) touches exactly two
  files, both `scripts/*.ps1`, zero `.rs` files, so these three gates are
  inapplicable to this diff by content, not merely unrun on this machine.
- `python3 scripts/check_manifest.py`: run, passed —
  `manifest: 12 entries, all commands known`, exit 0.
- `scripts/test-install.ps1` under Windows PowerShell 5.1: run fresh, 26/26
  `Check`/`CheckContains` lines `ok`, `all install.ps1 assertions passed`,
  exit 0.
- `scripts/test-install.ps1` under `pwsh` 7: run fresh, identical — 26/26
  `ok`, exit 0.
- Re-confirmed, at the moment of this record, which `tar` resolves first on
  this machine under both interpreters (per the S4 reviewer's request,
  since its own session found the opposite ordering): PortableGit's MSYS
  `tar` before `System32`'s native one, both interpreters agreeing — so
  this machine's suite run genuinely exercises the fix against the `tar`
  implementation the bug is about, not a pass that would hold trivially
  either way.

Full detail, the root-cause reproduction independent of the suite, and what
this entry does not establish are in `docs/evidence.md` itself, not
duplicated here.

S5 is closed. Next: the pull request for #83.
