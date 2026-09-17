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

$HvName = 'herdr-voice'
$HvRepo = 'aliaksandr-haurylau-godel/herdr-voice'
$HvRetryAttempts = if ($env:HV_RETRY_ATTEMPTS) { [int]$env:HV_RETRY_ATTEMPTS } else { 5 }
$HvRetryDelay    = if ($env:HV_RETRY_DELAY)    { [int]$env:HV_RETRY_DELAY }    else { 3 }

function Get-HvRoot { Split-Path -Parent $PSScriptRoot }

function Get-HvManifestVersion([string]$Root) {
    # The first `version = "..."` in the manifest is the package's; the tables
    # below it have no version key of their own.
    $line = Select-String -Path (Join-Path $Root 'herdr-plugin.toml') `
        -Pattern '^version = "([^"]*)"' | Select-Object -First 1
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

function Stop-Hv([string]$Reason, [string]$Next) {
    Write-Host "${HvName}: $Reason"
    Write-Host "${HvName}: nothing was installed. $Next"
    exit 1
}

function Test-HvCargo { [bool](Get-Command cargo -ErrorAction SilentlyContinue) }

function Invoke-HvFallback([string]$Reason) {
    Write-Host "${HvName}: $Reason"
    Write-Host "${HvName}: building from source instead, which compiles the candle crates and takes a while."
    if (-not (Test-HvCargo)) {
        Stop-Hv 'there is no archive for this platform and no cargo to build one' `
                'install a Rust toolchain from https://rustup.rs and run the install again, or install on a platform a release archive is published for.'
    }
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Stop-Hv 'the source build failed' 'read the compiler output above.'
    }
}

function Invoke-HvMain {
    $root = Get-HvRoot
    Set-Location $root
    $tag = 'v' + (Get-HvManifestVersion $root)
    $target = Get-HvTarget -Architecture $env:PROCESSOR_ARCHITECTURE
    if (-not $target) {
        Invoke-HvFallback "no release archive is built for Windows on $env:PROCESSOR_ARCHITECTURE"
        return
    }

    $archive = "$HvName-$tag-$target.tar.gz"
    $base = "https://github.com/$HvRepo/releases/download/$tag"
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $tmp | Out-Null

    switch (Invoke-HvFetch "$base/$archive" "$tmp\$archive") {
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

    switch (Invoke-HvFetch "$base/$archive.sha256" "$tmp\$archive.sha256") {
        'missing' {
            Stop-Hv "the release $tag publishes $archive but no $archive.sha256, so its bytes cannot be checked" `
                    'report it against the release; an unverified archive is not installed.'
        }
        'error' {
            Stop-Hv "could not reach $base/$archive.sha256 after $HvRetryAttempts attempts" `
                    'check the network and run the install again.'
        }
    }

    $expected = ((Get-Content "$tmp\$archive.sha256" -Raw).Trim() -split '\s+')[0]
    $actual = (Get-FileHash "$tmp\$archive" -Algorithm SHA256).Hash.ToLower()
    if ($expected.ToLower() -ne $actual) {
        Stop-Hv "$archive does not match the digest published with it (expected $expected, got $actual)" `
                'the download is damaged or the release was changed after it was published; run the install again, and report it if it repeats.'
    }

    tar -xzf "$tmp\$archive" -C $tmp
    New-Item -ItemType Directory -Force -Path (Join-Path $root 'target\release') | Out-Null
    Copy-Item (Join-Path $tmp "$HvName-$tag-$target\$HvName.exe") `
              (Join-Path $root "target\release\$HvName.exe") -Force
    Write-Host "${HvName}: installed target\release\$HvName.exe from $tag, verified against its published digest"
}

if ($env:HERDR_VOICE_INSTALL_LIB -ne '1') { Invoke-HvMain }
