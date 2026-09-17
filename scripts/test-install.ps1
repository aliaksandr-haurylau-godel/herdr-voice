# Drives every decision scripts/install.ps1 makes, with no network and no
# release, the way scripts/test-install.sh does for the shell script.
#
# install.ps1 is a second implementation of one set of rules, which is a second
# place those rules can be wrong, so it gets the same five outcomes driven
# through it rather than only a parse check.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$scriptPath = Join-Path $root 'scripts/install.ps1'
$failures = 0

function Check($label, $actual, $expected) {
    if ($actual -eq $expected) { Write-Host "ok    $label" }
    else { Write-Host "FAIL  $label`: got [$actual], expected [$expected]"; $script:failures++ }
}

function CheckContains($label, $haystack, $needle) {
    if ($haystack -and $haystack -match [regex]::Escape($needle)) { Write-Host "ok    $label" }
    else { Write-Host "FAIL  $label`: [$haystack] does not carry [$needle]"; $script:failures++ }
}

# --- it parses ---------------------------------------------------------------
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($scriptPath, [ref]$null, [ref]$errors) | Out-Null
if ($errors.Count -gt 0) {
    Write-Host "FAIL  install.ps1 does not parse: $($errors[0].Message)"
    $failures++
} else {
    Write-Host 'ok    install.ps1 parses'
}

$env:HERDR_VOICE_INSTALL_LIB = '1'
. $scriptPath

# --- the platform ------------------------------------------------------------
Check 'AMD64 maps to the built target' (Get-HvTarget -Architecture 'AMD64') 'x86_64-pc-windows-msvc'
# The release matrix has no ARM64 Windows build, so there is no archive and the
# run goes to the source build.
Check 'ARM64 has no target' (Get-HvTarget -Architecture 'ARM64') $null

