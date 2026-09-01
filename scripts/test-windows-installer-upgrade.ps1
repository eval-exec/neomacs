[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$InstallerAPath,

  [Parameter(Mandatory = $true)]
  [string]$InstallerBPath,

  [switch]$ConfirmEphemeralRunner
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if (
  -not $ConfirmEphemeralRunner -or
  $env:CI -cne "true" -or
  $env:GITHUB_ACTIONS -cne "true" -or
  $env:RUNNER_ENVIRONMENT -cne "github-hosted"
) {
  throw "This destructive installer contract test may run only with -ConfirmEphemeralRunner on an ephemeral GitHub Actions runner."
}

function Invoke-Installer {
  param([string]$Path)

  $process = Start-Process -FilePath $Path -ArgumentList "/S" -PassThru -Wait
  if ($process.ExitCode -ne 0) {
    throw "installer exited with code $($process.ExitCode): $Path"
  }
}

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class NeomacsInstallerWindows {
  private delegate bool EnumWindowsProc(IntPtr window, IntPtr parameter);

  [DllImport("user32.dll")]
  private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr parameter);

  [DllImport("user32.dll")]
  private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

  [DllImport("user32.dll")]
  public static extern IntPtr GetDlgItem(IntPtr dialog, int itemId);

  [DllImport("user32.dll")]
  public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

  public static IntPtr[] ForProcess(int expectedProcessId) {
    var result = new List<IntPtr>();
    EnumWindows(delegate(IntPtr window, IntPtr parameter) {
      uint processId;
      GetWindowThreadProcessId(window, out processId);
      if (processId == expectedProcessId) {
        result.Add(window);
      }
      return true;
    }, IntPtr.Zero);
    return result.ToArray();
  }
}
"@

function Invoke-InstallerAndCancelAtWelcome {
  param([string]$Path)

  $process = Start-Process -FilePath $Path -PassThru
  try {
    for ($attempt = 0; $attempt -lt 100; $attempt++) {
      $process.Refresh()
      if ($process.HasExited -or $process.MainWindowHandle -ne [IntPtr]::Zero) {
        break
      }
      Start-Sleep -Milliseconds 100
    }
    if ($process.HasExited -or $process.MainWindowHandle -eq [IntPtr]::Zero) {
      throw "installer did not show its welcome page"
    }
    if (-not $process.CloseMainWindow()) {
      throw "could not request cancellation from the installer welcome page"
    }

    # MUI_ABORTWARNING displays a native Yes/No dialog. IDYES is 6 and
    # BM_CLICK is 0x00F5, so this confirms cancellation without desktop input.
    $confirmed = $false
    for ($attempt = 0; $attempt -lt 100 -and -not $process.HasExited; $attempt++) {
      foreach ($window in [NeomacsInstallerWindows]::ForProcess($process.Id)) {
        $yesButton = [NeomacsInstallerWindows]::GetDlgItem($window, 6)
        if ($yesButton -ne [IntPtr]::Zero) {
          [void][NeomacsInstallerWindows]::SendMessage(
            $yesButton,
            0x00F5,
            [IntPtr]::Zero,
            [IntPtr]::Zero
          )
          $confirmed = $true
          break
        }
      }
      if (-not $confirmed) {
        Start-Sleep -Milliseconds 100
      }
    }
    if (-not $confirmed) {
      throw "installer did not show its cancellation confirmation"
    }
    if (-not $process.WaitForExit(10000)) {
      throw "installer did not exit after cancellation"
    }
  } finally {
    if (-not $process.HasExited) {
      $process.Kill()
      $process.WaitForExit()
    }
    $process.Dispose()
  }
}

$installerA = (Resolve-Path $InstallerAPath).Path
$installerB = (Resolve-Path $InstallerBPath).Path
$installDir = Join-Path $env:LOCALAPPDATA "Programs\NEO Emacs"
$shareDir = Join-Path $installDir "share\neomacs"
$uninstaller = Join-Path $installDir "uninstall.exe"
$aOnly = Join-Path $shareDir "removed-in-b.txt"
$bOnly = Join-Path $shareDir "added-in-b.txt"
$common = Join-Path $shareDir "common.txt"
$unrelated = Join-Path $installDir "created-between-versions.txt"
$installed = $false

