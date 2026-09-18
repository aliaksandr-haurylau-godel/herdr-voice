# DESIGN_83

Design for issue #83, against `tasks/83/AC_83.md` (gate S1: READY,
`tasks/83/RUN_83.md`).

Classified **bounded** under `superpowers:brainstorming` — a well-scoped fix
to two existing PowerShell scripts, both already read in full, with no new
component or interface. `AC_83.md`'s empirical work (a real repro, a
confirmed root cause, a confirmed working fix shape, all run in this
session) already settles what a bounded design's clarifying questions would
normally chase. Per this run's own precedent from #36's S2
(`tasks/36/DESIGN_36.md:1`, "async/overnight workflow substitutes the
project's own gate — planner via `octoflow-reviewer-planner` — for
`superpowers:brainstorming`'s live chat approval"), the same substitution
applies here: this session's user is not present for a live approval
exchange, so the design is written directly and gated by
`octoflow-reviewer-planner` rather than presented in chat and approved with
a nod. Every decision below is an engineering choice `CLAUDE.md` already
delegates, not a scope, naming or spend decision reserved for the owner —
the scope boundary itself is already fixed by `AC_83.md`'s "Resolved before
this run began."

## 1. `scripts/install.ps1`: extract with cwd set, filename relative

Replace the current call (`:180-184`):

```powershell
tar -xzf $hvArchiveFile -C $tmp
if ($LASTEXITCODE -ne 0) {
    Stop-Hv "$archive was verified but could not be unpacked" `
            'the archive is not a readable gzip tarball; report it against the release.'
}
```

with:

```powershell
Push-Location $tmp
try {
    tar -xzf $archive
    $tarExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
if ($tarExitCode -ne 0) {
    Stop-Hv "$archive was verified but could not be unpacked" `
            'the archive is not a readable gzip tarball; report it against the release.'
}
```

`$archive` (`:128`, `"$HvName-$tag-$target.tar.gz"`) is already a bare
filename with no directory component — it is `$hvArchiveFile` that is
absolute (`:141`, `Join-Path $tmp $archive`). With the working directory set
to `$tmp` before the call, the archive sits at `$tmp\$archive` and the same
bare `$archive` reaches `tar` as its `-f` argument, so `-C $tmp` is no
longer needed at all (the working directory already is `$tmp`) — dropped
rather than kept redundant.

`$LASTEXITCODE` is captured into `$tarExitCode` immediately after the `tar`
call, before `Pop-Location` runs, per `AC_83.md`'s own "what S2 has to
settle." `$LASTEXITCODE` is a script-scope variable set by the last native
command; `Pop-Location` is a cmdlet, not a native command, so in practice it
does not disturb `$LASTEXITCODE`. The capture is done anyway, immediately
and explicitly, because the existing code already depends on reading
`$LASTEXITCODE` right after the command that set it (the pattern the
`Invoke-HvFallback`/`cargo build` call already establishes at `:104-105`),
and relying on an implicit "cmdlets don't touch it" fact one statement later
than necessary is a needless risk to take with no PowerShell version
guarantee behind it.

This nested `Push-Location`/`try`/`finally`/`Pop-Location` sits inside the
existing outer `try { ... } finally { Remove-Item -Recurse -Force $tmp
-ErrorAction SilentlyContinue }` block (`:135-198`) unchanged — it composes
safely because `Stop-Hv`'s `exit 1` is only ever reached (via the `if
($tarExitCode -ne 0)` check) after `Pop-Location` has already run, so the
process's working directory is restored before any exit path, and the outer
`finally`'s `Remove-Item` still runs regardless of which location the
process is in when it fires.

## 2. `scripts/test-install.ps1`: build the fixture the same way

Replace the fixture-building call (`:76-77`):

```powershell
$fakeArchive = Join-Path $fixture 'archive.tar.gz'
tar -C $stage -czf $fakeArchive "herdr-voice-v0.4.2-$target"
```

with:

```powershell
$fakeArchiveName = 'archive.tar.gz'
$fakeArchive = Join-Path $fixture $fakeArchiveName
Push-Location $fixture
try {
    tar -C $stage -czf $fakeArchiveName "herdr-voice-v0.4.2-$target"
    $fixtureTarExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
Check 'the fixture archive was created' ($fixtureTarExitCode -eq 0 -and (Test-Path $fakeArchive)) $true
```

`$fakeArchive` stays the absolute path and is unchanged everywhere else in
the file — the `Get-FileHash $fakeArchive` call (`:78`) and the
`Copy-Item $fakeArchive $Destination -Force` inside `RunMain`'s fetch stub
(`:115`) both keep reading the same real file by its absolute path; nothing
downstream needs to know the archive was created with a relative filename.
Per `AC_83.md`'s "what S2 has to settle," this is the narrower of the two
options offered: special-case only the `tar` invocation's own
directory/filename handling, thread nothing relative any further. Reason:
`Get-FileHash` and `Copy-Item` are ordinary PowerShell cmdlets, not the
external `tar` binary — neither of them has any colon-in-a-path ambiguity,
so widening the relative-path treatment to them would fix nothing and would
only add a second working-directory dependency for no benefit.

`-C $stage` is left exactly as it is. `AC_83.md`'s own confirmed-safe finding
covers it directly: an absolute path given to `-C` does not trigger the bug,
only the archive path bound to `-f`/`-czf`/`-xzf` does.

**The new `Check` line is a real addition, not incidental.** Today, nothing
in `test-install.ps1` checks that the fixture-building step actually
succeeded — a failure there currently surfaces as an uncaught, unrelated
crash further down the script (`Get-FileHash` on a file that does not
exist, or, on Windows PowerShell 5.1, `tar`'s own stderr being turned into a
terminating error by `$ErrorActionPreference = 'Stop'`, per the comment
already at `:126-131`). Both are confusing failures with no clear cause
printed in the suite's own `ok`/`FAIL` vocabulary. The new line makes a
fixture-creation failure report through the same mechanism every other
assertion in this file already uses, which is this project's own standard
for a user-visible failure naming what happened rather than surfacing as an
unrelated crash three statements later.

## 3. A forced extraction failure, so AC-2 has a real test

AC-2 needs a test that forces `tar` to fail *after* the fix, and asserts the
existing `Stop-Hv "$archive was verified but could not be unpacked"` path
still fires — §1's fix must not have quietly broken that check while
removing `-C`. Neither §1 nor §2 produces this on their own; §3 as
originally drafted only argued AC-5 needs no *further* test, which is a
different claim and does not cover AC-2.

`RunMain`'s fetch stub (`:108-117`) always copies the same `$fakeArchive`
into the archive destination when `$TestArchive -eq 'ok'`. To force a
genuine extraction failure — not a digest mismatch, which is already covered
by the existing "a wrong digest stops" case and never reaches `tar` at all —
a second, deliberately corrupt fixture file is needed, plus a way for
`RunMain`'s stub to copy *that* file instead of `$fakeArchive`, while still
answering the digest check with the corrupt file's own real hash so the run
reaches the `tar` call at all.

Add, next to `$fakeArchive`'s own construction:

```powershell
$corruptArchive = Join-Path $fixture 'corrupt.tar.gz'
'this is not a gzip tarball' | Set-Content -Path $corruptArchive
```

Add one new override variable to `RunMain`'s here-string, alongside the four
that already exist (`$TestArchive`, `$TestSidecar`, `$TestDigest`,
`$TestNoCargo`):

```powershell
`$TestArchiveFile = '$script:TestArchiveFile'
```

and change the fetch override's archive branch from the unconditional
`Copy-Item $fakeArchive $Destination -Force` to
`Copy-Item $TestArchiveFile $Destination -Force`. `$script:TestArchiveFile`
defaults to `$fakeArchive` (set once, alongside `$TestNoCargo = '0'` at
`:143`), so every existing case is unaffected without itself being edited —
each existing `Check`/`RunMain` pair already only sets the variables it
cares about and leaves the rest at whatever the previous case left them, and
this one follows the same convention.

The new case, placed after the existing "a wrong digest stops" case (they
are the two ways `tar` can be reached with something it cannot use — a
digest that will not match, and an archive that is not a valid gzip
tarball — and reading them together is clearer than separating them):

```powershell
# An archive that passes its own digest check but is not a real gzip
# tarball still stops the install, after the fix removed -C from the tar
# call — the check that used to read $LASTEXITCODE right after `-C $tmp`
# must still read it right after the relative-filename call.
Remove-Item -Recurse -Force $targetDir -ErrorAction SilentlyContinue
$TestArchive = 'ok'; $TestSidecar = 'ok'
$TestArchiveFile = $corruptArchive
$TestDigest = (Get-FileHash $corruptArchive -Algorithm SHA256).Hash.ToLower()
Check 'a corrupt archive fails to unpack' (RunMain) 1
CheckContains 'it named the archive as unpacked' $LastSaid 'could not be unpacked'
if (Test-Path $installedExe) {
    Write-Host 'FAIL  a corrupt archive was reported as installed'; $failures++
} else { Write-Host 'ok    a corrupt archive is not installed' }
$TestArchiveFile = $fakeArchive
```

The final `$TestArchiveFile = $fakeArchive` restores the default before the
next case runs, the same discipline `$TestNoCargo`'s reset-on-reuse already
follows for the cases after it.

This case is independent of §5's red/green PATH-ordering sequencing: it
exercises "extraction fails and is reported correctly," a property that
holds regardless of which `tar` `PATH` resolves, since a string that is not
gzip at all fails identically under the native `bsdtar` and under MSYS
`tar`. It only needs to exist post-fix, once `$tarExitCode` is the value the
`if` checks — pre-fix, this case would already fail for the wrong reason
(the PATH bug, not a genuinely corrupt archive), so it is not part of §5's
before/after pair and is not run until §1's fix is in place.

## 4. What demonstrates the fix, and why no further new test is needed

`AC-5` asks for a test that fails against the pre-fix shape and passes
against the post-fix shape, driving the real extraction logic. The design
here deliberately does not add a second, separate assertion for that:
the pre-existing `Check 'a good release installs' (RunMain) 0` and its two
immediate follow-ups (`:148-153`, that the binary landed and the marker
records "verified sha256") already do this, once §2's fix makes the suite
able to reach them at all.

`RunMain` (`:94-141`) overrides `Get-HvRoot` and `Invoke-HvFetch` only — it
does **not** stub `tar`, `Get-FileHash`, or any part of the extraction path.
Every call to `RunMain` therefore drives the real, compiled
`Invoke-HvMain`, including its real `tar -xzf ...` call, against whichever
`tar` this machine's `PATH` actually resolves — the same PortableGit MSYS
`tar` this session confirmed sits ahead of `System32`'s. So:

- Before §1's fix (but after §2's, so the suite can reach this point at
  all): `Invoke-HvMain`'s `tar -xzf $hvArchiveFile -C $tmp` call hits the
  identical `Cannot connect to C: resolve failed` failure this session
  reproduced directly, `$LASTEXITCODE` comes back nonzero,
  `Stop-Hv` fires, and `RunMain` returns `1` — the `Check 'a good release
  installs' (RunMain) 0` assertion fails. **This is confirmed by actually
  running it**, not assumed — Task 5 of `PLAN_83.md` requires this exact run
  before §1's fix is applied, so the "red" half of the red/green pair is
  witnessed, not inferred from the reasoning above.
- After §1's fix: the same call succeeds, `RunMain` returns `0`, the binary
  lands at the expected path, and the marker records the digest — the
  existing assertion turns green for the first time it has ever meant
  anything on a machine with this `PATH` ordering.

This is a stronger regression guard than a new, narrower assertion would be:
it is a full, unmocked, end-to-end run of the real production code path on
the real problematic `PATH`, not a synthetic check of one isolated call. A
second, separate assertion duplicating exactly what this one already proves
would be the kind of redundant test this project's own preference for
following existing patterns over speculative addition argues against
(mirrors `DESIGN_36.md`'s §7 reasoning for not adding a shared templating
helper two similar call sites do not yet justify).

## 5. Order of changes, so the TDD cycle is real

Both fixes are needed before the suite can run to completion at all (§2's
fix is a prerequisite for even reaching §1's code under test), so the
red/green pair in §4 needs a specific sequencing to be witnessed rather than
assumed:

1. Apply §2's fix alone (fixture creation). Run `scripts/test-install.ps1`.
   Confirm: the suite now reaches `RunMain`, and `Check 'a good release
   installs' (RunMain) 0` **fails** — the "red" state, driven by the real,
   still-unfixed `install.ps1`.
2. Apply §1's fix (extraction). Re-run the same suite. Confirm: every
   assertion, including the one that just failed, passes.

This sequencing is `PLAN_83.md`'s Task 5, written as two explicit steps
rather than one, specifically so the red state is observed and not merely
argued for.

## 6. Coverage against AC_83.md

| AC | Covered by |
|---|---|
| AC-1 | §1 |
| AC-2 | §1 (`$tarExitCode` is still checked, `Stop-Hv` still fires with the unchanged message), §3 (the corrupt-archive case proves it by actually forcing the failure post-fix, not by inspection alone) |
| AC-3 | §2 |
| AC-4 | §2 (`$fakeArchive` unchanged for every other use) |
| AC-5 | §4, §5 |
| AC-6 | Not touched by this design at all — `scripts/install.sh`/`scripts/test-install.sh` appear nowhere above |
| AC-7 | Not a design concern — `PLAN_83.md`'s S5 task schedules the four existing gates plus both PowerShell interpreters, the same shape `tasks/36/PLAN_36.md`'s S5 task used |

## What this design does not decide

The exact prose of the new `Check` line's label (§2) and whether
`$fixtureTarExitCode` or an inline expression reads more clearly at the call
site are left to implementation — both are mechanical once this contract is
fixed.