# --- the version, out of a manifest -----------------------------------------
# A fixture rather than the real manifest, so this still means something when the
# real version moves; the real one is then checked only for being readable.
$fixture = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $fixture | Out-Null
try {

@'
id = "haurylau.voice"
name = "Voice"
version = "0.4.2"
min_herdr_version = "0.8.0"
'@ | Set-Content -Path (Join-Path $fixture 'herdr-plugin.toml')
Check 'version from the manifest' (Get-HvManifestVersion $fixture) '0.4.2'

if (Get-HvManifestVersion $root) {
    Write-Host "ok    the repository manifest yields $(Get-HvManifestVersion $root)"
} else {
    Write-Host 'FAIL  the repository manifest yielded no version'; $failures++
}

# A manifest with no version answers empty rather than throwing a null-index
# error, so Invoke-HvMain can say what is wrong.
New-Item -ItemType Directory -Path (Join-Path $fixture 'noversion') | Out-Null
'id = "x"' | Set-Content -Path (Join-Path $fixture 'noversion/herdr-plugin.toml')
Check 'a manifest with no version answers empty' (Get-HvManifestVersion (Join-Path $fixture 'noversion')) ''

# --- the whole sequence, with a fake release ---------------------------------
# A real archive of the shape release.yml builds: one top-level directory named
# after the tag and the target, holding the binary.
$target = 'x86_64-pc-windows-msvc'
$stage = Join-Path $fixture 'stage'
$inner = Join-Path $stage "herdr-voice-v0.4.2-$target"
New-Item -ItemType Directory -Path $inner | Out-Null
'fake' | Set-Content -Path (Join-Path $inner 'herdr-voice.exe')
$fakeArchive = Join-Path $fixture 'archive.tar.gz'
tar -C $stage -czf $fakeArchive "herdr-voice-v0.4.2-$target"
$goodDigest = (Get-FileHash $fakeArchive -Algorithm SHA256).Hash.ToLower()

$checkout = Join-Path $fixture 'checkout'
New-Item -ItemType Directory -Path $checkout | Out-Null
Copy-Item (Join-Path $fixture 'herdr-plugin.toml') (Join-Path $checkout 'herdr-plugin.toml')

$env:PROCESSOR_ARCHITECTURE = 'AMD64'

# Where the binary and the witness are expected to land. Built per segment so
# the separator is the running platform's: this suite is useful on macOS and
# Linux as well as on the Windows runner that matters.
$releaseDir = Join-Path (Join-Path $checkout 'target') 'release'
$installedExe = Join-Path $releaseDir 'herdr-voice.exe'
$installMarker = Join-Path $releaseDir '.herdr-voice-install'
$targetDir = Join-Path $checkout 'target'

function RunMain {
    # Invoke-HvMain calls exit on its stopping paths, so it runs in a child
    # PowerShell: `exit` inside this process would end the suite. The child
    # dot-sources the script and the same overrides, then runs it.
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
    $file = Join-Path $fixture 'run.ps1'
    Set-Content -Path $file -Value $overrides
    $out = Join-Path $fixture 'out.txt'
    $exe = (Get-Process -Id $PID).Path
    & $exe -NoProfile -ExecutionPolicy Bypass -File $file *> $out
    $script:LastSaid = (Get-Content $out -Raw)
    return $LASTEXITCODE
}

$TestNoCargo = '0'

# The good path.
Remove-Item -Recurse -Force $targetDir -ErrorAction SilentlyContinue
$TestArchive = 'ok'; $TestSidecar = 'ok'; $TestDigest = $goodDigest
Check 'a good release installs' (RunMain) 0
if (Test-Path $installedExe) {
    Write-Host 'ok    the binary landed at target\release\herdr-voice.exe'
} else { Write-Host 'FAIL  no binary at target\release\herdr-voice.exe'; $failures++ }
CheckContains 'it recorded that it fetched and verified' `
    (Get-Content $installMarker -Raw) 'verified sha256'

# An uppercase digest is the same digest.
Remove-Item -Recurse -Force $targetDir -ErrorAction SilentlyContinue
$TestDigest = $goodDigest.ToUpper()
Check 'an uppercase digest still matches' (RunMain) 0

# A digest that disagrees stops, and nothing is unpacked.
Remove-Item -Recurse -Force $targetDir -ErrorAction SilentlyContinue
$TestDigest = '0000000000000000000000000000000000000000000000000000000000000000'
Check 'a wrong digest stops' (RunMain) 1
CheckContains 'it named the expected digest' $LastSaid '0000000000000000'
if (Test-Path $installedExe) {
    Write-Host 'FAIL  a mismatched archive was unpacked anyway'; $failures++
} else { Write-Host 'ok    a mismatched archive is not unpacked' }

# An archive whose digest is absent stops without unpacking.
Remove-Item -Recurse -Force $targetDir -ErrorAction SilentlyContinue
$TestArchive = 'ok'; $TestSidecar = 'missing'; $TestDigest = $goodDigest
Check 'a missing digest stops' (RunMain) 1
if (Test-Path $installedExe) {
    Write-Host 'FAIL  an unverifiable archive was unpacked'; $failures++
} else { Write-Host 'ok    an unverifiable archive is not unpacked' }

# A network that does not answer stops rather than falling back.
$TestArchive = 'error'; $TestSidecar = 'ok'
Check 'an unreachable archive stops' (RunMain) 1
CheckContains 'a stop says nothing was installed' $LastSaid 'nothing was installed'
if ($LastSaid -match 'building from source') {
    Write-Host 'FAIL  it fell back on a network error'; $failures++
} else { Write-Host 'ok    it did not fall back on a network error' }

# A 404 falls back, and the message names the tag, the target and the URL.
$TestArchive = 'missing'; $TestSidecar = 'ok'; $TestNoCargo = '1'
Check 'no archive and no cargo stops' (RunMain) 1
CheckContains 'the fallback message carries the tag' $LastSaid 'v0.4.2'
CheckContains 'the fallback message carries the target' $LastSaid $target
CheckContains 'the fallback message carries the URL' $LastSaid 'https://github.com/'
CheckContains 'it says where to get a toolchain' $LastSaid 'rustup.rs'

} finally {
    Remove-Item -Recurse -Force $fixture -ErrorAction SilentlyContinue
}

if ($failures -gt 0) { Write-Host "`n$failures assertion(s) failed"; exit 1 }
Write-Host "`nall install.ps1 assertions passed"
