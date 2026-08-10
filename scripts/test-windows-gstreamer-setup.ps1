#Requires -Version 7.0
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$setup = Join-Path $PSScriptRoot 'setup-windows-gstreamer.ps1'
$build = Join-Path $PSScriptRoot 'emacs-build.ps1'

$root = Join-Path ([System.IO.Path]::GetTempPath()) `
  "neomacs-gst-test-$([guid]::NewGuid().ToString('N'))"
$repoTarget = Join-Path $root 'repo'
$sdkRoot = Join-Path $repoTarget 'PFiles64\gstreamer\1.0\msvc_x86_64'
$runtimeMsi = Join-Path $root 'runtime.msi'
$develMsi = Join-Path "$sdkRoot-msi-cache" `
  'gstreamer-1.0-devel-msvc-x86_64-1.26.9.msi'
$originalEnvironment = @{}
Get-ChildItem Env: | ForEach-Object { $originalEnvironment[$_.Name] = $_.Value }
$previousStartProcess = Get-Item Function:\Start-Process -ErrorAction SilentlyContinue

try {
  New-Item -ItemType Directory -Force -Path `
    $repoTarget,
    (Split-Path -Parent $develMsi),
    (Join-Path $root 'github'),
    (Join-Path $sdkRoot 'bin') | Out-Null
  New-Item -ItemType File -Path $runtimeMsi, $develMsi, `
    (Join-Path $root 'github\env'), (Join-Path $root 'github\path'), `
    (Join-Path $sdkRoot 'bin\partial-marker') | Out-Null
  $env:GSTREAMER_VERSION = '1.26.9'
  $env:GITHUB_ENV = Join-Path $root 'github\env'
  $env:GITHUB_PATH = Join-Path $root 'github\path'

  $runtimePayload = Join-Path $root 'runtime-payload'
  $develPayload = Join-Path $root 'devel-payload'
  New-Item -ItemType Directory -Force -Path `
    (Join-Path $runtimePayload 'bin'),
    (Join-Path $runtimePayload 'include'),
    (Join-Path $runtimePayload 'lib\pkgconfig'),
    (Join-Path $develPayload 'bin'),
    (Join-Path $develPayload 'include'),
    (Join-Path $develPayload 'lib\pkgconfig') | Out-Null
  New-Item -ItemType File -Path `
    (Join-Path $runtimePayload 'bin\gstreamer-1.0-0.dll'),
    (Join-Path $runtimePayload 'include\runtime.h'),
    (Join-Path $runtimePayload 'lib\pkgconfig\glib-2.0.pc'),
    (Join-Path $develPayload 'bin\pkg-config.exe'),
    (Join-Path $develPayload 'include\devel.h'),
    (Join-Path $develPayload 'lib\pkgconfig\gstreamer-1.0.pc') | Out-Null

  $global:payloadTrees = @($runtimePayload, $develPayload)
  $global:payloadIndex = 0
  $global:msiTargets = @()
  $global:expectedMsiTarget = $repoTarget
  function global:Start-Process {
    param(
      [string]$FilePath,
      [switch]$Wait,
      [switch]$PassThru,
      [object[]]$ArgumentList
    )
    if ($FilePath -ne 'msiexec.exe') {
      throw "unexpected process: $FilePath"
    }
    if ($ArgumentList.Count -ne 4 -or
        $ArgumentList[0] -cne '/a' -or
        $ArgumentList[2] -cne '/qn' -or
        -not ([string]$ArgumentList[1]).StartsWith('"') -or
        -not ([string]$ArgumentList[1]).EndsWith('"')) {
      throw "unexpected msiexec arguments: $($ArgumentList -join ', ')"
    }
    $targetArgument = [string]$ArgumentList[3]
    if ($targetArgument -cne "TARGETDIR=`"$global:expectedMsiTarget`"") {
      throw "unexpected MSI TARGETDIR argument: $targetArgument"
    }
    $target = ([string]$targetArgument).Substring(10).Trim('"')
    $global:msiTargets += $target
    $payload = Join-Path $target 'PFiles64\gstreamer\1.0\msvc_x86_64'
    New-Item -ItemType Directory -Force -Path $payload | Out-Null
    Copy-Item -Path (Join-Path $global:payloadTrees[$global:payloadIndex] '*') `
      -Destination $payload -Recurse -Force
    $global:payloadIndex++
    [pscustomobject]@{ ExitCode = 0 }
  }

  & $setup -Install -GStreamerRoot $sdkRoot -GStreamerRuntimeMsi $runtimeMsi | Out-Null

  if ($global:payloadIndex -ne 2 -or $global:msiTargets.Count -ne 2) {
    throw 'setup did not administratively extract both MSIs'
  }
  if ($global:msiTargets[0] -cne $repoTarget -or $global:msiTargets[0] -cne $global:msiTargets[1]) {
    throw 'runtime and development MSIs did not share the repository TARGETDIR'
  }
  foreach ($marker in @(
      'bin\pkg-config.exe',
      'bin\gstreamer-1.0-0.dll',
      'include',
      'lib\pkgconfig\glib-2.0.pc',
      'lib\pkgconfig\gstreamer-1.0.pc')) {
    $type = if ($marker -eq 'include') { 'Container' } else { 'Leaf' }
    if (-not (Test-Path -LiteralPath (Join-Path $sdkRoot $marker) -PathType $type)) {
      throw "final SDK is missing required marker: $marker"
    }
  }
  foreach ($nested in @('bin\bin', 'lib\lib')) {
    if (Test-Path -LiteralPath (Join-Path $sdkRoot $nested)) {
      throw "final SDK contains duplicate nested directory: $nested"
    }
  }
  if ($env:GSTREAMER_ROOT_X86_64 -cne $sdkRoot) {
    throw 'GSTREAMER_ROOT_X86_64 was not exported as the exact SDK root'
  }

  $badRoot = Join-Path $root 'custom-sdk'
  $badError = $null
  try {
    & $setup -Install -GStreamerRoot $badRoot -GStreamerRuntimeMsi $runtimeMsi
  } catch {
    $badError = $_.Exception.Message
  }
  if ($badError -notmatch 'PFiles64.*msvc_x86_64') {
    throw 'missing custom root did not explain the required managed layout'
  }

  $buildOutput = @(& $build -SkipBuild -SkipPackage)
  if ($buildOutput -notcontains 'build and packaging skipped') {
    throw 'combined emacs-build skip flags were not a no-op'
  }

  $buildHarness = Join-Path $root 'build-harness'
  $buildHarnessScripts = Join-Path $buildHarness 'scripts'
  $buildHarnessTools = Join-Path $buildHarness 'tools'
  $buildHarnessGStreamer = Join-Path $buildHarness 'gstreamer'
  $buildHarnessGit = Join-Path $buildHarness 'Git'
  $buildHarnessGitCmd = Join-Path $buildHarnessGit 'cmd'
  $buildHarnessGitBin = Join-Path $buildHarnessGit 'bin'
  $buildHarnessGitUsrBin = Join-Path $buildHarnessGit 'usr\bin'
  New-Item -ItemType Directory -Force -Path `
    $buildHarnessScripts,
    $buildHarnessTools,
    $buildHarnessGStreamer,
    $buildHarnessGitCmd,
    $buildHarnessGitBin,
    $buildHarnessGitUsrBin | Out-Null
  New-Item -ItemType File -Force -Path `
    (Join-Path $buildHarnessGitCmd 'git.exe'),
    (Join-Path $buildHarnessGitBin 'bash.exe'),
    (Join-Path $buildHarnessGitUsrBin 'awk.exe') | Out-Null
  Copy-Item -LiteralPath $build -Destination $buildHarnessScripts
  @'
param(
  [switch]$Install,
  [string]$GStreamerRoot,
  [string]$GStreamerRuntimeMsi
)
$env:GSTREAMER_ROOT_X86_64 = $env:NEOMACS_TEST_GSTREAMER_ROOT
$env:PKG_CONFIG = $env:NEOMACS_TEST_PKG_CONFIG
$env:PKG_CONFIG_PATH = $env:NEOMACS_TEST_GSTREAMER_ROOT
$env:PKG_CONFIG_LIBDIR = $env:NEOMACS_TEST_GSTREAMER_ROOT
'@ | Set-Content -LiteralPath (Join-Path $buildHarnessScripts 'setup-windows-gstreamer.ps1')
  @'
@exit /b 0
'@ | Set-Content -LiteralPath (Join-Path $buildHarnessTools 'pkg-config.cmd')
  @'
@where awk.exe >nul 2>&1 || exit /b 42
@exit /b 0
'@ | Set-Content -LiteralPath (Join-Path $buildHarnessTools 'cargo.cmd')

  $env:NEOMACS_TEST_GSTREAMER_ROOT = $buildHarnessGStreamer
  $env:NEOMACS_TEST_PKG_CONFIG = Join-Path $buildHarnessTools 'pkg-config.cmd'
  $env:PATH = "$buildHarnessGitCmd;$buildHarnessTools;$env:PATH"
  $buildHarnessOutput = @(
    & pwsh -NoProfile -File (Join-Path $buildHarnessScripts 'emacs-build.ps1') `
      -SkipPackage 2>&1
  )
  if ($LASTEXITCODE -ne 0) {
    throw "PowerShell build did not expose Git awk.exe to xtask: $($buildHarnessOutput -join "`n")"
  }

  Write-Output 'windows GStreamer setup contract passed'
}
finally {
  $currentNames = @(Get-ChildItem Env: | Select-Object -ExpandProperty Name)
  foreach ($name in $currentNames | Where-Object { -not $originalEnvironment.ContainsKey($_) }) {
    Remove-Item -LiteralPath "Env:$name" -ErrorAction SilentlyContinue
  }
  foreach ($entry in $originalEnvironment.GetEnumerator()) {
    Set-Item -LiteralPath "Env:$($entry.Key)" -Value $entry.Value
  }
  if ($null -eq $previousStartProcess) {
    Remove-Item Function:\Start-Process -ErrorAction SilentlyContinue
  } else {
    Set-Item Function:\Start-Process -Value $previousStartProcess.Definition
  }
  Remove-Variable payloadTrees, payloadIndex, msiTargets, expectedMsiTarget `
    -Scope Global -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $root -Recurse -Force -ErrorAction SilentlyContinue
}
