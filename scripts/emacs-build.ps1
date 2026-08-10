#Requires -Version 7.0
[CmdletBinding()]
param(
  [switch]$SkipBuild,
  [switch]$SkipPackage,
  [string]$Version,
  [string]$GStreamerRoot,
  [string]$GStreamerRuntimeMsi,
  [string[]]$DependencyRoot = @()
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$targetTriple = 'x86_64-pc-windows-msvc'
$runtimeExtractionRoot = Join-Path ([System.IO.Path]::GetTempPath()) "neomacs-gstreamer-runtime-$([guid]::NewGuid().ToString('N'))"

function Get-CommandPath {
  param([Parameter(Mandatory)][string]$Name)

  $command = Get-Command -Name $Name -ErrorAction SilentlyContinue |
    Where-Object CommandType -eq 'Application' |
    Select-Object -First 1
  if ($null -ne $command) {
    return $command.Source
  }
  return $null
}

function Normalize-Version {
  param([Parameter(Mandatory)][string]$Value)

  $normalized = $Value.Trim()
  if ($normalized.StartsWith('v', [System.StringComparison]::Ordinal)) {
    $normalized = $normalized.Substring(1)
  }
  if ([string]::IsNullOrWhiteSpace($normalized)) {
    throw 'version must not be empty'
  }
  if ($normalized -eq '.' -or $normalized -eq '..' -or $normalized.EndsWith('.')) {
    throw "version is not safe for a directory name: $Value"
  }
  foreach ($invalidCharacter in [System.IO.Path]::GetInvalidFileNameChars()) {
    if ($normalized.IndexOf($invalidCharacter) -ge 0) {
      throw "version contains an invalid filename character: $Value"
    }
  }
  return $normalized
}

function Get-GitCommit {
  $gitPath = Get-CommandPath 'git.exe'
  if ($null -eq $gitPath) {
    $gitPath = Get-CommandPath 'git'
  }
  if ($null -eq $gitPath) {
    return 'unknown'
  }

  $commitOutput = & $gitPath -C $repoRoot rev-parse --short=12 HEAD 2>$null
  $commitExitCode = $LASTEXITCODE
  if ($commitExitCode -eq 0) {
    $commit = (@($commitOutput) -join '').Trim()
    if ($commit) {
      return $commit
    }
  }

  return 'unknown'
}

function Get-Version {
  $gitPath = Get-CommandPath 'git.exe'
  if ($null -eq $gitPath) {
    $gitPath = Get-CommandPath 'git'
  }
  if ($null -eq $gitPath) {
    return '0.0.0-dev'
  }

  $tagOutput = & $gitPath -C $repoRoot describe --tags --abbrev=0 2>$null
  $tagExitCode = $LASTEXITCODE
  if ($tagExitCode -eq 0) {
    $tag = (@($tagOutput) -join '').Trim()
    if ($tag) {
      return (Normalize-Version $tag)
    }
  }

  $commit = Get-GitCommit
  if ($commit -ne 'unknown') {
    return (Normalize-Version $commit)
  }

  return '0.0.0-dev'
}

function Get-GitBashPath {
  $candidates = @()
  $gitPath = Get-CommandPath 'git.exe'
  if ($null -eq $gitPath) {
    $gitPath = Get-CommandPath 'git'
  }
  if ($null -ne $gitPath) {
    $gitRoot = Split-Path -Parent (Split-Path -Parent $gitPath)
    $candidates += Join-Path $gitRoot 'bin\bash.exe'
    $candidates += Join-Path $gitRoot 'usr\bin\bash.exe'
  }
  foreach ($programFiles in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
    if ([string]::IsNullOrWhiteSpace($programFiles)) {
      continue
    }
    $candidates += Join-Path $programFiles 'Git\bin\bash.exe'
  }
  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  throw 'Git Bash (bash.exe) was not found with Git or under Program Files\Git\bin'
}

function Get-DumpbinPath {
  $dumpbinPath = Get-CommandPath 'dumpbin.exe'
  if ($null -ne $dumpbinPath) {
    return $dumpbinPath
  }

  $vswherePath = Get-CommandPath 'vswhere.exe'
  if ($null -eq $vswherePath) {
    $vswhereCandidates = @()
    foreach ($programFiles in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
      if (-not [string]::IsNullOrWhiteSpace($programFiles)) {
        $vswhereCandidates += Join-Path $programFiles 'Microsoft Visual Studio\Installer\vswhere.exe'
      }
    }
    $vswherePath = $vswhereCandidates |
      Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
      Select-Object -First 1
  }
  if ($null -eq $vswherePath) {
    throw 'dumpbin.exe was not found on PATH and vswhere.exe is unavailable'
  }

  $installations = & $vswherePath `
    -latest `
    -products '*' `
    -requires 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64' `
    -property installationPath 2>$null
  $vswhereExitCode = $LASTEXITCODE
  if ($vswhereExitCode -ne 0) {
    throw "vswhere.exe failed while locating an MSVC installation (exit code $vswhereExitCode)"
  }

  foreach ($installation in @($installations)) {
    $installationPath = $installation.ToString().Trim()
    if (-not $installationPath) {
      continue
    }

    $dumpbin = Get-ChildItem -Path (Join-Path $installationPath 'VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe') `
      -File `
      -ErrorAction SilentlyContinue |
      Sort-Object FullName -Descending |
      Select-Object -First 1
    if ($null -ne $dumpbin) {
      return $dumpbin.FullName
    }
  }

  throw 'dumpbin.exe was not found under an MSVC installation reported by vswhere.exe'
}

function Get-KnownDllNames {
  $knownDlls = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
  )
  $registryPaths = @(
    'Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs',
    'Registry::HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs32'
  )

  foreach ($registryPath in $registryPaths) {
    if (-not (Test-Path -LiteralPath $registryPath)) {
      continue
    }
    $properties = Get-ItemProperty -LiteralPath $registryPath
    foreach ($property in $properties.PSObject.Properties) {
      if ($property.Name -like 'PS*' -or $property.Value -isnot [string]) {
        continue
      }
      [void]$knownDlls.Add([System.IO.Path]::GetFileName($property.Value))
    }
  }

  return ,$knownDlls
}

