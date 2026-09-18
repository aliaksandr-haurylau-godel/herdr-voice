# Windows install tar/PATH fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `scripts/install.ps1` and `scripts/test-install.ps1` so their
`tar` calls extract/create archives without depending on which `tar`
implementation `PATH` resolves first — the defect this run found while
reproducing issue #83's filed `Get-FileHash` symptom, which did not
reproduce on this machine.

**Architecture:** No new component. Two existing PowerShell scripts each
gain a `Push-Location`/`try`/`finally`/`Pop-Location` block around their one
`tar` call, so the call receives a bare relative filename instead of an
absolute Windows path with a drive-letter colon — the exact shape MSYS tar
misreads as a remote-archive host spec. One new test case in
`scripts/test-install.ps1` proves the existing `Stop-Hv` failure path still
fires after the change, using a second, deliberately corrupt fixture file.

**Tech Stack:** Windows PowerShell 5.1 / PowerShell 7 (`pwsh`), native `tar`
(bsdtar or MSYS, whichever `PATH` resolves), no new dependency.

**Spec:** `tasks/83/DESIGN_83.md`, against `tasks/83/AC_83.md` (S1: READY,
S2: READY, `tasks/83/RUN_83.md`).

## Global Constraints

- `scripts/install.sh` and `scripts/test-install.sh` are not touched by any
  task in this plan (`AC_83.md`, AC-6; out of bounds per the issue itself).
- No Rust source is touched by any task in this plan.
- Every failure message a person can see must keep naming what to do next
  (`CLAUDE.md`, "Rules for the code") — this plan changes no existing
  message text, only where `$LASTEXITCODE` is read from and which fixture
  files exist.
- `scripts/test-install.ps1`'s existing `Check`/`CheckContains` helper
  functions and `RunMain` child-process pattern (`scripts/test-install.ps1:12-19`,
  `:94-141`) are the only test tooling available — no new test framework.

---

### Task 1: Fix `scripts/test-install.ps1`'s fixture creation, witnessing the real red state first

**Files:**
- Modify: `scripts/test-install.ps1:76-78`
- Modify: `scripts/test-install.ps1:143` (default assignment)

**Interfaces:**
- Consumes: nothing from another task in this plan.
- Produces: `$fakeArchive` (absolute path, unchanged in every other use in
  the file — `Get-FileHash $fakeArchive` at the line after this block, and
  `Copy-Item $TestArchiveFile $Destination` inside `RunMain`'s fetch stub,
  wired in Task 3), `$fixtureTarExitCode` (local to the setup block, not
  read elsewhere), and the `Check 'the fixture archive was created' ...`
  line's pass/fail as the new suite output.

