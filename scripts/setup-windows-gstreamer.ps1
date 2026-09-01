param(
  [switch]$Install
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($env:GSTREAMER_VERSION)) {
  throw "GSTREAMER_VERSION is not set"
}

$gstreamerArch = if ([string]::IsNullOrWhiteSpace($env:GSTREAMER_ARCH)) {
  "x86_64"
} else {
  $env:GSTREAMER_ARCH
}
if ($gstreamerArch -notin @("x86_64", "arm64")) {
  throw "unsupported GSTREAMER_ARCH: $gstreamerArch"
}

$installRoot = "C:\gstreamer"
$installerCacheRoot = "C:\gstreamer-installer-cache"
$baseUrl = "https://gstreamer.freedesktop.org/data/pkg/windows/$env:GSTREAMER_VERSION/msvc"
$installerName = "gstreamer-1.0-msvc-$gstreamerArch-$env:GSTREAMER_VERSION.exe"
$installerPath = Join-Path $installerCacheRoot $installerName

function Download-IfMissing($uri, $path) {
  if (Test-Path $path) {
    return
  }
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
  Invoke-WebRequest -Uri $uri -OutFile $path
}

function Install-GStreamer($path) {
  $process = Start-Process $path -Wait -PassThru -ArgumentList @(
    "/VERYSILENT",
    "/NORESTART",
    "/ALLUSERS",
    "/TYPE=devel",
    "/DIR=$installRoot"
  )
  if ($process.ExitCode -ne 0) {
    throw "GStreamer installer failed with exit code $($process.ExitCode): $path"
  }
}

function Export-CiEnv($name, $value) {
  if ($env:GITHUB_ENV) {
    Add-Content -Path $env:GITHUB_ENV -Value "$name=$value"
  } else {
    Set-Item -Path "Env:$name" -Value $value
  }
}

function Export-CiPath($value) {
  if ($env:GITHUB_PATH) {
    Add-Content -Path $env:GITHUB_PATH -Value $value
  } else {
    $env:PATH = "$value;$env:PATH"
  }
}

if ($Install) {
  Download-IfMissing "$baseUrl/$installerName" $installerPath
  Install-GStreamer $installerPath
}

$searchRoots = @($installRoot, "${env:ProgramFiles}\gstreamer", "${env:ProgramFiles(x86)}\gstreamer") |
  Where-Object { Test-Path $_ }
$glibPc = $searchRoots |
  ForEach-Object { Get-ChildItem -Path $_ -Filter glib-2.0.pc -Recurse -ErrorAction SilentlyContinue } |
  Select-Object -First 1

if (-not $glibPc) {
  $searchRoots | ForEach-Object { Get-ChildItem -Path $_ -Depth 4 -ErrorAction SilentlyContinue }
  throw "glib-2.0.pc not found; restore or install the GStreamer development files first"
}

$pkgConfigDir = Split-Path -Parent $glibPc.FullName
$libDir = Split-Path -Parent $pkgConfigDir
$gstRoot = Split-Path -Parent $libDir
$pkgConfig = "$gstRoot\bin\pkg-config.exe"

if (-not (Test-Path $pkgConfig)) {
  choco install pkgconfiglite -y
  $pkgConfig = (Get-Command pkg-config.exe -All |
    Where-Object { $_.Source -notmatch "\\Git\\" } |
    Select-Object -First 1 -ExpandProperty Source)
  if (-not $pkgConfig) {
    throw "pkg-config.exe not found after installing pkgconfiglite"
  }
}

Export-CiPath (Split-Path -Parent $pkgConfig)
Export-CiPath "$gstRoot\bin"
Export-CiEnv "GSTREAMER_ARCH" $gstreamerArch
Export-CiEnv "GSTREAMER_ROOT" "$gstRoot\"
if ($gstreamerArch -eq "x86_64") {
  Export-CiEnv "GSTREAMER_ROOT_X86_64" "$gstRoot\"
} else {
  Export-CiEnv "GSTREAMER_ROOT_ARM64" "$gstRoot\"
}
Export-CiEnv "PKG_CONFIG" $pkgConfig
Export-CiEnv "PKG_CONFIG_PATH" "$gstRoot\lib\pkgconfig"
Export-CiEnv "GSTREAMER_INSTALLER" $installerPath
