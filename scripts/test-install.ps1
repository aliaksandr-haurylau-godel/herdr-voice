# Drives what can be driven without a Windows machine to install on: the file
# parses, and it maps the architecture the way the release matrix does.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root 'scripts/install.ps1'
$failures = 0

$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($script, [ref]$null, [ref]$errors) | Out-Null
if ($errors.Count -gt 0) {
    Write-Host "FAIL  install.ps1 does not parse: $($errors[0].Message)"
    $failures++
} else {
    Write-Host 'ok    install.ps1 parses'
}

$env:HERDR_VOICE_INSTALL_LIB = '1'
. $script

function Check($label, $actual, $expected) {
    if ($actual -eq $expected) { Write-Host "ok    $label" }
    else { Write-Host "FAIL  $label`: got [$actual], expected [$expected]"; $script:failures++ }
}

Check 'AMD64 maps to the built target' (Get-HvTarget -Architecture 'AMD64') 'x86_64-pc-windows-msvc'
# The release matrix has no ARM64 Windows build, so there is no archive and the
# run goes to the source build.
Check 'ARM64 has no target' (Get-HvTarget -Architecture 'ARM64') $null

# The version has to come out of the manifest, the same way the shell script
# reads it: it is the only statement of the release inside the checkout.
Check 'the version comes from the manifest' (Get-HvManifestVersion $root) '0.0.0'

if ($failures -gt 0) { Write-Error "$failures assertion(s) failed"; exit 1 }
Write-Host "`nall install.ps1 assertions passed"
