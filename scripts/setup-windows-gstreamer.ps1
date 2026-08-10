param(
  [switch]$Install,
  [string]$GStreamerRoot,
  [string]$GStreamerRuntimeMsi
)

$ErrorActionPreference = 'Stop'
$runtimeMsiExplicitlySupplied = $PSBoundParameters.ContainsKey('GStreamerRuntimeMsi')

function Resolve-SessionPath([string]$Path) {
  return $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Path)
}

function Normalize-RootPath([string]$Path) {
  $resolved = Resolve-SessionPath $Path
  $pathRoot = [System.IO.Path]::GetPathRoot($resolved)
  if ([string]::IsNullOrEmpty($pathRoot) -or $resolved.Length -le $pathRoot.Length) {
    return $pathRoot
  }
  return $resolved.TrimEnd([char[]]@('\', '/'))
}

function Download-IfMissing($uri, $path) {
  if (Test-Path -LiteralPath $path -PathType Leaf) {
    Write-Host "Reusing downloaded file: $path"
    return
  }
  $parent = [System.IO.Path]::GetDirectoryName($path)
  New-Item -ItemType Directory -Force -Path $parent | Out-Null
  $partialPath = Join-Path $parent ".$([System.IO.Path]::GetFileName($path)).partial-$([guid]::NewGuid().ToString('N'))"
  try {
    Write-Host "Downloading $uri to $path"
    Invoke-WebRequest -Uri $uri -OutFile $partialPath
    if (-not (Test-Path -LiteralPath $partialPath -PathType Leaf)) {
      throw "download did not produce a file: $uri"
    }
    [System.IO.File]::Move($partialPath, $path)
  }
  finally {
    if (Test-Path -LiteralPath $partialPath) {
      try {
        Remove-Item -LiteralPath $partialPath -Force -ErrorAction Stop
      }
      catch {
        Write-Warning "failed to remove partial download '$partialPath': $($_.Exception.Message)"
      }
    }
  }
}

function Expand-MsiPayload($Msi, $TargetDirectory) {
  $msiArguments = @(
    '/a'
    "`"$Msi`""
    '/qn'
    "TARGETDIR=`"$TargetDirectory`""
  )
  $process = Start-Process msiexec.exe -Wait -PassThru -ArgumentList $msiArguments
  if ($process.ExitCode -ne 0) {
    throw "msiexec failed with exit code $($process.ExitCode): $Msi"
  }
}

function Get-GStreamerRootMissingMarkers($Root) {
  $requiredMarkers = @(
    @{ RelativePath = 'bin\pkg-config.exe'; Type = 'Leaf' },
    @{ RelativePath = 'bin\gstreamer-1.0-0.dll'; Type = 'Leaf' },
    @{ RelativePath = 'include'; Type = 'Container' },
    @{ RelativePath = 'lib\pkgconfig\glib-2.0.pc'; Type = 'Leaf' },
    @{ RelativePath = 'lib\pkgconfig\gstreamer-1.0.pc'; Type = 'Leaf' }
  )
  foreach ($marker in $requiredMarkers) {
    $path = Join-Path $Root $marker.RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType $marker.Type)) {
      $marker.RelativePath
    }
  }
}

function Export-CiEnv($name, $value) {
  Set-Item -LiteralPath "Env:$name" -Value $value
  if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_ENV)) {
    Add-Content -LiteralPath $env:GITHUB_ENV -Value "$name=$value"
  }
}

function Export-CiPath($value) {
  $env:PATH = "$value;$env:PATH"
  if (-not [string]::IsNullOrWhiteSpace($env:GITHUB_PATH)) {
    Add-Content -LiteralPath $env:GITHUB_PATH -Value $value
  }
}