- [ ] **Step 1: Run the suite as it is today and record the crash**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-install.ps1`

Expected (confirmed by running it in this session, on this machine, before
any change): six `ok` lines (parse, target mapping ×2, manifest version
×2), then

```
tar (child): Cannot connect to C: resolve failed
/usr/bin/tar: Child returned status 128
/usr/bin/tar: Error is not recoverable: exiting now
Resolve-Path : Cannot find path
'<...>\archive.tar.gz' because it does not exist.
```

followed by a `Resolve-Path`/`ItemNotFoundException` stack trace, and the
process exits `1`. This is the "red" state this task starts from: `tar` at
`:77` fails to create `$fakeArchive` because it is called with `$fakeArchive`
as an absolute path (`Join-Path $fixture 'archive.tar.gz'`), and the crash
actually surfaces two lines later, when `Get-FileHash $fakeArchive` at `:78`
calls `Resolve-Path` on a file that was never created — not a clean `Check`
`FAIL` line, but an uncaught terminating exception that aborts the whole
script before any of the suite's own assertions about the real defect can
run.

- [ ] **Step 2: Apply the fix**

Replace `scripts/test-install.ps1:76-77`:

```powershell
$fakeArchive = Join-Path $fixture 'archive.tar.gz'
tar -C $stage -czf $fakeArchive "herdr-voice-v0.4.2-$target"
```

with:

```powershell
$fakeArchiveName = 'archive.tar.gz'
$fakeArchive = Join-Path $fixture $fakeArchiveName
# Built with the working directory set to $fixture and a bare relative
# filename, not $fakeArchive's absolute path - the same reason
# scripts/install.ps1's own extraction avoids one; see its comment in
# Invoke-HvMain. -C $stage is unaffected: it is a directory-change argument,
# not the archive path, and is not what tar reads as a remote-archive spec.
Push-Location $fixture
try {
    tar -C $stage -czf $fakeArchiveName "herdr-voice-v0.4.2-$target"
    $fixtureTarExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
Check 'the fixture archive was created' ($fixtureTarExitCode -eq 0 -and (Test-Path $fakeArchive)) $true
```

`scripts/test-install.ps1:78` (`$goodDigest = (Get-FileHash $fakeArchive -Algorithm SHA256).Hash.ToLower()`)
is unchanged — `$fakeArchive` still resolves to the same absolute path, now
pointing at a file that actually exists.

Also add the corrupt-fixture file used by Task 3, created here so both
fixture files are built together, right after the line above (still inside
the same `try` block that already wraps the fixture section, before the
`$checkout`/`Copy-Item` block that follows):

```powershell
$corruptArchive = Join-Path $fixture 'corrupt.tar.gz'
'this is not a gzip tarball' | Set-Content -Path $corruptArchive
```

And add the default assignment at `scripts/test-install.ps1:143`, next to
the existing `$TestNoCargo = '0'`:

```powershell
$TestNoCargo = '0'
$TestArchiveFile = $fakeArchive
```

(`$TestArchiveFile` is not consumed by anything yet — `RunMain`'s here-string
and fetch stub are wired to use it in Task 3. Until then it is an unused
variable, which is fine for this step: `$ErrorActionPreference = 'Stop'`
does not fail on an unused variable, and Task 1's own goal is only the
fixture-creation fix and its `Check` line.)

- [ ] **Step 3: Re-run the suite and confirm the new red state is the right one**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-install.ps1`

Expected: the crash from Step 1 is gone. `ok    the fixture archive was created`
appears. The suite now reaches `RunMain` and prints
`FAIL  a good release installs: got [1], expected [0]` (or equivalent) —
this is `install.ps1`'s own tar/PATH bug, now reached and reported cleanly by
the suite's own `Check` mechanism instead of crashing before it could be
observed at all. The overall script still exits `1` (the suite now runs to
completion and reports the count of failures, per `scripts/test-install.ps1:196-198`).
This is the expected, documented state at the end of this task — the fix for
this failure is Task 2, not this one.

- [ ] **Step 4: Commit**

```bash
git add scripts/test-install.ps1
git commit -m "fix: build the install test fixture with a relative tar path

MSYS tar reads an absolute Windows path passed as the archive argument as a
[user@]host:file remote-archive spec, so building the fixture archive with
an absolute path crashes the suite outright on a machine where PATH
resolves an MSYS tar ahead of the native one. Build it with the working
directory set to the fixture folder and a relative filename instead, and
check the step actually succeeded rather than letting a failure surface as
an unrelated Get-FileHash crash three lines later."
```

---

### Task 2: Fix `scripts/install.ps1`'s extraction call

**Files:**
- Modify: `scripts/install.ps1:176-184`

**Interfaces:**
- Consumes: nothing from Task 1 directly, but Task 1 must already be applied
  for the suite to reach this code path at all (`scripts/test-install.ps1`
  crashes before `RunMain` runs otherwise).
- Produces: `$tarExitCode` (local to `Invoke-HvMain`, read once by the `if`
  immediately after), the corrected extraction behavior every later task's
  test run depends on.

- [ ] **Step 1: Confirm the starting state**

This is the `FAIL  a good release installs` state Task 1 Step 3 already
produced and recorded — no separate run is needed to re-confirm it; proceed
directly to the fix.

- [ ] **Step 2: Apply the fix**

Replace `scripts/install.ps1:176-184`:

```powershell
    # Checked rather than left to fail on its own. tar is a native command, so
    # $ErrorActionPreference does not catch it, and without this a verified
    # download that unpacks badly ends on an uncaught Copy-Item error with no
    # statement that nothing was installed and nothing to do next.
    tar -xzf $hvArchiveFile -C $tmp
    if ($LASTEXITCODE -ne 0) {
        Stop-Hv "$archive was verified but could not be unpacked" `
                'the archive is not a readable gzip tarball; report it against the release.'
    }
