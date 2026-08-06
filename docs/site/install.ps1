<#
.SYNOPSIS
    Fyr installer for Windows — https://fyr.guide/install.ps1
.DESCRIPTION
    Installs or updates the Fyr Docker container on Windows.
    Settings are persisted in $env:APPDATA\fyr\install.conf for future updates.
.PARAMETER Tag
    Docker image tag (default: latest). Accepts any tag string.
.PARAMETER Update
    Recreate container with latest image (preserves data).
.PARAMETER DataDir
    Bind-mount a host directory as the data volume.
.PARAMETER DataVolume
    Use a named Docker volume (default: fyr-data).
.PARAMETER Port
    Host port to expose (default: 8080).
.PARAMETER AdminPassword
    Enable admin mode with the given password.
.PARAMETER ReadOnly
    Enable strict read-only mode (no mutations).
.PARAMETER Help
    Show this help message.
.EXAMPLE
    irm https://fyr.guide/install.ps1 | iex
.EXAMPLE
    irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Update
.EXAMPLE
    irm https://fyr.guide/install.ps1 | iex; Install-Fyr -DataDir C:\fyr-data -Port 9090
.EXAMPLE
    irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Legacy
#>


param(
    [string]$Tag = "",
    [switch]$Update,
    [switch]$Legacy,
    [switch]$PcLegacy,
    [switch]$RpiLegacy,
    [string]$DataDir = "",
    [string]$DataVolume = "",
    [string]$Port = "",
    [Alias('AdminPassword')]
    [string]$AdminSecret = "",
    [switch]$ReadOnly,
    [switch]$Help
)

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
$ContainerName = "fyr"
$ImageRepo = "hexagon/fyr"
$DefaultTag = "latest"
$DefaultPort = "8080"
$DefaultDataVolume = "fyr-data"
$ConfigDir = Join-Path $env:APPDATA "fyr"
$ConfigFile = Join-Path $ConfigDir "install.conf"

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------
if ($Help) {
    @"
Fyr installer for Windows — https://fyr.guide/install.ps1

Usage:
  irm https://fyr.guide/install.ps1 | iex
  irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Update

Parameters:
    -Tag <tag>             Docker image tag (default: latest, optimized)
  -Update                Recreate container with latest image (preserves data)
    -Legacy                Use the legacy compatibility tag for this platform
    -PcLegacy              Use the x86_64 legacy compatibility tag
    -RpiLegacy             Use the arm64 Raspberry Pi legacy compatibility tag
  -DataDir <path>        Bind-mount a host directory as the data volume
  -DataVolume <name>     Use a named Docker volume (default: fyr-data)
  -Port <number>         Host port to expose (default: 8080)
  -AdminPassword <pw>    Enable admin mode with the given password
  -ReadOnly              Enable strict read-only mode (no mutations)
  -Help                  Show this help message

Notes:
  latest/dev/version tags now prefer CPU-optimized images.
  Use -Legacy on older or mixed hardware that needs broader compatibility.
  -DataDir and -DataVolume are mutually exclusive.
  If neither is given, a named Docker volume 'fyr-data' is used.
  Settings are persisted in `$env:APPDATA\fyr\install.conf for future updates.
"@
    return
}

function Resolve-LegacySuffix {
    param([ValidateSet('auto','pc','rpi')] [string]$Mode = 'auto')

    switch ($Mode) {
        'pc' { return 'pc-legacy' }
        'rpi' { return 'rpi-legacy' }
        default {
            switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
                'X64' { return 'pc-legacy' }
                'Arm64' { return 'rpi-legacy' }
                default { return 'legacy' }
            }
        }
    }
}

function Resolve-LegacyTag {
    param(
        [string]$CurrentTag,
        [ValidateSet('auto','pc','rpi')] [string]$Mode = 'auto'
    )

    $suffix = Resolve-LegacySuffix -Mode $Mode
    switch -Regex ($CurrentTag) {
        '^$|^latest$' { return $suffix }
        '^dev$' { return "dev-$suffix" }
        '^amd64-avx2$' { return 'pc-legacy' }
        '^arm64-dotprod$' { return 'rpi-legacy' }
        '^dev-amd64-avx2$' { return 'dev-pc-legacy' }
        '^dev-arm64-dotprod$' { return 'dev-rpi-legacy' }
        'legacy|pc-legacy|rpi-legacy$' { return $CurrentTag }
        '^v.+$' { return "$CurrentTag-$suffix" }
        default { return $CurrentTag }
    }
}

# ---------------------------------------------------------------------------
# Config file management
# ---------------------------------------------------------------------------
$Config = @{}
if (Test-Path $ConfigFile) {
    try {
        $Config = Get-Content $ConfigFile -Raw | ConvertFrom-Json -AsHashtable
    } catch {
        # Silently use defaults if config file is unreadable; not critical.
    }
}

# Merge: CLI args override config, config fills in defaults
if (-not $Tag)           { $Tag           = if ($Config.ContainsKey('tag'))           { $Config['tag'] }           else { $DefaultTag } }
if (-not $Port)          { $Port          = if ($Config.ContainsKey('port'))          { $Config['port'] }          else { $DefaultPort } }
if (-not $DataVolume -and -not $DataDir) {
    if ($Config.ContainsKey('data_volume') -and $Config['data_volume']) {
        $DataVolume = $Config['data_volume']
    } elseif ($Config.ContainsKey('data_dir') -and $Config['data_dir']) {
        $DataDir = $Config['data_dir']
    } else {
        $DataVolume = $DefaultDataVolume
    }
}

# Validate mutual exclusivity
if ($DataDir -and $DataVolume) {
    Write-Error "-DataDir and -DataVolume are mutually exclusive. Use one or the other, not both."
    return
}

if ($Legacy -or $PcLegacy -or $RpiLegacy) {
    $legacyMode = if ($PcLegacy) { 'pc' } elseif ($RpiLegacy) { 'rpi' } else { 'auto' }
    $Tag = Resolve-LegacyTag -CurrentTag $Tag -Mode $legacyMode
}

# Write config for next run
if (-not (Test-Path $ConfigDir)) {
    New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
}
$ConfigData = @{
    data_dir            = $DataDir
    data_volume         = $DataVolume
    port                = $Port
    tag                 = $Tag
    admin_password_set  = [bool]$AdminSecret
    readonly            = [bool]$ReadOnly
}
$ConfigData | ConvertTo-Json | Set-Content $ConfigFile -Encoding UTF8

$Image = "${ImageRepo}:${Tag}"

Write-Host "==> Using Docker image: ${Image}"
Write-Host "==> Port: ${Port}"

# ---------------------------------------------------------------------------
# 1. Check if Docker is installed
# ---------------------------------------------------------------------------
try {
    $dockerVersion = docker --version 2>&1
    if (-not $dockerVersion) { throw }
} catch {
    Write-Host "==> ERROR: Docker is required but not installed."
    Write-Host "==> Please install Docker Desktop first: https://docs.docker.com/desktop/setup/install/windows-install/"
    return
}

# ---------------------------------------------------------------------------
# 2. Prepare data storage
# ---------------------------------------------------------------------------
$VolumeArg = ""
if ($DataDir) {
    # Bind mount mode
    Write-Host "==> Using host directory: ${DataDir}"
    if (-not (Test-Path $DataDir)) {
        Write-Host "==> Creating data directory at ${DataDir}..."
        New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
    }
    $VolumeArg = "${DataDir}:/data"
} elseif ($DataVolume) {
    # Named volume mode
    Write-Host "==> Using Docker volume: ${DataVolume}"
    docker volume inspect $DataVolume 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Host "==> Creating Docker volume ${DataVolume}..."
        docker volume create $DataVolume 2>&1 | Out-Null
    }
    $VolumeArg = "${DataVolume}:/data"
}

# ---------------------------------------------------------------------------
# 3. Check if container already exists
# ---------------------------------------------------------------------------
$existingContainer = docker ps -a -q -f "name=^/${ContainerName}$" 2>&1
if ($existingContainer) {
    if ($Update) {
        Write-Host "==> Update flag detected. Stopping and removing old container..."
        docker stop $ContainerName 2>$null | Out-Null
        docker rm $ContainerName 2>$null | Out-Null
    } else {
        Write-Host "==> WARNING: Container '${ContainerName}' is already installed!"
        Write-Host "==> If you want to update/recreate it, run this script with the -Update parameter:"
        Write-Host "==>   irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Update"
        return
    }
}

# ---------------------------------------------------------------------------
# 4. Pre-pull the image
# ---------------------------------------------------------------------------
Write-Host "==> Pre-pulling image ${Image}..."
docker pull $Image

# ---------------------------------------------------------------------------
# 5. Build environment variables
# ---------------------------------------------------------------------------
$EnvFlags = @(
    "-e", "FYR_HOST=0.0.0.0",
    "-e", "DATA_DIR=/data"
)
if ($AdminSecret) {
    $EnvFlags += "-e", "FYR_ADMIN_PASSWORD=${AdminSecret}"
}
if ($ReadOnly) {
    $EnvFlags += "-e", "FYR_READONLY=true"
}

# ---------------------------------------------------------------------------
# 6. Run the container
# ---------------------------------------------------------------------------
Write-Host "==> Starting container ${ContainerName}..."
$dockerArgs = @(
    "run", "-d",
    "--restart", "unless-stopped",
    "-p", "${Port}:8080",
    "--name", $ContainerName
) + $EnvFlags + @(
    "-v", $VolumeArg,
    $Image
)

& docker @dockerArgs

if ($LASTEXITCODE -eq 0) {
    Write-Host "==> Success! Container '${ContainerName}' is up and running."
    Write-Host "==> Access Fyr at http://localhost:${Port} (replace localhost with the server's IP if connecting remotely)."
} else {
    Write-Host "==> ERROR: Failed to start container. Check Docker logs for details."
    return
}