function Test-IgnoredDependency {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][System.Collections.Generic.HashSet[string]]$KnownDlls,
    [Parameter(Mandatory)][string]$SystemDirectory
  )

  if ($Name -match '^(api-ms-|ext-ms-)') {
    return $true
  }
  if ($Name -match '^(VCRUNTIME|MSVCP|CONCRT).*\.dll$' -or $Name -ieq 'ucrtbase.dll') {
    return $true
  }
  if ($KnownDlls.Contains($Name)) {
    return $true
  }
  if (Test-Path -LiteralPath (Join-Path $SystemDirectory $Name) -PathType Leaf) {
    return $true
  }
  return $false
}

function Get-DumpbinDependencies {
  param([Parameter(Mandatory)][string]$FilePath)

  $output = & $script:DumpbinPath /NOLOGO /DEPENDENTS $FilePath 2>$null
  $exitCode = $LASTEXITCODE
  if ($exitCode -ne 0) {
    throw "dumpbin.exe failed for '$FilePath' (exit code $exitCode)"
  }

  $inDependencies = $false
  $dependencies = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
  )
  foreach ($line in @($output)) {
    $text = $line.ToString()
    if ($text -match 'Image has the following dependencies:') {
      $inDependencies = $true
      continue
    }
    if ($inDependencies -and $text -match 'Image has the following delay load dependencies:') {
      break
    }
    if ($inDependencies -and $text -match '^\s+Summary\s*$') {
      break
    }
    if ($inDependencies -and $text -match '^\s+([^\s]+\.dll)\s*$') {
      [void]$dependencies.Add($Matches[1])
    }
  }

  return ,$dependencies
}

function Find-Dependency {
  param(
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][string[]]$SearchRoots
  )

  foreach ($root in $SearchRoots) {
    if ([string]::IsNullOrWhiteSpace($root)) {
      continue
    }
    $normalizedRoot = $root.Trim('"')
    $candidate = Join-Path $normalizedRoot $Name
    if (Test-Path -LiteralPath $candidate -PathType Leaf) {
      return (Resolve-Path -LiteralPath $candidate).Path
    }
  }
  return $null
}