```

with:

```powershell
    # Checked rather than left to fail on its own. tar is a native command, so
    # $ErrorActionPreference does not catch it, and without this a verified
    # download that unpacks badly ends on an uncaught Copy-Item error with no
    # statement that nothing was installed and nothing to do next.
    #
    # Extracted with the working directory set to $tmp and a bare relative
    # filename, not $hvArchiveFile's absolute path: an absolute Windows path
    # passed to tar's -f is read by an MSYS/Cygwin tar - the one a typical Git
    # for Windows install puts ahead of the native one on PATH - as a
    # [user@]host:file remote-archive spec, and "C:\..." becomes "connect to
    # host C". A relative filename with the right working directory is
    # correct whichever tar resolves, so -C is no longer needed either.
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

`$archive` (`scripts/install.ps1:128`, `"$HvName-$tag-$target.tar.gz"`) is
already a bare filename with no directory component — nothing about its
definition changes.

- [ ] **Step 3: Re-run the suite and confirm the fix**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-install.ps1`

Expected: `ok    a good release installs`, `ok    the binary landed at target\release\herdr-voice.exe`,
`ok    it recorded that it fetched and verified`. Every other pre-existing
`Check`/`CheckContains` line still reports `ok` (the uppercase-digest,
wrong-digest, missing-digest, unreachable-archive and no-archive-no-cargo
cases — none of them reach the `tar` call at all, since they all stop before
it, so none of them could have been affected by this change, and this run
confirms none was). The script's own final line
(`all install.ps1 assertions passed`) appears and it exits `0`.

- [ ] **Step 4: Commit**

```bash
git add scripts/install.ps1
git commit -m "fix: extract the install archive with a relative tar path

Same defect as the fixture-building fix: tar -xzf \$hvArchiveFile -C \$tmp
passes an absolute Windows path as the archive argument, which an MSYS tar
ahead of the native one on PATH misreads as a remote-archive host spec
('Cannot connect to C: resolve failed'). Extract with the working directory
already set to \$tmp and a bare relative filename instead, so -C is no
longer needed and the call is correct regardless of which tar resolves.
Reproduced directly on a machine with a typical Git for Windows install,
where this is what issue #83's filed Get-FileHash symptom turned out to
actually be once the issue's own diagnostic ruled out the filed cause."
```

---

### Task 3: Prove the existing unpack-failure message still fires (AC-2)

**Files:**
- Modify: `scripts/test-install.ps1:98-121` (`RunMain`'s here-string and fetch
  stub)
- Modify: `scripts/test-install.ps1:143` (already has `$TestArchiveFile =
  $fakeArchive` from Task 1; no change needed here beyond what Task 1 added)
- Modify: `scripts/test-install.ps1:160-167` area (insert the new case after
  "a wrong digest stops")

**Interfaces:**
- Consumes: `$corruptArchive` (created in Task 1, Step 2 — an absolute path
  to a file that is not a valid gzip stream), `$fakeArchive`,
  `$installedExe`, `$LastSaid`, `$failures` (all pre-existing script-scope
  variables `RunMain`'s test cases already use), `RunMain` itself (Task 1's
  and Task 2's own fixes did not change its signature).
- Produces: nothing later tasks in this plan consume — this is the last
  code-change task.

- [ ] **Step 1: Sanity-check the corrupt fixture actually fails under `tar`**

Run, by hand, once, to confirm the fixture is genuinely corrupt and not
spuriously readable (not part of the automated suite — a one-time check
before wiring it in):

```powershell
powershell -NoProfile -Command "'this is not a gzip tarball' | Set-Content -Path \$env:TEMP\corrupt-check.tar.gz; tar -xzf \$env:TEMP\corrupt-check.tar.gz -C \$env:TEMP; Write-Host \"exit: \$LASTEXITCODE\"; Remove-Item \$env:TEMP\corrupt-check.tar.gz"
```

Expected: `tar` reports it is not in gzip format (or an equivalent decode
error) and `exit:` is nonzero. If this ever printed `exit: 0`, the fixture
content would need to change before proceeding — record what was observed.

- [ ] **Step 2: Wire `$TestArchiveFile` through `RunMain`**

Replace `scripts/test-install.ps1:98-121`:

```powershell
    $overrides = @"