function Invoke-GStreamerSetup {
  $repoRoot = Resolve-SessionPath (Join-Path $PSScriptRoot '..')
  $managedGStreamerSuffix = 'PFiles64\gstreamer\1.0\msvc_x86_64'
  $usingDefaultGStreamerRoot = [string]::IsNullOrWhiteSpace($GStreamerRoot)
  $version = if ([string]::IsNullOrWhiteSpace($env:GSTREAMER_VERSION)) {
    '1.26.9'
  } else {
    $env:GSTREAMER_VERSION
  }
  if ($usingDefaultGStreamerRoot) {
    $GStreamerRoot = Join-Path $repoRoot $managedGStreamerSuffix
  }
  $GStreamerRoot = Normalize-RootPath $GStreamerRoot
  $msiCacheRoot = if ($usingDefaultGStreamerRoot) {
    Join-Path $repoRoot 'gstreamer-msi-cache'
  } else {
    "$GStreamerRoot-msi-cache"
  }
  $extractionTarget = $null
  if ($GStreamerRoot.EndsWith($managedGStreamerSuffix, [System.StringComparison]::OrdinalIgnoreCase)) {
    $extractionTarget = $GStreamerRoot.Substring(0, $GStreamerRoot.Length - $managedGStreamerSuffix.Length)
    if ([string]::IsNullOrWhiteSpace($extractionTarget)) {
      $extractionTarget = [System.IO.Path]::GetPathRoot($GStreamerRoot)
    } else {
      $extractionTarget = $extractionTarget.TrimEnd([char[]]@('\', '/'))
    }
  }
  $baseUrl = "https://gstreamer.freedesktop.org/data/pkg/windows/$version/msvc"
  if ($runtimeMsiExplicitlySupplied) {
    if ([string]::IsNullOrWhiteSpace($GStreamerRuntimeMsi)) {
      throw 'explicit GStreamer runtime MSI path must not be empty'
    }
    $runtimeMsi = Resolve-SessionPath $GStreamerRuntimeMsi
    if (-not (Test-Path -LiteralPath $runtimeMsi -PathType Leaf)) {
      throw "explicit GStreamer runtime MSI does not exist: '$GStreamerRuntimeMsi'"
    }
  } else {
    $runtimeMsi = Resolve-SessionPath (Join-Path $msiCacheRoot "gstreamer-1.0-msvc-x86_64-$version.msi")
  }
  $develMsi = Resolve-SessionPath (Join-Path $msiCacheRoot "gstreamer-1.0-devel-msvc-x86_64-$version.msi")

  Write-Host "Selected GStreamer root: $GStreamerRoot"

  $missingMarkers = @(Get-GStreamerRootMissingMarkers $GStreamerRoot)
  $sdkValid = $missingMarkers.Count -eq 0
  if ($Install) {
    if ($sdkValid) {
      Write-Host "Reusing valid GStreamer SDK root: $GStreamerRoot"
    } elseif ($null -eq $extractionTarget) {
      if (Test-Path -LiteralPath $GStreamerRoot) {
        throw "GStreamer SDK root exists but is invalid: '$GStreamerRoot'. Missing required paths: $($missingMarkers -join ', '). Manually remove or rename this directory, then rerun with -Install; setup will not delete it automatically."
      }
      throw "GStreamer SDK root '$GStreamerRoot' is missing and cannot be installed directly. A missing root must end with '$managedGStreamerSuffix'."
    } else {
      Download-IfMissing "$baseUrl/gstreamer-1.0-msvc-x86_64-$version.msi" $runtimeMsi
      Download-IfMissing "$baseUrl/gstreamer-1.0-devel-msvc-x86_64-$version.msi" $develMsi
      Expand-MsiPayload $runtimeMsi $extractionTarget
      Expand-MsiPayload $develMsi $extractionTarget
      $missingMarkers = @(Get-GStreamerRootMissingMarkers $GStreamerRoot)
      if ($missingMarkers.Count -ne 0) {
        throw "GStreamer SDK payload is missing required paths: $($missingMarkers -join ', ')"
      }
      $sdkValid = $true
    }
  }

  Download-IfMissing "$baseUrl/gstreamer-1.0-msvc-x86_64-$version.msi" $runtimeMsi

  if (-not $sdkValid) {
    Write-Warning "GStreamer SDK root '$GStreamerRoot' is not valid; missing required paths: $($missingMarkers -join ', '). Continuing with runtime MSI only. Provide a valid SDK root or manually remove or rename this directory and rerun with -Install to install the SDK."
  } else {
    $pkgConfig = Join-Path $GStreamerRoot 'bin\pkg-config.exe'
    Export-CiPath (Join-Path $GStreamerRoot 'bin')
    Export-CiEnv 'GSTREAMER_ROOT_X86_64' $GStreamerRoot
    Export-CiEnv 'PKG_CONFIG' $pkgConfig
    Export-CiEnv 'PKG_CONFIG_PATH' (Join-Path $GStreamerRoot 'lib\pkgconfig')
    Export-CiEnv 'PKG_CONFIG_LIBDIR' (Join-Path $GStreamerRoot 'lib\pkgconfig')
  }
  Export-CiEnv 'GSTREAMER_RUNTIME_MSI' $runtimeMsi
}

if ($MyInvocation.InvocationName -ne '.') {
  Invoke-GStreamerSetup
}