function Register-StagedBinary {
  param(
    [Parameter(Mandatory)][hashtable]$StagedNamesByDirectory,
    [Parameter(Mandatory)][string]$FilePath
  )

  $directory = [System.IO.Path]::GetDirectoryName($FilePath)
  if (-not $StagedNamesByDirectory.ContainsKey($directory)) {
    $StagedNamesByDirectory[$directory] = [System.Collections.Generic.HashSet[string]]::new(
      [System.StringComparer]::OrdinalIgnoreCase
    )
  }
  [void]$StagedNamesByDirectory[$directory].Add(
    [System.IO.Path]::GetFileName($FilePath)
  )
}

function Test-StagedDependency {
  param(
    [Parameter(Mandatory)][hashtable]$StagedNamesByDirectory,
    [Parameter(Mandatory)][string]$ImporterPath,
    [Parameter(Mandatory)][string]$Name,
    [Parameter(Mandatory)][string]$BinDirectory
  )

  $importerDirectory = [System.IO.Path]::GetDirectoryName($ImporterPath)
  if ($StagedNamesByDirectory.ContainsKey($importerDirectory) -and
      $StagedNamesByDirectory[$importerDirectory].Contains($Name)) {
    return $true
  }
  if ($StagedNamesByDirectory.ContainsKey($BinDirectory) -and
      $StagedNamesByDirectory[$BinDirectory].Contains($Name)) {
    return $true
  }
  return $false
}

function Test-GStreamerPkgConfig {
  param(
    [Parameter(Mandatory)][string]$PkgConfigPath
  )

  foreach ($package in @('glib-2.0', 'gstreamer-1.0', 'cairo', 'pango', 'pangocairo')) {
    & $PkgConfigPath --exists $package 2>$null
    $pkgConfigExitCode = $LASTEXITCODE
    if ($pkgConfigExitCode -ne 0) {
      throw "pkg-config could not find required package '$package' using '$PkgConfigPath' (exit code $pkgConfigExitCode); check PKG_CONFIG_PATH and PKG_CONFIG_LIBDIR"
    }
  }
}