`$env:HERDR_VOICE_INSTALL_LIB = '1'
. '$scriptPath'
`$checkout = '$checkout'
`$fakeArchive = '$fakeArchive'
`$TestArchive = '$script:TestArchive'
`$TestSidecar = '$script:TestSidecar'
`$TestDigest = '$script:TestDigest'
`$TestNoCargo = '$script:TestNoCargo'
function Get-HvRoot { `$checkout }
function Invoke-HvFetch([string]`$Url, [string]`$Destination) {
    if (`$Destination -like '*.sha256') {
        if (`$TestSidecar -ne 'ok') { return `$TestSidecar }
        Set-Content -Path `$Destination -Value "`$TestDigest  archive.tar.gz"
        return 'ok'
    }
    if (`$TestArchive -ne 'ok') { return `$TestArchive }
    Copy-Item `$fakeArchive `$Destination -Force
    return 'ok'
}
if (`$TestNoCargo -eq '1') { function Test-HvCargo { `$false } }
`$env:HERDR_VOICE_INSTALL_LIB = '0'
Invoke-HvMain
"@
```

with:

```powershell
    $overrides = @"
`$env:HERDR_VOICE_INSTALL_LIB = '1'
. '$scriptPath'
`$checkout = '$checkout'
`$fakeArchive = '$fakeArchive'
`$TestArchive = '$script:TestArchive'
`$TestSidecar = '$script:TestSidecar'
`$TestDigest = '$script:TestDigest'
`$TestNoCargo = '$script:TestNoCargo'
`$TestArchiveFile = '$script:TestArchiveFile'
function Get-HvRoot { `$checkout }
function Invoke-HvFetch([string]`$Url, [string]`$Destination) {
    if (`$Destination -like '*.sha256') {
        if (`$TestSidecar -ne 'ok') { return `$TestSidecar }
        Set-Content -Path `$Destination -Value "`$TestDigest  archive.tar.gz"
        return 'ok'
    }
    if (`$TestArchive -ne 'ok') { return `$TestArchive }
    Copy-Item `$TestArchiveFile `$Destination -Force
    return 'ok'
}
if (`$TestNoCargo -eq '1') { function Test-HvCargo { `$false } }
`$env:HERDR_VOICE_INSTALL_LIB = '0'
Invoke-HvMain
"@
```

Only two lines change: the new `` `$TestArchiveFile = '$script:TestArchiveFile' ``
line added after `` `$TestNoCargo = '$script:TestNoCargo' ``, and
`Copy-Item `$fakeArchive `$Destination -Force` changed to
`Copy-Item `$TestArchiveFile `$Destination -Force`. `$script:TestArchiveFile`
already defaults to `$fakeArchive` from Task 1 Step 2's addition at `:143`,
so every existing case (good path, uppercase digest, wrong digest, missing
digest, unreachable archive, no-archive-no-cargo) is unaffected — none of
them sets `$TestArchiveFile` themselves, so they all keep copying
`$fakeArchive`, exactly as before this task.

- [ ] **Step 3: Add the new test case**

Insert, in `scripts/test-install.ps1`, immediately after the existing block:

```powershell
# A digest that disagrees stops, and nothing is unpacked.
Remove-Item -Recurse -Force $targetDir -ErrorAction SilentlyContinue
$TestDigest = '0000000000000000000000000000000000000000000000000000000000000000'
Check 'a wrong digest stops' (RunMain) 1
CheckContains 'it named the expected digest' $LastSaid '0000000000000000'
if (Test-Path $installedExe) {
    Write-Host 'FAIL  a mismatched archive was unpacked anyway'; $failures++
} else { Write-Host 'ok    a mismatched archive is not unpacked' }
```

