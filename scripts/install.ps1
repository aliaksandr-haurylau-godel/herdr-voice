# The herdr [[build]] step for Windows: put the released binary at
# target\release\herdr-voice.exe without compiling it.
#
# Same sequence as scripts/install.sh, and the same rules: a 404 that survives the
# retries is the source build, anything else that stops us reaching the archive is
# a stop, an archive whose digest is missing or wrong is a stop, and every stop
# says the install was aborted and nothing registered.
#
# It is a second implementation rather than a shared one because there is nothing
# worth sharing: the release matrix builds exactly one Windows target, so where
# the shell script needs a four-way map this needs a constant.
#
# Dot-sourcing this with HERDR_VOICE_INSTALL_LIB=1 defines the functions and runs
# nothing. scripts/test-install.ps1 does that.

$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 renders a progress bar per chunk for Invoke-WebRequest
# -OutFile, which makes a large download roughly an order of magnitude slower. A
# release archive carrying a candle-linked binary is not small, and the result
# looks like a hang on the one code path that gets no second chance.
$ProgressPreference = 'SilentlyContinue'

$HvName = 'herdr-voice'
$HvRepo = 'aliaksandr-haurylau-godel/herdr-voice'
# Set by Invoke-HvMain before anything needs it. Declared here so a function
# called on its own, as the test suite does, has something to read.
$HvRoot = '.'
$HvRetryAttempts = if ($env:HV_RETRY_ATTEMPTS) { [int]$env:HV_RETRY_ATTEMPTS } else { 5 }
$HvRetryDelay    = if ($env:HV_RETRY_DELAY)    { [int]$env:HV_RETRY_DELAY }    else { 3 }

function Get-HvRoot { Split-Path -Parent $PSScriptRoot }