try {
  if ($SkipPackage -and $SkipBuild) {
    Write-Output 'build and packaging skipped'
    return
  }

  $setupParameters = @{}
  if (-not [string]::IsNullOrWhiteSpace($GStreamerRoot)) {
    $setupParameters.GStreamerRoot = $GStreamerRoot
  }
  if (-not $SkipBuild) {
    $setupParameters.Install = $true
  }
  if ($PSBoundParameters.ContainsKey('GStreamerRuntimeMsi')) {
    $setupParameters.GStreamerRuntimeMsi = $GStreamerRuntimeMsi
  }
  elseif (-not [string]::IsNullOrWhiteSpace($env:GSTREAMER_RUNTIME_MSI)) {
    $setupParameters.GStreamerRuntimeMsi = $env:GSTREAMER_RUNTIME_MSI
  }
  & (Join-Path $PSScriptRoot 'setup-windows-gstreamer.ps1') @setupParameters

  $developmentRoot = $null
  if (-not $SkipBuild) {
    $developmentRoot = $env:GSTREAMER_ROOT_X86_64
    if ([string]::IsNullOrWhiteSpace($developmentRoot)) {
      throw 'GSTREAMER_ROOT_X86_64 is not set'
    }
    if (-not (Test-Path -LiteralPath $developmentRoot -PathType Container)) {
      throw "GSTREAMER_ROOT_X86_64 does not exist: $developmentRoot"
    }
    $pkgConfig = $env:PKG_CONFIG
    if ([string]::IsNullOrWhiteSpace($pkgConfig)) {
      throw 'PKG_CONFIG is not set after GStreamer setup'
    }
    if (-not (Test-Path -LiteralPath $pkgConfig -PathType Leaf)) {
      throw "PKG_CONFIG does not exist: $pkgConfig"
    }
    Test-GStreamerPkgConfig -PkgConfigPath $pkgConfig
  }

  if (-not $SkipPackage) {
    if ($PSBoundParameters.ContainsKey('Version')) {
      $version = Normalize-Version $Version
    }
    else {
      $version = Get-Version
    }

    $runtimeMsi = $env:GSTREAMER_RUNTIME_MSI
    if ([string]::IsNullOrWhiteSpace($runtimeMsi)) {
      throw 'GStreamer runtime MSI is not set; use -GStreamerRuntimeMsi or GSTREAMER_RUNTIME_MSI'
    }
    if (-not (Test-Path -LiteralPath $runtimeMsi -PathType Leaf)) {
      throw "GStreamer runtime MSI does not exist: $runtimeMsi"
    }

    $gitBashPath = Get-GitBashPath
    $script:DumpbinPath = Get-DumpbinPath
  }

  if (-not $SkipBuild) {
    $buildGitBashDirectory = Split-Path -Parent (Get-GitBashPath)
    $gitUsrBin = if ((Split-Path -Leaf (Split-Path -Parent $buildGitBashDirectory)) -ieq 'usr') {
      $buildGitBashDirectory
    } else {
      Join-Path (Split-Path -Parent $buildGitBashDirectory) 'usr\bin'
    }
    $awkPath = Join-Path $gitUsrBin 'awk.exe'
    if (-not (Test-Path -LiteralPath $awkPath -PathType Leaf)) {
      throw "Git awk.exe does not exist: $awkPath"
    }
    $previousPath = $env:PATH
    Push-Location $repoRoot
    try {
      $env:PATH = "$gitUsrBin;$env:PATH"
      & cargo xtask fresh-build --release --features neomacs-layout-engine/freetype-bundled
      $buildExitCode = $LASTEXITCODE
      if ($buildExitCode -ne 0) {
        throw "cargo xtask fresh-build failed (exit code $buildExitCode)"
      }
    }
    finally {
      $env:PATH = $previousPath
      Pop-Location
    }
  }

  if ($SkipPackage) {
    Write-Output 'build completed; packaging skipped'
    return
  }

  $packageName = "neomacs-$version-$targetTriple"
  $distDirectory = Join-Path $repoRoot 'dist'
  $archivePath = Join-Path $distDirectory "$packageName.zip"
  $packageRoot = Join-Path $distDirectory $packageName
  $binDirectory = Join-Path $packageRoot 'bin'

  New-Item -ItemType Directory -Path $distDirectory -Force | Out-Null
  if (Test-Path -LiteralPath $packageRoot) {
    Remove-Item -LiteralPath $packageRoot -Recurse -Force
  }

  $releaseDirectory = Join-Path $repoRoot 'target\release'
  $requiredArtifacts = @(
    (Join-Path $releaseDirectory 'neomacs.exe'),
    (Join-Path $releaseDirectory 'neomacsclient.exe'),
    (Join-Path $releaseDirectory 'neomacs.pdump')
  )
  foreach ($artifact in $requiredArtifacts) {
    if (-not (Test-Path -LiteralPath $artifact -PathType Leaf)) {
      throw "missing required release artifact: $artifact"
    }
  }

  New-Item -ItemType Directory -Path $binDirectory -Force | Out-Null
  $neomacsShare = Join-Path $packageRoot 'share\neomacs'
  New-Item -ItemType Directory -Path $neomacsShare -Force | Out-Null

  foreach ($artifact in $requiredArtifacts) {
    Copy-Item -LiteralPath $artifact -Destination $binDirectory
  }

  foreach ($directoryName in @('lisp', 'etc', 'leim')) {
    $sourceDirectory = Join-Path $repoRoot $directoryName
    if (-not (Test-Path -LiteralPath $sourceDirectory -PathType Container)) {
      throw "missing required package directory: $sourceDirectory"
    }
    Copy-Item -LiteralPath $sourceDirectory `
      -Destination (Join-Path $neomacsShare $directoryName) `
      -Recurse
  }

  $infoDirectory = Join-Path $repoRoot 'info'
  if (Test-Path -LiteralPath $infoDirectory -PathType Container) {
    Copy-Item -LiteralPath $infoDirectory `
      -Destination (Join-Path $neomacsShare 'info') `
      -Recurse
  }

  foreach ($fileName in @('README.md', 'COPYING')) {
    $sourceFile = Join-Path $repoRoot $fileName
    if (-not (Test-Path -LiteralPath $sourceFile -PathType Leaf)) {
      throw "missing required package file: $sourceFile"
    }
    Copy-Item -LiteralPath $sourceFile -Destination $packageRoot
  }
  [System.IO.File]::WriteAllText(
    (Join-Path $packageRoot 'VERSION'),
    ((@(
      'name: neomacs'
      "target: $targetTriple"
      "git: $(Get-GitCommit)"
      "built: $([System.DateTime]::UtcNow.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", [System.Globalization.CultureInfo]::InvariantCulture))"
    ) -join "`n") + "`n"),
    [System.Text.UTF8Encoding]::new($false)
  )

  New-Item -ItemType Directory -Path $runtimeExtractionRoot -Force | Out-Null
  $msiArguments = @(
    '/a'
    "`"$runtimeMsi`""
    '/qn'
    "TARGETDIR=`"$runtimeExtractionRoot`""
  )
  $msiexecProcess = Start-Process msiexec.exe -Wait -PassThru -ArgumentList $msiArguments
  if ($msiexecProcess.ExitCode -ne 0) {
    throw "msiexec.exe failed while extracting GStreamer runtime MSI (exit code $($msiexecProcess.ExitCode))"
  }

  $runtimeRoot = Join-Path $runtimeExtractionRoot 'PFiles64\gstreamer\1.0\msvc_x86_64'
  if (-not (Test-Path -LiteralPath $runtimeRoot -PathType Container)) {
    throw "extracted GStreamer runtime root does not exist: $runtimeRoot"
  }
  $runtimeBin = Join-Path $runtimeRoot 'bin'
  if (-not (Test-Path -LiteralPath $runtimeBin -PathType Container)) {
    throw "extracted GStreamer bin directory does not exist: $runtimeBin"
  }
  foreach ($runtimeDllName in @('glib-2.0-0.dll', 'gstreamer-1.0-0.dll')) {
    $runtimeDllPath = Join-Path $runtimeBin $runtimeDllName
    if (-not (Test-Path -LiteralPath $runtimeDllPath -PathType Leaf)) {
      throw "required extracted GStreamer runtime DLL is missing: $runtimeDllPath"
    }
  }

  $vendorScript = Join-Path $repoRoot 'scripts\vendor-windows-gstreamer-runtime.sh'
  $hadDevelopmentRoot = Test-Path -LiteralPath 'Env:GSTREAMER_ROOT_X86_64'
  $previousGStreamerRoot = $env:GSTREAMER_ROOT_X86_64
  try {
    $env:GSTREAMER_ROOT_X86_64 = $runtimeRoot
    & $gitBashPath $vendorScript '--package-root' $packageRoot '--bin-dir' $binDirectory
    $vendorExitCode = $LASTEXITCODE
    if ($vendorExitCode -ne 0) {
      throw "GStreamer runtime vendor script failed (exit code $vendorExitCode)"
    }
  }
  finally {
    if ($hadDevelopmentRoot) {
      $env:GSTREAMER_ROOT_X86_64 = $previousGStreamerRoot
    }
    else {
      Remove-Item Env:GSTREAMER_ROOT_X86_64 -ErrorAction SilentlyContinue
    }
  }

  $pluginDirectory = Join-Path $packageRoot 'lib\gstreamer-1.0'
  if (Test-Path -LiteralPath $pluginDirectory -PathType Container) {
    Get-ChildItem -LiteralPath $pluginDirectory -Recurse -File |
      Where-Object { $_.Extension -ieq '.a' -or $_.Extension -ieq '.lib' } |
      Remove-Item -Force
  }

  $dependencyRoots = @((Join-Path $runtimeRoot 'bin'))
  foreach ($root in @($DependencyRoot)) {
    if (-not [string]::IsNullOrWhiteSpace($root)) {
      $dependencyRoots += $root
    }
  }

  $knownDlls = Get-KnownDllNames
  $systemDirectory = [System.Environment]::SystemDirectory
  $pendingFiles = [System.Collections.Generic.Queue[string]]::new()
  $scannedFiles = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
  )
  $stagedNamesByDirectory = @{}

  # Optional plugin trees are outside the strict dependency closure by design.
  foreach ($loaderRoot in @(
    $binDirectory,
    (Join-Path $packageRoot 'libexec')
  )) {
    if (-not (Test-Path -LiteralPath $loaderRoot -PathType Container)) {
      continue
    }
    Get-ChildItem -LiteralPath $loaderRoot -Recurse -File |
      Where-Object { $_.Extension -ieq '.exe' -or $_.Extension -ieq '.dll' } |
      ForEach-Object {
        Register-StagedBinary -StagedNamesByDirectory $stagedNamesByDirectory -FilePath $_.FullName
        $pendingFiles.Enqueue($_.FullName)
      }
  }

  while ($pendingFiles.Count -gt 0) {
    $filePath = $pendingFiles.Dequeue()
    if (-not $scannedFiles.Add($filePath)) {
      continue
    }

    $importedDlls = Get-DumpbinDependencies $filePath
    foreach ($dependencyName in $importedDlls) {
      if (Test-IgnoredDependency `
        -Name $dependencyName `
        -KnownDlls $knownDlls `
        -SystemDirectory $systemDirectory) {
        continue
      }

      if (Test-StagedDependency `
        -StagedNamesByDirectory $stagedNamesByDirectory `
        -ImporterPath $filePath `
        -Name $dependencyName `
        -BinDirectory $binDirectory) {
        continue
      }

      $dependencyPath = Find-Dependency -Name $dependencyName -SearchRoots $dependencyRoots
      if ($null -eq $dependencyPath) {
        throw "unresolved DLL '$dependencyName' imported by '$filePath'"
      }

      $destinationPath = Join-Path $binDirectory $dependencyName
      if (-not (Test-Path -LiteralPath $destinationPath -PathType Leaf)) {
        Copy-Item -LiteralPath $dependencyPath -Destination $destinationPath
      }
      Register-StagedBinary -StagedNamesByDirectory $stagedNamesByDirectory -FilePath $destinationPath
      if ([System.IO.Path]::GetExtension($destinationPath) -ieq '.dll') {
        $pendingFiles.Enqueue((Resolve-Path -LiteralPath $destinationPath).Path)
      }
    }
  }

  if (Test-Path -LiteralPath $archivePath) {
    Remove-Item -LiteralPath $archivePath -Force
  }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [System.IO.Compression.ZipFile]::CreateFromDirectory(
    $packageRoot,
    $archivePath,
    [System.IO.Compression.CompressionLevel]::Optimal,
    $true
  )

  $requiredEntries = @(
    "$packageName/bin/neomacs.exe",
    "$packageName/bin/neomacsclient.exe",
    "$packageName/bin/neomacs.pdump"
  )
  $zip = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
  try {
    $entryNames = [System.Collections.Generic.HashSet[string]]::new(
      [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($entry in $zip.Entries) {
      [void]$entryNames.Add($entry.FullName.Replace('\', '/'))
    }
    foreach ($requiredEntry in $requiredEntries) {
      if (-not $entryNames.Contains($requiredEntry)) {
        throw "ZIP is missing required entry: $requiredEntry"
      }
    }
    foreach ($directoryName in @('lisp', 'etc', 'leim')) {
      $prefix = "$packageName/share/neomacs/$directoryName/"
      if (-not ($entryNames | Where-Object { $_.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) })) {
        throw "ZIP is missing required directory contents: $prefix"
      }
    }
    $sdkOnlyEntries = @($entryNames | Where-Object { $_ -match '(?i)\.(pdb|h|a|lib)$' })
    if ($sdkOnlyEntries.Count -gt 0) {
      throw "release ZIP contains $($sdkOnlyEntries.Count) SDK-only files: $($sdkOnlyEntries -join ', ')"
    }
  }
  finally {
    $zip.Dispose()
  }

  Write-Output "wrote $archivePath"
}
finally {
  try {
    if (Test-Path -LiteralPath $runtimeExtractionRoot) {
      Remove-Item -LiteralPath $runtimeExtractionRoot -Recurse -Force -ErrorAction Stop
    }
  }
  catch {
    Write-Warning "failed to remove runtime MSI extraction directory '$runtimeExtractionRoot': $($_.Exception.Message)"
  }
}
