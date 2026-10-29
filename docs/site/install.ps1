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
.PARAMETER Https
    Serve HTTPS with a self-signed certificate (enables GPS / My Location in
    browsers on the LAN).
.PARAMETER TlsCert
    Serve HTTPS with your own certificate chain (PEM file); use with -TlsKey.
.PARAMETER TlsKey
    Private key (PEM file) for -TlsCert.
.PARAMETER TlsSans
    Extra comma-separated names/IPs for the self-signed certificate.
.PARAMETER HttpRedirectPort
    Also listen on this host port with plain HTTP and redirect to HTTPS.
.PARAMETER NoHttps
    Turn HTTPS off again and forget saved HTTPS settings.
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
.EXAMPLE
    irm https://fyr.guide/install.ps1 | iex; Install-Fyr -Https
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
    [switch]$Https,
    [string]$TlsCert = "",
    [string]$TlsKey = "",
    [string]$TlsSans = "",
    [string]$HttpRedirectPort = "",
    [switch]$NoHttps,
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
  -Https                 Serve HTTPS with a self-signed certificate
  -TlsCert <path>        Serve HTTPS with your own certificate (PEM) ...
  -TlsKey <path>         ... and its private key (PEM)
  -TlsSans <list>        Extra comma-separated names/IPs for the certificate
  -HttpRedirectPort <n>  Also redirect plain HTTP on this port to HTTPS
  -NoHttps               Turn HTTPS off and forget saved HTTPS settings
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

# HTTPS settings: CLI overrides saved settings; -NoHttps clears them.
$TlsMode = ""
if ($Https) { $TlsMode = "self-signed" }
if (-not $NoHttps) {
    if (-not $TlsMode -and -not $TlsCert -and -not $TlsKey -and $Config.ContainsKey('tls')) {
        if ($Config['tls'] -in @('self-signed', 'provided')) { $TlsMode = $Config['tls'] }
    }
    if ($TlsMode -ne 'self-signed') {
        if (-not $TlsCert -and $Config.ContainsKey('tls_cert')) { $TlsCert = $Config['tls_cert'] }
        if (-not $TlsKey -and $Config.ContainsKey('tls_key')) { $TlsKey = $Config['tls_key'] }
    }
    if (-not $TlsSans -and $Config.ContainsKey('tls_sans')) { $TlsSans = $Config['tls_sans'] }
    if (-not $HttpRedirectPort -and $Config.ContainsKey('http_redirect_port')) { $HttpRedirectPort = $Config['http_redirect_port'] }
} else {
    $TlsMode = ""; $TlsCert = ""; $TlsKey = ""; $TlsSans = ""; $HttpRedirectPort = ""
}
if ($TlsCert -or $TlsKey) {
    if ($TlsMode -eq 'self-signed') {
        Write-Error "-Https (self-signed) cannot be combined with -TlsCert/-TlsKey."
        return
    }
    if (-not $TlsCert -or -not $TlsKey) {
        Write-Error "-TlsCert and -TlsKey must be given together."
        return
    }
    foreach ($f in @($TlsCert, $TlsKey)) {
        if (-not (Test-Path -PathType Leaf $f)) {
            Write-Error "Certificate file not found: $f"
            return
        }
    }
    $TlsCert = (Resolve-Path $TlsCert).Path
    $TlsKey = (Resolve-Path $TlsKey).Path
    $TlsMode = "provided"
} elseif ($TlsMode -eq 'provided') {
    Write-Error "Saved HTTPS settings name no certificate; use -Https, -TlsCert/-TlsKey or -NoHttps."
    return
}
if ($TlsSans -and $TlsSans -notmatch '^[A-Za-z0-9.,:*-]+$') {
    Write-Error "-TlsSans accepts host names and IP addresses separated by commas."
    return
}
if ($HttpRedirectPort) {
    if ($HttpRedirectPort -notmatch '^[0-9]+$') { Write-Error "-HttpRedirectPort must be a number."; return }
    if (-not $TlsMode) { Write-Error "-HttpRedirectPort needs HTTPS (-Https or -TlsCert/-TlsKey)."; return }
    if ($HttpRedirectPort -eq $Port) { Write-Error "-HttpRedirectPort must differ from -Port."; return }
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
    tls                 = $(if ($TlsMode) { $TlsMode } else { 'off' })
    tls_cert            = $TlsCert
    tls_key             = $TlsKey
    tls_sans            = $TlsSans
    http_redirect_port  = $HttpRedirectPort
}
$ConfigData | ConvertTo-Json | Set-Content $ConfigFile -Encoding UTF8