try {
  $installed = $true
  Invoke-Installer -Path $installerA
  if (-not (Test-Path $aOnly -PathType Leaf)) {
    throw "version A did not install its version-specific owned file"
  }

  Invoke-InstallerAndCancelAtWelcome -Path $installerB
  if (-not (Test-Path $aOnly -PathType Leaf)) {
    throw "opening and cancelling version B removed version A's payload"
  }
  if (Test-Path $bOnly) {
    throw "opening and cancelling version B installed part of version B's payload"
  }
  if ((Get-Content -Path $common -Raw) -cne "version a`n") {
    throw "opening and cancelling version B changed version A's shared payload"
  }

  Set-Content -Path $unrelated -Value "not installer owned" -NoNewline

  # Version A's uninstaller is run IN PLACE during the upgrade, and an in-place
  # uninstaller cannot delete itself, so A's uninstall.exe is still on disk when
  # B installs over it.  B must overwrite it via WriteUninstaller; if that write
  # is lost, every later uninstall runs A's file list, which deletes the shared
  # payload and leaves anything only B owns behind - exactly what aarch64 shows.
  # Hash it either side of the upgrade so a lost overwrite is reported HERE,
  # naming the cause, rather than surfacing later as a stray file.
  $uninstallerHashBefore = (Get-FileHash -Path $uninstaller -Algorithm SHA256).Hash

  Invoke-Installer -Path $installerB

  $uninstallerHashAfter = (Get-FileHash -Path $uninstaller -Algorithm SHA256).Hash
  if ($uninstallerHashBefore -eq $uninstallerHashAfter) {
    throw ("version B did not replace the uninstaller: it is still byte-identical " +
      "to version A's ($uninstallerHashAfter), so an uninstall would run A's " +
      "file list and leave B's own files behind")
  }

  if (Test-Path $aOnly) {
    throw "version B left a file owned only by version A"
  }
  if (-not (Test-Path $bOnly -PathType Leaf)) {
    throw "version B did not install its version-specific owned file"
  }
  if ((Get-Content -Path $common -Raw) -cne "version b`n") {
    throw "version B did not replace the shared payload"
  }
  if (-not (Test-Path $unrelated -PathType Leaf)) {
    throw "upgrade deleted a file not owned by either installer"
  }

  # `_?=' is NSIS's documented switch for running an uninstaller IN PLACE.
  # Without it a silent uninstaller copies itself to %TEMP% and relaunches, so
  # -Wait returns before any deletion happens and the caller is left polling a
  # guessed budget.  That guess is what failed here: on windows-11-arm the
  # relaunched copy is a 32-bit x86 image under emulation, a wholly different
  # cost from the native aarch64 build, and 5s then 10s both expired with the
  # file still present.  Running in place removes the race instead of pricing
  # it -- the process we wait on is the one that deletes.
  # ONE argument string, not two.  NSIS reads `_?=' as the raw remainder of the
  # command line, and the install directory contains a space ("NEO Emacs"), so
  # passing it as its own -ArgumentList element makes PowerShell quote it and
  # NSIS then takes the quote as part of the path.  It fails the way this whole
  # bug already looks -- exit 0, nothing deleted -- so the quoting must not
  # happen in the first place.
  # ProcessStartInfo.Arguments reaches CreateProcess VERBATIM.  Start-Process
  # builds its command line from -ArgumentList and can quote an element that
  # contains a space, and the install directory has one ("NEO Emacs"); NSIS
  # reads _?= as the raw remainder of the command line, so a quote lands inside
  # the path and the switch is silently ignored.
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $uninstaller
  $psi.Arguments = "/S _?=$installDir"
  $psi.UseShellExecute = $false
  $process = [System.Diagnostics.Process]::Start($psi)
  $process.WaitForExit()
  if ($process.ExitCode -ne 0) {
    throw "version B uninstaller exited with code $($process.ExitCode)"
  }
  $installed = $false

  # ATTEST that _?= was honoured rather than assuming it.  An in-place
  # uninstaller cannot delete itself - its own image is open - while one that
  # self-copied to %TEMP% and relaunched deletes the original.  So the
  # uninstaller still being here IS the evidence the switch took effect, and
  # its absence is the precise failure that looks like "deleted nothing":
  # without _?= the work happens in a relaunched copy, which on windows-11-arm
  # is an emulated x86 image that does not carry out the deletions.
  $uninstallerLeftBehind = Join-Path $installDir "uninstall.exe"
  if (-not (Test-Path $uninstallerLeftBehind)) {
    throw ("the uninstaller removed itself, so _?= was not honoured and it " +
      "self-copied and relaunched instead of running in place")
  }
  # The image of the just-exited uninstaller can still be held for a moment,
  # which is the same effect that loses WriteUninstaller during the upgrade, so
  # a single removal here fails silently.  Retry while it is released.
  for ($i = 0; $i -lt 50 -and (Test-Path $uninstallerLeftBehind); $i++) {
    Remove-Item $uninstallerLeftBehind -Force -ErrorAction SilentlyContinue
    if (Test-Path $uninstallerLeftBehind) { Start-Sleep -Milliseconds 100 }
  }

  # An NSIS uninstaller launched with /S copies itself to %TEMP% and relaunches,
  # so -Wait above returns before the deletion happens: this poll IS the wait.
  # Give it the same 10s budget the process waits in this file already use
  # (WaitForExit(10000), and the 100-attempt loops above) rather than half of
  # it -- 5s passed on windows-latest and failed on windows-11-arm.
  #
  # Report the elapsed wait on failure. Without it a repeat failure cannot be
  # told apart from a budget that is merely still too small, which is exactly
  # the ambiguity that made the first one expensive to diagnose.
  $uninstallTimeout = [TimeSpan]::FromSeconds(10)
  $waited = [Diagnostics.Stopwatch]::StartNew()
  while ((Test-Path $bOnly) -and $waited.Elapsed -lt $uninstallTimeout) {
    Start-Sleep -Milliseconds 100
  }
  $waited.Stop()
  if (Test-Path $bOnly) {
    # Two guesses have already been spent on this (a wait budget, then argument
    # quoting), each costing a CI round.  Report what the uninstaller actually
    # did instead of inferring it: whether it deleted nothing, or everything
    # except this file, distinguishes "the uninstaller did not run its deletes"
    # from "this path was never in its uninstall list", and no amount of
    # re-reading the script can tell those apart.
    Write-Host "=== uninstall diagnostics ==="
    Write-Host "install dir exists: $(Test-Path $installDir)"
    if (Test-Path $installDir) {
      Write-Host "--- surviving files under $installDir ---"
      Get-ChildItem -Path $installDir -Recurse -File -ErrorAction SilentlyContinue |
        ForEach-Object { Write-Host "  $($_.FullName)" }
    }
    foreach ($probe in @($aOnly, $bOnly, $common, $unrelated, $uninstaller)) {
      Write-Host "  exists=$(Test-Path $probe)  $probe"
    }
    Write-Host "=== end diagnostics ==="
    throw ("version B uninstaller left an installer-owned file " +
      "after waiting $([int]$waited.Elapsed.TotalMilliseconds)ms " +
      "(budget $([int]$uninstallTimeout.TotalMilliseconds)ms): $bOnly")
  }
  if (-not (Test-Path $unrelated -PathType Leaf)) {
    throw "version B uninstaller deleted an unrelated file"
  }

  Remove-Item $unrelated -Force

  # Remove-Item without -Recurse only succeeds on an EMPTY directory, so this
  # line was an assertion pretending to be cleanup - and it reports a non-empty
  # directory as "Object reference not set to an instance of an object", which
  # says nothing about what survived.  Assert it properly, then clean up.
  # uninstall.exe is excluded by construction, not by convenience: this test
  # runs the uninstaller IN PLACE with _?=, and an in-place uninstaller cannot
  # delete its own open image.  A real user's uninstall self-copies and does
  # remove it.  Everything else here is a file the uninstaller owned and should
  # have deleted.
  $survivingFiles = @(
    Get-ChildItem -Path $installDir -Recurse -File -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -ne $uninstaller }
  )
  if ($survivingFiles.Count -gt 0) {
    throw ("the uninstaller left files behind: " +
      (($survivingFiles | ForEach-Object { $_.FullName }) -join ", "))
  }
  # Empty directories are reported, not fatal: NSIS RMDir without /r
  # deliberately preserves a directory that still holds anything, so leftover
  # empty ones are untidy rather than a broken uninstall.
  $survivingDirs = @(
    Get-ChildItem -Path $installDir -Recurse -Directory -ErrorAction SilentlyContinue
  )
  if ($survivingDirs.Count -gt 0) {
    Write-Host ("note: uninstall left empty directories: " +
      (($survivingDirs | ForEach-Object { $_.FullName }) -join ", "))
  }
  Remove-Item $installDir -Recurse -Force
} finally {
  if ($installed -and (Test-Path $uninstaller -PathType Leaf)) {
    Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait | Out-Null
  }
  if (Test-Path $unrelated -PathType Leaf) {
    Remove-Item $unrelated -Force
  }
  if (Test-Path $installDir) {
    Remove-Item $installDir -Recurse -Force
  }
}
