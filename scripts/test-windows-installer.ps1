[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$InstallerPath,

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

function Assert-Equal {
  param(
    [AllowNull()]$Actual,
    [AllowNull()]$Expected,
    [string]$Message
  )

  if ($Actual -cne $Expected) {
    throw "$Message`nexpected: $Expected`nactual:   $Actual"
  }
}

function Assert-RegistryKeyAbsent {
  param(
    [Microsoft.Win32.RegistryKey]$Root,
    [string]$Path,
    [string]$Message
  )

  $key = $Root.OpenSubKey($Path)
  if ($null -ne $key) {
    $key.Dispose()
    throw $Message
  }
}

function Invoke-Installer {
  param([string]$Path)

  $process = Start-Process -FilePath $Path -ArgumentList "/S" -PassThru -Wait
  if ($process.ExitCode -ne 0) {
    throw "installer exited with code $($process.ExitCode)"
  }
}

function Read-RawRegistryValue {
  param(
    [Microsoft.Win32.RegistryKey]$Key,
    [string]$Name
  )

  return $Key.GetValue(
    $Name,
    $null,
    [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
  )
}

$installer = (Resolve-Path $InstallerPath).Path
$productName = "NEO Emacs"
$productRegistrationName = "$productName (User)"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\$productName"
$binDir = Join-Path $installDir "bin"
$uninstaller = Join-Path $installDir "uninstall.exe"
$uninstallKeyPath = "Software\Microsoft\Windows\CurrentVersion\Uninstall\$productRegistrationName"
$appPathsRoot = "Software\Microsoft\Windows\CurrentVersion\App Paths"
$neomacsAppPath = "$appPathsRoot\neomacs.exe"
$clientAppPath = "$appPathsRoot\neomacsclient.exe"
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$productName"

$userEnvironment = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("Environment")
$machineEnvironment = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
  "SYSTEM\CurrentControlSet\Control\Session Manager\Environment"
)
if ($null -eq $machineEnvironment) {
  throw "machine environment registry key is missing"
}

$originalUserPathExists = $userEnvironment.GetValueNames() -contains "Path"
$originalUserPath = Read-RawRegistryValue -Key $userEnvironment -Name "Path"
$originalUserPathKind = if ($originalUserPathExists) {
  $userEnvironment.GetValueKind("Path")
} else {
  $null
}
$originalMachinePath = Read-RawRegistryValue -Key $machineEnvironment -Name "Path"
$originalMachinePathKind = $machineEnvironment.GetValueKind("Path")

$fixtureSegments = 0..180 | ForEach-Object {
  "C:\neomacs-installer-contract\segment-$('{0:D3}' -f $_)"
}
$fixtureUserPath = "%LOCALAPPDATA%\Microsoft\WindowsApps;" + ($fixtureSegments -join ";")
if ($fixtureUserPath.Length -le 1024) {
  throw "test fixture must exceed the default NSIS string limit"
}

$installed = $false
$mutatedAppPath = $false
try {
  $userEnvironment.SetValue(
    "Path",
    $fixtureUserPath,
    [Microsoft.Win32.RegistryValueKind]::ExpandString
  )

  # A second install verifies that registration is idempotent and upgrades do
  # not create duplicate installer-owned state.
  $installed = $true
  Invoke-Installer -Path $installer
  Invoke-Installer -Path $installer

  Assert-Equal -Actual (Read-RawRegistryValue -Key $userEnvironment -Name "Path") `
    -Expected $fixtureUserPath -Message "installer changed the current user's Path"
  Assert-Equal -Actual $userEnvironment.GetValueKind("Path") `
    -Expected ([Microsoft.Win32.RegistryValueKind]::ExpandString) `
    -Message "installer changed the current user's Path registry type"
  Assert-Equal -Actual (Read-RawRegistryValue -Key $machineEnvironment -Name "Path") `
    -Expected $originalMachinePath -Message "installer changed the machine Path"
  Assert-Equal -Actual $machineEnvironment.GetValueKind("Path") `
    -Expected $originalMachinePathKind `
    -Message "installer changed the machine Path registry type"

  foreach ($registration in @(
    @{ Key = $neomacsAppPath; Executable = "neomacs.exe" },
    @{ Key = $clientAppPath; Executable = "neomacsclient.exe" }
  )) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($registration.Key)
    if ($null -eq $key) {
      throw "missing App Paths registration for $($registration.Executable)"
    }
    try {
      Assert-Equal -Actual $key.GetValue("") `
        -Expected (Join-Path $binDir $registration.Executable) `
        -Message "App Paths executable target is wrong"
      Assert-Equal $key.GetValue("Path") $binDir "App Paths process path is wrong"
    } finally {
      $key.Dispose()
    }
  }

  $uninstallKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($uninstallKeyPath)
  if ($null -eq $uninstallKey) {
    throw "missing per-user Apps & Features registration"
  }
  try {
    Assert-Equal $uninstallKey.GetValue("DisplayName") $productRegistrationName `
      "wrong Apps & Features display name"
    Assert-Equal $uninstallKey.GetValue("InstallLocation") $installDir "wrong install location"
    Assert-Equal $uninstallKey.GetValue("NoModify") 1 "NoModify must be set"
    Assert-Equal $uninstallKey.GetValue("NoRepair") 1 "NoRepair must be set"
  } finally {
    $uninstallKey.Dispose()
  }

  foreach ($executable in @("neomacs.exe", "neomacsclient.exe")) {
    $path = Join-Path $binDir $executable
    if (-not (Test-Path $path -PathType Leaf)) {
      throw "installed command is missing: $path"
    }
    # The App Paths values were asserted above. PowerShell's Start-Process does
    # not reliably consult App Paths when resolving a bare executable name, so
    # use the validated target for the executable smoke test.
    $process = Start-Process -FilePath $path -ArgumentList "--version" -PassThru -Wait
    if ($process.ExitCode -ne 0) {
      throw "$executable --version exited with code $($process.ExitCode)"
    }
  }

  # Issue #317: a normal Windows environment has no Unix SHELL. GNU Emacs
  # supplies its private cmdproxy in that case, and `M-! whoami` must work in
  # the installed tree rather than searching for /bin/sh.
  $savedShell = $env:SHELL
  try {
    Remove-Item Env:SHELL -ErrorAction SilentlyContinue
    $neomacs = Join-Path $binDir "neomacs.exe"
    # Windows PowerShell 5.1 does not escape embedded double quotes when it
    # builds the native command line, so each elisp string quote must arrive
    # backslash-escaped for CommandLineToArgvW (\" decodes to a literal ";
    # the backslash of \n is not followed by a quote, so it survives as-is).
    $shellProbeExpression = `
      '(progn (princ (format \"shell=%s\n\" shell-file-name)) (princ (shell-command-to-string \"whoami\")))'
    $shellProbe = @(& $neomacs "--batch" "-Q" "--eval" $shellProbeExpression 2>&1)
    if ($LASTEXITCODE -ne 0) {
      throw "installed shell-command probe exited with code $LASTEXITCODE`n$($shellProbe -join "`n")"
    }
    $shellProbeText = $shellProbe -join "`n"
    if ($shellProbeText -notmatch 'shell=.*cmdproxy\.exe') {
      throw "shell-file-name does not select the packaged cmdproxy:`n$shellProbeText"
    }
    if ($shellProbeText -notmatch '(?m)^.+\\.+$') {
      throw "shell-command did not return a Windows identity:`n$shellProbeText"
    }
  } finally {
    if ($null -eq $savedShell) {
      Remove-Item Env:SHELL -ErrorAction SilentlyContinue
    } else {
      $env:SHELL = $savedShell
    }
  }

  foreach ($shortcut in @(
    (Join-Path $startMenuDir "$productName.lnk"),
    (Join-Path $startMenuDir "Uninstall $productName.lnk")
  )) {
    if (-not (Test-Path $shortcut -PathType Leaf)) {
      throw "installer-owned shortcut is missing: $shortcut"
    }
  }

  # If another program takes ownership of an App Paths key after install, the
  # Neomacs uninstaller must leave that changed key intact.
  $replacementExecutable = "C:\replacement-owner\neomacs.exe"
  $changedAppPath = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($neomacsAppPath, $true)
  if ($null -eq $changedAppPath) {
    throw "cannot mutate neomacs.exe App Paths registration for ownership test"
  }
  try {
    $changedAppPath.SetValue("", $replacementExecutable)
  } finally {
    $changedAppPath.Dispose()
  }
  $mutatedAppPath = $true

  $unrelatedFile = Join-Path $installDir "created-after-install.txt"
  Set-Content -Path $unrelatedFile -Value "not owned by the Neomacs installer" -NoNewline

  # Simulate another installer changing Path after Neomacs was installed.
  $postInstallUserPath = "$fixtureUserPath;C:\installed-after-neomacs"
  $userEnvironment.SetValue(
    "Path",
    $postInstallUserPath,
    [Microsoft.Win32.RegistryValueKind]::ExpandString
  )

  $process = Start-Process -FilePath $uninstaller -ArgumentList "/S" -PassThru -Wait
  if ($process.ExitCode -ne 0) {
    throw "uninstaller exited with code $($process.ExitCode)"
  }
  $installed = $false

  for ($attempt = 0; $attempt -lt 50 -and (Test-Path (Join-Path $binDir "neomacs.exe")); $attempt++) {
    Start-Sleep -Milliseconds 100
  }

  Assert-Equal -Actual (Read-RawRegistryValue -Key $userEnvironment -Name "Path") `
    -Expected $postInstallUserPath `
    -Message "uninstaller changed unrelated current-user Path state"
  Assert-Equal -Actual $userEnvironment.GetValueKind("Path") `
    -Expected ([Microsoft.Win32.RegistryValueKind]::ExpandString) `
    -Message "uninstaller changed the current user's Path registry type"
  Assert-Equal -Actual (Read-RawRegistryValue -Key $machineEnvironment -Name "Path") `
    -Expected $originalMachinePath -Message "uninstaller changed the machine Path"
  Assert-Equal -Actual $machineEnvironment.GetValueKind("Path") `
    -Expected $originalMachinePathKind `
    -Message "uninstaller changed the machine Path registry type"

  $preservedAppPath = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($neomacsAppPath)
  if ($null -eq $preservedAppPath) {
    throw "uninstaller deleted an App Paths key changed after installation"
  }
  try {
    Assert-Equal -Actual $preservedAppPath.GetValue("") `
      -Expected $replacementExecutable `
      -Message "uninstaller changed an App Paths value owned by another program"
  } finally {
    $preservedAppPath.Dispose()
  }
  Assert-RegistryKeyAbsent -Root ([Microsoft.Win32.Registry]::CurrentUser) `
    -Path $clientAppPath -Message "uninstaller left the neomacsclient.exe App Paths key"
  Assert-RegistryKeyAbsent -Root ([Microsoft.Win32.Registry]::CurrentUser) `
    -Path $uninstallKeyPath -Message "uninstaller left its Apps & Features key"

  if (Test-Path $startMenuDir) {
    throw "uninstaller left its Start Menu directory"
  }
  if (Test-Path (Join-Path $binDir "neomacs.exe")) {
    throw "uninstaller left an installer-owned executable"
  }
  if (-not (Test-Path $unrelatedFile -PathType Leaf)) {
    throw "uninstaller deleted a file created after installation"
  }
  Remove-Item $unrelatedFile -Force
  Remove-Item $installDir -Force
} finally {
  if ($installed -and (Test-Path $uninstaller -PathType Leaf)) {
    Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait | Out-Null
  }

  if ($originalUserPathExists) {
    $userEnvironment.SetValue("Path", $originalUserPath, $originalUserPathKind)
  } else {
    $userEnvironment.DeleteValue("Path", $false)
  }
  if ($mutatedAppPath) {
    [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($neomacsAppPath, $false)
  }
  $userEnvironment.Dispose()
  $machineEnvironment.Dispose()
}
