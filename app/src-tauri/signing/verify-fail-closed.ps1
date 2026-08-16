<#
.SYNOPSIS
  Automated verify for Task 1 (11-02-PLAN.md): proves sign.ps1 fails closed.

.DESCRIPTION
  Drives sign.ps1 through its three documented failure cases and asserts every
  one exits non-zero. This is the single most important signing behaviour in
  the phase: once the release workflow injects bundle.windows.signCommand,
  an unconfigured build must FAIL rather than silently reporting a successful,
  unsigned build.

  Exits non-zero if ANY case unexpectedly succeeds (exit code 0), or if
  sign.ps1 fails to parse cleanly under the PowerShell parser.

.EXAMPLE
  pwsh -NoProfile -File app/src-tauri/signing/verify-fail-closed.ps1
#>

$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SignScript = Join-Path $ScriptDir 'sign.ps1'

$failures = @()

function Test-Case {
    param(
        [string]$Name,
        [scriptblock]$Invocation
    )
    # Run in a child process so a non-zero exit from sign.ps1 can never abort
    # this harness (the harness itself must always finish and report).
    $exitCode = & $Invocation
    if ($exitCode -eq 0) {
        Write-Host "FAIL: '$Name' unexpectedly exited 0 (should have failed closed)"
        $script:failures += $Name
    } else {
        Write-Host "PASS: '$Name' exited non-zero ($exitCode) as expected"
    }
}

# --- 0. Parser sanity: sign.ps1 must parse cleanly under PowerShell --------
$parseErrors = $null
$null = [System.Management.Automation.Language.Parser]::ParseFile(
    $SignScript, [ref]$null, [ref]$parseErrors
)
if ($parseErrors -and $parseErrors.Count -gt 0) {
    Write-Host "FAIL: sign.ps1 has $($parseErrors.Count) PowerShell parse error(s):"
    $parseErrors | ForEach-Object { Write-Host "  $_" }
    $failures += 'parser-sanity'
} else {
    Write-Host "PASS: sign.ps1 parses cleanly under PowerShell's parser"
}

# --- 0b. No Azure credential NAME appears in any output-producing statement -
# (Write-Host/Write-Error/echo lines). The credential *values* never exist in
# this repo at all; this check guards against the credential *variable names*
# ever being interpolated into a diagnostic line, which is the actual T-11-06
# information-disclosure risk (leaking AZURE_CLIENT_SECRET's VALUE via a
# careless debug print).
$signSource = Get-Content -Raw -LiteralPath $SignScript
$outputLines = $signSource -split "`n" | Where-Object { $_ -match 'Write-Host|Write-Error' }
$forbiddenNames = @('AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET', 'AZURE_TENANT_ID')
$leaked = @()
foreach ($line in $outputLines) {
    foreach ($name in $forbiddenNames) {
        if ($line -match [regex]::Escape($name)) {
            $leaked += "$name in: $($line.Trim())"
        }
    }
}
if ($leaked.Count -gt 0) {
    Write-Host "FAIL: sign.ps1 output-producing statements reference Azure credential names:"
    $leaked | ForEach-Object { Write-Host "  $_" }
    $failures += 'no-credential-names-in-output'
} else {
    Write-Host "PASS: no Azure credential name appears in any output-producing statement"
}

# Resolve the current PowerShell executable so this harness runs unmodified
# under both pwsh (PowerShell 7+, what windows-latest CI runners ship) and
# Windows PowerShell 5.1 (powershell.exe, what may be available locally).
$ShellExe = if ($PSVersionTable.PSEdition -eq 'Core') { 'pwsh' } else { 'powershell' }

# Prepare a real artifact file so the dlib-related cases below reach step 2
# of sign.ps1 rather than failing earlier at the artifact-path check.
$tempArtifact = Join-Path ([System.IO.Path]::GetTempPath()) "verify-fail-closed-artifact-$([guid]::NewGuid()).txt"
Set-Content -LiteralPath $tempArtifact -Value 'not a real installer, just a path that exists'

# Windows PowerShell (and some PS7 configurations) wraps ANY stderr text from
# a native/child process into a terminating ErrorRecord when
# $ErrorActionPreference is 'Stop' -- even when that stream is redirected to
# $null. sign.ps1's Write-Error calls are exactly such stderr text (this
# harness deliberately drives sign.ps1 into its failure paths), so each
# invocation below runs under a local 'Continue' preference and relies on
# $LASTEXITCODE (which the child process sets correctly regardless) rather
# than on an exception never being thrown.
try {
    # --- 1. No argument at all -------------------------------------------
    Test-Case -Name 'no-argument' -Invocation {
        $prevDlib = $env:TRUSTED_SIGNING_DLIB
        Remove-Item Env:\TRUSTED_SIGNING_DLIB -ErrorAction SilentlyContinue
        $prevEAP = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & $ShellExe -NoProfile -File $SignScript *>$null
        $code = $LASTEXITCODE
        $ErrorActionPreference = $prevEAP
        if ($prevDlib) { $env:TRUSTED_SIGNING_DLIB = $prevDlib }
        return $code
    }

    # --- 2. Dlib env var unset, valid artifact path -----------------------
    Test-Case -Name 'dlib-unset' -Invocation {
        $prevDlib = $env:TRUSTED_SIGNING_DLIB
        Remove-Item Env:\TRUSTED_SIGNING_DLIB -ErrorAction SilentlyContinue
        $prevEAP = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & $ShellExe -NoProfile -File $SignScript $tempArtifact *>$null
        $code = $LASTEXITCODE
        $ErrorActionPreference = $prevEAP
        if ($prevDlib) { $env:TRUSTED_SIGNING_DLIB = $prevDlib }
        return $code
    }

    # --- 3. Dlib env var set to a path that does not exist ----------------
    Test-Case -Name 'dlib-missing-file' -Invocation {
        $prevDlib = $env:TRUSTED_SIGNING_DLIB
        $env:TRUSTED_SIGNING_DLIB = Join-Path ([System.IO.Path]::GetTempPath()) "does-not-exist-$([guid]::NewGuid()).dll"
        $prevEAP = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        & $ShellExe -NoProfile -File $SignScript $tempArtifact *>$null
        $code = $LASTEXITCODE
        $ErrorActionPreference = $prevEAP
        if ($prevDlib) { $env:TRUSTED_SIGNING_DLIB = $prevDlib } else { Remove-Item Env:\TRUSTED_SIGNING_DLIB -ErrorAction SilentlyContinue }
        return $code
    }
}
finally {
    Remove-Item -LiteralPath $tempArtifact -ErrorAction SilentlyContinue
}

if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "verify-fail-closed.ps1: FAILED -- $($failures.Count) case(s) did not fail closed: $($failures -join ', ')"
    exit 1
}

Write-Host ""
Write-Host "verify-fail-closed.ps1: PASSED -- sign.ps1 fails closed in every tested case"
exit 0
