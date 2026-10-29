[CmdletBinding()]
param(
  [string]$DistDirectory = "dist",
  [ValidateSet("x86_64", "aarch64")]
  [string]$Architecture,
  [switch]$ConfirmEphemeralRunner
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($Architecture)) {
  $Architecture = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    default { throw "unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
  }
}

if (
  -not $ConfirmEphemeralRunner -or
  $env:CI -cne "true" -or
  $env:GITHUB_ACTIONS -cne "true" -or
  $env:RUNNER_ENVIRONMENT -cne "github-hosted"
) {
  throw "This destructive installer contract may run only with -ConfirmEphemeralRunner on an ephemeral GitHub Actions runner."
}

$expectedTarget = switch ($Architecture) {
  "x86_64" { "x86_64-pc-windows-msvc" }
  "aarch64" { "aarch64-pc-windows-msvc" }
  default { throw "unsupported Windows installer architecture: $Architecture" }
}

$dist = (Resolve-Path $DistDirectory).Path
$installers = @(Get-ChildItem -Path $dist -Filter "*-user-setup.exe" -File)
if ($installers.Count -ne 1) {
  throw "expected exactly one user installer in $dist; found $($installers.Count)"
}
# Refuse a foreign-architecture artifact instead of handing it to a silent
# install: a mismatched installer stops at the NSIS architecture gate, and
# before that gate learned to fail silently the dialog blocked the job.
if ($installers[0].Name -notlike "*$expectedTarget*") {
  throw "installer $($installers[0].Name) is not a $Architecture package (expected *$expectedTarget*); refusing to install a foreign-architecture installer"
}

Write-Host "installer contract ($Architecture): $($installers[0].FullName)"
Write-Host "phase 1/3: install, idempotent reinstall, and uninstall"
& "$PSScriptRoot/test-windows-installer.ps1" `
  -InstallerPath $installers[0].FullName `
  -ConfirmEphemeralRunner

$fixtureDir = Join-Path $env:RUNNER_TEMP "neomacs-installer-upgrade-contract"
New-Item -ItemType Directory -Path $fixtureDir -Force | Out-Null
Write-Host "phase 2/3: build upgrade fixtures for $Architecture"
& bash "$PSScriptRoot/package-windows-installer-upgrade-fixtures.sh" `
  $fixtureDir $Architecture
if ($LASTEXITCODE -ne 0) {
  throw "failed to build installer upgrade fixtures"
}

Write-Host "phase 3/3: upgrade and uninstall contract"
& "$PSScriptRoot/test-windows-installer-upgrade.ps1" `
  -InstallerAPath (Join-Path $fixtureDir "neomacs-installer-contract-a.exe") `
  -InstallerBPath (Join-Path $fixtureDir "neomacs-installer-contract-b.exe") `
  -ConfirmEphemeralRunner