function Get-HvManifestVersion([string]$Root) {
    # The first `version = "..."` in the manifest is the package's; the tables
    # below it have no version key of their own.
    $line = Select-String -Path (Join-Path $Root 'herdr-plugin.toml') `
        -Pattern '^version = "([^"]*)"' | Select-Object -First 1
    if (-not $line) { return '' }
    $line.Matches[0].Groups[1].Value
}

function Get-HvTarget([string]$Architecture) {
    # The release matrix builds one Windows target. ARM64 has no archive, which
    # is not an error - it is the source build.
    switch ($Architecture) {
        'AMD64' { 'x86_64-pc-windows-msvc' }
        default { $null }
    }
}

function Invoke-HvFetch([string]$Url, [string]$Destination) {
    # Returns 'ok', 'missing' or 'error', after retrying. GitHub answers 404 for
    # some minutes after a release publishes, so a single 404 settles nothing:
    # the retry decides whether we know, and the answer decides what we do.
    for ($attempt = 1; ; $attempt++) {
        $outcome = 'error'
        try {
            Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
            $outcome = 'ok'
        } catch {
            $code = $_.Exception.Response.StatusCode.value__
            if ($code -eq 404) { $outcome = 'missing' }
        }
        if ($outcome -eq 'ok' -or $attempt -ge $HvRetryAttempts) { return $outcome }
        Start-Sleep -Seconds $HvRetryDelay
    }
}

function Write-HvError([string]$Message) {
    # Standard error, the way scripts/install.sh writes its failures. Write-Error
    # would raise a terminating error under $ErrorActionPreference = 'Stop' and
    # lose the exit code this script chooses.
    [Console]::Error.WriteLine($Message)
}

function Stop-Hv([string]$Reason, [string]$Next) {
    Write-HvError "${HvName}: $Reason"
    Write-HvError "${HvName}: nothing was installed. $Next"
    exit 1
}

function Test-HvCargo { [bool](Get-Command cargo -ErrorAction SilentlyContinue) }

function Write-HvSource([string]$How) {
    # The durable witness scripts/install-check.sh reads, written next to the
    # binary rather than only printed: herdr reports a build command's output
    # when the build fails, and nothing establishes that it echoes a successful
    # one.
    # Join-Path per segment rather than a literal 'target\release': the
    # separator is then the running platform's, which keeps the script testable
    # off Windows as well as correct on it.
    $dir = Join-Path (Join-Path $HvRoot 'target') 'release'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Set-Content -Path (Join-Path $dir '.herdr-voice-install') -Value $How
}

function Invoke-HvFallback([string]$Reason) {
    Write-HvError "${HvName}: $Reason"
    Write-HvError "${HvName}: building from source instead, which compiles the candle crates and takes a while."
    if (-not (Test-HvCargo)) {
        Stop-Hv 'there is no archive for this platform and no cargo to build one' `
                'install a Rust toolchain from https://rustup.rs and run the install again, or install on a platform a release archive is published for.'
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Stop-Hv 'the source build failed' `
                'read the compiler output above; this is a build of this crate, not of the install step.'
    }
    Write-HvSource 'built from source'
}

function Invoke-HvMain {
    $root = Get-HvRoot
    $script:HvRoot = $root
    Set-Location $root
    $version = Get-HvManifestVersion $root
    if (-not $version) {
        Stop-Hv "no version could be read from $root\herdr-plugin.toml" `
                'the manifest is the only place this checkout says which release it belongs to; check that it has a top-level version key.'
    }
    $tag = 'v' + $version
    $target = Get-HvTarget -Architecture $env:PROCESSOR_ARCHITECTURE
    if (-not $target) {
        Invoke-HvFallback "no release archive is built for Windows on $env:PROCESSOR_ARCHITECTURE"
        return
    }

    $archive = "$HvName-$tag-$target.tar.gz"
    $base = "https://github.com/$HvRepo/releases/download/$tag"
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    # scripts/install.sh has `trap ... EXIT` for this. Without the finally, every
    # install and every retried failure leaves the archive and its unpacked copy
    # behind in %TEMP%. Stop-Hv's `exit 1` still runs it.
    try {

    # Named distinctly on purpose. PowerShell resolves a variable a function does
    # not define from its CALLER's scope, so a local here called $archivePath
    # would be picked up by any helper that happened to use the same name — which
    # is exactly how the test suite's fetch stub first went wrong.
    $hvArchiveFile = Join-Path $tmp $archive
    $hvSidecarFile = Join-Path $tmp "$archive.sha256"

    switch (Invoke-HvFetch "$base/$archive" $hvArchiveFile) {
        'missing' {
            # A 404 here is what a missing release and a missing target both look
            # like, and nothing else fetched tells them apart, so the message
            # names both rather than guessing.
            Invoke-HvFallback "no archive at $base/$archive - either no release is tagged $tag, or that release has no build for $target"
            return
        }
        'error' {
            Stop-Hv "could not reach $base/$archive after $HvRetryAttempts attempts" `
                    'check the network and run the install again.'
        }
    }

    switch (Invoke-HvFetch "$base/$archive.sha256" $hvSidecarFile) {
        'missing' {
            Stop-Hv "the release $tag publishes $archive but no $archive.sha256, so its bytes cannot be checked" `
                    'report it against the release; an unverified archive is not installed.'
        }
        'error' {
            Stop-Hv "could not reach $base/$archive.sha256 after $HvRetryAttempts attempts" `
                    'check the network and run the install again.'
        }
    }

    $expected = ((Get-Content $hvSidecarFile -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash $hvArchiveFile -Algorithm SHA256).Hash.ToLower()
    if ($expected.ToLower() -ne $actual) {
        Stop-Hv "$archive does not match the digest published with it (expected $expected, got $actual)" `
                'the download is damaged or the release was changed after it was published; run the install again, and report it if it repeats.'
    }

    # Checked rather than left to fail on its own. tar is a native command, so
    # $ErrorActionPreference does not catch it, and without this a verified
    # download that unpacks badly ends on an uncaught Copy-Item error with no
    # statement that nothing was installed and nothing to do next.
    tar -xzf $hvArchiveFile -C $tmp
    if ($LASTEXITCODE -ne 0) {
        Stop-Hv "$archive was verified but could not be unpacked" `
                'the archive is not a readable gzip tarball; report it against the release.'
    }
    $unpacked = Join-Path (Join-Path $tmp "$HvName-$tag-$target") "$HvName.exe"
    if (-not (Test-Path $unpacked)) {
        Stop-Hv "$archive does not contain $HvName-$tag-$target\$HvName.exe" `
                'the archive was built with a different layout than this script expects; report it against the release.'
    }
    $releaseDir = Join-Path (Join-Path $root 'target') 'release'
    New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null
    Copy-Item $unpacked (Join-Path $releaseDir "$HvName.exe") -Force
    Write-HvSource "fetched $archive from $tag, verified sha256 $actual"
    Write-Host "${HvName}: installed target\release\$HvName.exe from $tag, verified against its published digest"

    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }
}

if ($env:HERDR_VOICE_INSTALL_LIB -ne '1') { Invoke-HvMain }