$Image = "${ImageRepo}:${Tag}"

Write-Host "==> Using Docker image: ${Image}"
Write-Host "==> Port: ${Port}"
if ($TlsMode) { Write-Host "==> HTTPS: ${TlsMode}" }

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

# HTTPS. Container and host ports are kept equal when redirecting, so the
# redirect points at the port clients actually use.
$PortFlags = @("-p", "${Port}:8080")
$ExtraVolumes = @()
$Scheme = "http"
if ($TlsMode) {
    $Scheme = "https"
    if ($TlsMode -eq 'self-signed') {
        $EnvFlags += "-e", "FYR_TLS=self-signed"
        # The container cannot see the host's names/addresses; pass them on.
        $sans = @()
        if ($env:COMPUTERNAME -match '^[A-Za-z0-9-]+$') {
            $sans += $env:COMPUTERNAME.ToLower(), "$($env:COMPUTERNAME.ToLower()).local"
        }
        try {
            $sans += Get-NetIPAddress -AddressFamily IPv4 -ErrorAction Stop |
                Where-Object { $_.IPAddress -notmatch '^(127\.|169\.254\.)' } |
                ForEach-Object { $_.IPAddress }
        } catch {
            # Get-NetIPAddress is unavailable (e.g. PowerShell on non-Windows); skip.
        }
        if ($TlsSans) { $sans += $TlsSans -split ',' | Where-Object { $_ } }
        if ($sans.Count -gt 0) {
            $allSans = ($sans | Select-Object -Unique) -join ','
            Write-Host "==> Certificate names: ${allSans}"
            $EnvFlags += "-e", "FYR_TLS_SANS=${allSans}"
        }
    } else {
        Write-Host "==> Using certificate ${TlsCert} and key ${TlsKey}"
        $EnvFlags += "-e", "FYR_TLS_CERT=/run/fyr-tls/cert.pem", "-e", "FYR_TLS_KEY=/run/fyr-tls/key.pem"
        $ExtraVolumes += "-v", "${TlsCert}:/run/fyr-tls/cert.pem:ro", "-v", "${TlsKey}:/run/fyr-tls/key.pem:ro"
    }
    if ($HttpRedirectPort) {
        $PortFlags = @("-p", "${Port}:${Port}", "-p", "${HttpRedirectPort}:${HttpRedirectPort}")
        $EnvFlags += "-e", "FYR_PORT=${Port}", "-e", "FYR_HTTP_REDIRECT_PORT=${HttpRedirectPort}"
    }
}

# ---------------------------------------------------------------------------
# 6. Run the container
# ---------------------------------------------------------------------------
Write-Host "==> Starting container ${ContainerName}..."
$dockerArgs = @(
    "run", "-d",
    "--restart", "unless-stopped"
) + $PortFlags + @(
    "--name", $ContainerName
) + $EnvFlags + @(
    "-v", $VolumeArg
) + $ExtraVolumes + @(
    $Image
)

& docker @dockerArgs

if ($LASTEXITCODE -eq 0) {
    Write-Host "==> Success! Container '${ContainerName}' is up and running."
    Write-Host "==> Access Fyr at ${Scheme}://localhost:${Port} (replace localhost with the server's IP if connecting remotely)."
    if ($TlsMode -eq 'self-signed') {
        Write-Host "==> Browsers will warn about the self-signed certificate once per device."
        Write-Host "==> Compare the fingerprint before accepting it: docker logs ${ContainerName} 2>&1 | Select-String fingerprint"
    }
} else {
    Write-Host "==> ERROR: Failed to start container. Check Docker logs for details."
    return
}