and before the existing block:

```powershell
# An archive whose digest is absent stops without unpacking.
```

add:

```powershell
# An archive that passes its own digest check but is not a real gzip
# tarball still stops the install, after the fix removed -C from the tar
# call - the check that used to read $LASTEXITCODE right after `-C $tmp`
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

The final `$TestArchiveFile = $fakeArchive` line restores the default before
the next case (`$TestArchive = 'ok'; $TestSidecar = 'missing'; ...`, the
missing-digest case) runs, so it and every case after it keep copying
`$fakeArchive` exactly as they do today.

- [ ] **Step 4: Run the full suite and confirm every case passes**

Run: `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-install.ps1`

Expected: every line reports `ok`, including the new
`ok    a corrupt archive fails to unpack`,
`ok    it named the archive as unpacked`, and
`ok    a corrupt archive is not installed`. The final line reads
`all install.ps1 assertions passed` and the script exits `0`.

- [ ] **Step 5: Commit**

```bash
git add scripts/test-install.ps1
git commit -m "test: prove the unpack-failure message still fires after the fix

Add a second, deliberately corrupt fixture archive and a \$TestArchiveFile
override so a test case can force RunMain's real Invoke-HvMain to reach a
genuine tar failure - not a digest mismatch, which the existing case already
covers and never reaches tar at all - and confirm Stop-Hv's existing
'could not be unpacked' message and exit code still fire now that the
extraction call no longer uses -C."
```

---

### Task 4: Code review gate (S4 close)

Not a code task — this is where `CLAUDE.md`'s rule "S4 closes on a review of
the diff, not on a pull request" is satisfied. Invoke
`superpowers:requesting-code-review` against the full diff on
`fix/83-windows-install-tar` since it branched from `origin/main`
(`d83570c`) — Tasks 1 through 3 above are the entire diff. Record the
verdict in `tasks/83/RUN_83.md`'s S4 section, the same shape
`tasks/36/RUN_36.md`'s Gate S4 section uses. If the review sends anything
back for fixing, apply it, re-run the full `scripts/test-install.ps1` suite,
and re-request review before closing S4.

## Self-review

**Spec coverage.** `DESIGN_83.md` §1 → Task 2. §2 → Task 1. §3 → Task 3. §4
(no further test needed) → explains why Task 3 is the only new-test task;
nothing to implement for it directly. §5 (order of changes) → Task 1's
Step 1/Step 3 and Task 2's Step 1/Step 3 reproduce exactly that sequencing,
with the real observed output from this session substituted for the
design's predicted output. §6 (coverage table) → every AC row maps to a
task above: AC-1/AC-2 → Task 2 (AC-2 also → Task 3); AC-3/AC-4 → Task 1;
AC-5 → Task 1 Step 3 + Task 2 Step 3 (the red/green pair, witnessed); AC-6 →
no task touches the `sh` path; AC-7 → S5, after this plan's tasks close S4
(not a task in this plan — `CLAUDE.md` keeps S5 as its own stage with its
own `docs/evidence.md` artifact, the same separation `tasks/36/PLAN_36.md`'s
own Task 10 vs. its RUN's separate "Gate S5" section keeps).

**Placeholder scan.** No "TBD"/"TODO"/"handle edge cases" anywhere above;
every step's code block is the literal text to write, not a description of
it; every `Check`/`CheckContains` line quoted is the exact existing or new
line, not a paraphrase.

**Type/name consistency.** `$fakeArchive`, `$fakeArchiveName`,
`$corruptArchive`, `$fixtureTarExitCode`, `$tarExitCode`, `$TestArchiveFile`
are spelled identically everywhere they appear across all three tasks —
checked by re-reading each task's code blocks side by side after drafting.
`RunMain`'s here-string escaping (backtick before `$` for a variable that
must survive into the child script's literal text) matches the four
pre-existing lines' convention exactly for the one new line added.
