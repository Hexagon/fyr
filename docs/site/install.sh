#!/bin/sh
# Fyr installer — https://fyr.guide/install.sh
# Usage: curl -fsSL https://fyr.guide/install.sh | sh
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- update
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- --data-dir /srv/fyr --port 9090
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- --legacy
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- --https
#
# NOTE: This script is POSIX sh-compatible. It is piped via `curl | sh` which
# bypasses the shebang, so it must work under /bin/sh (dash, bash, busybox sh).
# No bashisms allowed: no [[, no &>, no =~, no == inside [ ].
set -e

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
CONTAINER_NAME="fyr"
IMAGE_REPO="hexagon/fyr"
DEFAULT_TAG="latest"
DEFAULT_PORT="8080"
DEFAULT_DATA_VOLUME="fyr-data"
CONFIG_DIR="${HOME}/.config/fyr"
CONFIG_FILE="${CONFIG_DIR}/install.conf"

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------
show_help() {
    cat <<EOF
Fyr installer — https://fyr.guide/install.sh

Usage:
  curl -fsSL https://fyr.guide/install.sh | sh
  curl -fsSL https://fyr.guide/install.sh | sh -s -- [options] [tag]

Arguments:
    latest|dev|<tag>       Docker image tag (default: latest, optimized)
  update                 Recreate container with latest image (preserves data)

Options:
    --legacy               Use the legacy compatibility tag for this platform
    --pc-legacy            Use the x86_64 legacy compatibility tag
    --rpi-legacy           Use the arm64 Raspberry Pi legacy compatibility tag
  --data-dir <path>      Bind-mount a host directory as the data volume
  --data-volume <name>   Use a named Docker volume (default: fyr-data)
  --port <number>        Host port to expose (default: 8080)
  --admin-password <pw>  Enable admin mode with the given password
  --readonly             Enable strict read-only mode (no mutations)
  --https                Serve HTTPS with a self-signed certificate (enables
                         GPS/My Location in browsers on the LAN)
  --tls-cert <path>      Serve HTTPS with your own certificate chain (PEM);
  --tls-key <path>       use together with --tls-key (host paths)
  --tls-sans <list>      Extra comma-separated names/IPs for the self-signed
                         certificate (host name and IPs are added automatically)
  --http-redirect-port <n>
                         Also listen on this host port with plain HTTP and
                         redirect to HTTPS (e.g. 80)
  --no-https             Turn HTTPS off again and forget saved HTTPS settings
  --help                 Show this help message

Notes:
    latest/dev/version tags now prefer CPU-optimized images.
    Use --legacy on older or mixed hardware that needs broader compatibility.
  --data-dir and --data-volume are mutually exclusive.
  If neither is given, a named Docker volume 'fyr-data' is used.
  Settings are persisted in ~/.config/fyr/install.conf for future updates.
EOF
    exit 0
}

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------
TAG=""
IS_UPDATE=false
DATA_DIR=""
DATA_VOLUME=""
PORT=""
ADMIN_PASSWORD=""
READONLY=false
LEGACY_MODE=""
TLS_MODE=""
TLS_CERT=""
TLS_KEY=""
TLS_SANS=""
REDIRECT_PORT=""
NO_HTTPS=false

# Read the value of an option that requires an argument, or fail.
require_arg() {
    if [ -z "$2" ] || echo "$2" | grep -q '^--'; then
        echo "==> ERROR: $1 requires $3 argument"
        exit 1
    fi
}

# Make a path absolute (docker -v needs absolute host paths).
absolute_path() {
    case "$1" in
        /*) echo "$1" ;;
        *) echo "$(pwd)/$1" ;;
    esac
}

# Host names and LAN addresses clients may use to reach this machine. The
# container cannot see them, so they are passed in FYR_TLS_SANS.
detect_host_sans() {
    sans=""
    h=$(hostname 2>/dev/null || true)
    case "$h" in
        ""|*[!A-Za-z0-9.-]*|-*) ;;
        *.*) sans="$h" ;;
        *) sans="$h,$h.local" ;;
    esac
    ips=$(hostname -I 2>/dev/null || true)
    if [ -z "$ips" ] && command -v ip >/dev/null 2>&1; then
        ips=$(ip -o addr show scope global 2>/dev/null | awk '{print $4}' | cut -d/ -f1 | tr '\n' ' ')
    fi
    if [ -z "$ips" ] && command -v ifconfig >/dev/null 2>&1; then
        ips=$(ifconfig 2>/dev/null | awk '/inet /{print $2}' | sed 's/^addr://' | tr '\n' ' ')
    fi
    for ip in $ips; do
        case "$ip" in
            127.*|169.254.*|*[!0-9A-Fa-f.:]*) ;;
            fc*:*|fd*:*|FC*:*|FD*:*) sans="$sans,$ip" ;;
            *:*) ;;
            *) sans="$sans,$ip" ;;
        esac
    done
    echo "$sans" | sed 's/^,//'
}

resolve_legacy_suffix() {
    mode="$1"
    case "$mode" in
        pc)
            echo "pc-legacy"
            ;;
        rpi)
            echo "rpi-legacy"
            ;;
        *)
            arch=$(uname -m 2>/dev/null || echo "")
            case "$arch" in
                x86_64|amd64)
                    echo "pc-legacy"
                    ;;
                aarch64|arm64)
                    echo "rpi-legacy"
                    ;;
                *)
                    echo "legacy"
                    ;;
            esac
            ;;
    esac
}

apply_legacy_tag() {
    current_tag="$1"
    mode="$2"
    suffix=$(resolve_legacy_suffix "$mode")

    case "$current_tag" in
        ""|latest)
            echo "$suffix"
            ;;
        dev)
            echo "dev-$suffix"
            ;;
        amd64-avx2)
            echo "pc-legacy"
            ;;
        arm64-dotprod)
            echo "rpi-legacy"
            ;;
        dev-amd64-avx2)
            echo "dev-pc-legacy"
            ;;
        dev-arm64-dotprod)
            echo "dev-rpi-legacy"
            ;;
        *-legacy|*pc-legacy|*rpi-legacy)
            echo "$current_tag"
            ;;
        v*)
            echo "${current_tag}-$suffix"
            ;;
        *)
            echo "$current_tag"
            ;;
    esac
}

while [ $# -gt 0 ]; do
    case "$1" in
        --help|-h)
            show_help
            ;;
        --legacy)
            LEGACY_MODE="auto"
            shift
            ;;
        --pc-legacy)
            LEGACY_MODE="pc"
            shift
            ;;
        --rpi-legacy)
            LEGACY_MODE="rpi"
            shift
            ;;
        update)
            IS_UPDATE=true
            shift
            ;;
        --data-dir)
            if [ -z "$2" ] || echo "$2" | grep -q '^--'; then
                echo "==> ERROR: --data-dir requires a path argument"
                exit 1
            fi
            DATA_DIR="$2"
            shift 2
            ;;
        --data-volume)
            if [ -z "$2" ] || echo "$2" | grep -q '^--'; then
                echo "==> ERROR: --data-volume requires a volume name argument"
                exit 1
            fi
            DATA_VOLUME="$2"
            shift 2
            ;;
        --port)
            if [ -z "$2" ] || echo "$2" | grep -q '^--'; then
                echo "==> ERROR: --port requires a number argument"
                exit 1
            fi
            PORT="$2"
            shift 2
            ;;
        --admin-password)
            if [ -z "$2" ] || echo "$2" | grep -q '^--'; then
                echo "==> ERROR: --admin-password requires a password argument"
                exit 1
            fi
            ADMIN_PASSWORD="$2"
            shift 2
            ;;
        --readonly)
            READONLY=true
            shift
            ;;
        --https)
            TLS_MODE="self-signed"
            shift
            ;;
        --tls-cert)
            require_arg "$1" "$2" "a path"
            TLS_CERT="$2"
            shift 2
            ;;
        --tls-key)
            require_arg "$1" "$2" "a path"
            TLS_KEY="$2"
            shift 2
            ;;
        --tls-sans)
            require_arg "$1" "$2" "a comma-separated list"
            TLS_SANS="$2"
            shift 2
            ;;
        --http-redirect-port)
            require_arg "$1" "$2" "a number"
            REDIRECT_PORT="$2"
            shift 2
            ;;
        --no-https)
            NO_HTTPS=true
            shift
            ;;
        *)
            # Treat as tag (any string accepted)
            TAG="$1"
            shift
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Config file management
# ---------------------------------------------------------------------------
# Read existing config if present
if [ -f "$CONFIG_FILE" ]; then
    while IFS='=' read -r key value; do
        # Skip comments and blank lines
        case "$key" in
            \#*) continue ;;
            "") continue ;;
        esac
        case "$key" in
            data_dir)
                [ -z "$DATA_DIR" ] && DATA_DIR="$value"
                ;;
            data_volume)
                [ -z "$DATA_VOLUME" ] && DATA_VOLUME="$value"
                ;;
            port)
                [ -z "$PORT" ] && PORT="$value"
                ;;
            tag)
                [ -z "$TAG" ] && TAG="$value"
                ;;
            admin_password_set)
                if [ "$value" = "true" ] && [ -z "$ADMIN_PASSWORD" ]; then
                    ADMIN_PASSWORD="<saved>"
                fi
                ;;
            readonly)
                if [ "$value" = "true" ] && [ "$READONLY" = false ]; then
                    READONLY=true
                fi
                ;;
            tls)
                if [ -z "$TLS_MODE" ] && [ -z "$TLS_CERT" ]; then
                    case "$value" in
                        self-signed) TLS_MODE="self-signed" ;;
                        provided) TLS_MODE="provided" ;;
                    esac
                fi
                ;;
            tls_cert)
                [ -z "$TLS_CERT" ] && [ "$TLS_MODE" != "self-signed" ] && TLS_CERT="$value"
                ;;
            tls_key)
                [ -z "$TLS_KEY" ] && [ "$TLS_MODE" != "self-signed" ] && TLS_KEY="$value"
                ;;
            tls_sans)
                [ -z "$TLS_SANS" ] && TLS_SANS="$value"
                ;;
            http_redirect_port)
                [ -z "$REDIRECT_PORT" ] && REDIRECT_PORT="$value"
                ;;
        esac
    done < "$CONFIG_FILE"
fi

# Apply defaults for anything still unset
[ -z "$TAG" ]  && TAG="$DEFAULT_TAG"
[ -z "$PORT" ] && PORT="$DEFAULT_PORT"
if [ -z "$DATA_VOLUME" ] && [ -z "$DATA_DIR" ]; then
    DATA_VOLUME="$DEFAULT_DATA_VOLUME"
fi

if [ -n "$LEGACY_MODE" ]; then
    TAG=$(apply_legacy_tag "$TAG" "$LEGACY_MODE")
fi

# Validate mutual exclusivity
if [ -n "$DATA_DIR" ] && [ -n "$DATA_VOLUME" ]; then
    echo "==> ERROR: --data-dir and --data-volume are mutually exclusive."
    echo "==> Use one or the other, not both."
    exit 1
fi

# HTTPS settings
if [ "$NO_HTTPS" = true ]; then
    TLS_MODE=""
    TLS_CERT=""
    TLS_KEY=""
    TLS_SANS=""
    REDIRECT_PORT=""
fi
if [ -n "$TLS_CERT" ] || [ -n "$TLS_KEY" ]; then
    if [ "$TLS_MODE" = "self-signed" ]; then
        echo "==> ERROR: --https (self-signed) cannot be combined with --tls-cert/--tls-key."
        exit 1
    fi
    if [ -z "$TLS_CERT" ] || [ -z "$TLS_KEY" ]; then
        echo "==> ERROR: --tls-cert and --tls-key must be given together."
        exit 1
    fi
    TLS_CERT=$(absolute_path "$TLS_CERT")
    TLS_KEY=$(absolute_path "$TLS_KEY")
    for f in "$TLS_CERT" "$TLS_KEY"; do
        case "$f" in
            *[\"\$\`\\]*|*:*)
                echo "==> ERROR: Unsupported characters in certificate path: $f"
                exit 1
                ;;
        esac
        if [ ! -f "$f" ]; then
            echo "==> ERROR: Certificate file not found: $f"
            exit 1
        fi
    done
    TLS_MODE="provided"
elif [ "$TLS_MODE" = "provided" ]; then
    echo "==> ERROR: Saved HTTPS settings name no certificate; use --https, --tls-cert/--tls-key or --no-https."
    exit 1
fi
case "$TLS_SANS" in
    *[!A-Za-z0-9.,:*-]*)
        echo "==> ERROR: --tls-sans accepts host names and IP addresses separated by commas."
        exit 1
        ;;
esac
if [ -n "$REDIRECT_PORT" ]; then
    case "$REDIRECT_PORT" in
        *[!0-9]*)
            echo "==> ERROR: --http-redirect-port must be a number."
            exit 1
            ;;
    esac
    if [ -z "$TLS_MODE" ]; then
        echo "==> ERROR: --http-redirect-port needs HTTPS (--https or --tls-cert/--tls-key)."
        exit 1
    fi
    if [ "$REDIRECT_PORT" = "$PORT" ]; then
        echo "==> ERROR: --http-redirect-port must differ from --port."
        exit 1
    fi
fi

# Write config for next run
mkdir -p "$CONFIG_DIR"
cat > "$CONFIG_FILE" <<CONFEOF
# Fyr installer config — auto-generated, edit to persist settings
data_dir=${DATA_DIR}
data_volume=${DATA_VOLUME}
port=${PORT}
tag=${TAG}
admin_password_set=$( [ -n "$ADMIN_PASSWORD" ] && echo "true" || echo "false" )
readonly=$( [ "$READONLY" = true ] && echo "true" || echo "false" )
tls=${TLS_MODE:-off}
tls_cert=${TLS_CERT}
tls_key=${TLS_KEY}
tls_sans=${TLS_SANS}
http_redirect_port=${REDIRECT_PORT}
CONFEOF

# ---------------------------------------------------------------------------
# 1. Check if Docker is installed
# ---------------------------------------------------------------------------
if ! command -v docker >/dev/null 2>&1; then
    echo "==> ERROR: Docker is required but not installed."
    echo "==> Please install Docker first: https://docs.docker.com/engine/install/"
    echo "==> Or run the official convenience script: curl -fsSL https://get.docker.com | sh"
    exit 1
fi

# Check if Docker daemon is accessible (may need sudo)
if ! docker ps >/dev/null 2>&1; then
    # Check if the error is because the user isn't in the docker group
    if groups "$(id -un)" 2>/dev/null | grep -qv '\bdocker\b'; then
        echo "==> ERROR: The current user '$(id -un)' is not in the 'docker' group."
        echo "==> To fix this, run:"
        echo "==>   sudo usermod -aG docker $(id -un)"
        echo "==> Then log out and back in, and re-run this installer."
        exit 1
    fi
    echo "==> Docker requires elevated privileges."
    echo "==> Re-run the script with sudo:"
    echo "==>   curl -fsSL https://fyr.guide/install.sh | sudo sh"
    exit 1
fi

IMAGE="${IMAGE_REPO}:${TAG}"

echo "==> Using Docker image: ${IMAGE}"
echo "==> Port: ${PORT}"
if [ -n "$TLS_MODE" ]; then
    echo "==> HTTPS: ${TLS_MODE}"
fi

# ---------------------------------------------------------------------------
# 2. Prepare data storage
# ---------------------------------------------------------------------------
VOLUME_FLAG=""
if [ -n "$DATA_DIR" ]; then
    # Bind mount mode
    echo "==> Using host directory: ${DATA_DIR}"
    if [ ! -d "$DATA_DIR" ]; then
        echo "==> Creating data directory at ${DATA_DIR}..."
        mkdir -p "$DATA_DIR"
        # Only chown if we just created it and are running as root
        if [ "$(id -u)" -eq 0 ]; then
            echo "==> Setting permissions on ${DATA_DIR} to UID 1000..."
            chown -R 1000:1000 "$DATA_DIR"
        else
            echo "==> WARNING: Not running as root — skipping permission fix on ${DATA_DIR}."
            echo "==> If Fyr encounters permission errors, run: sudo chown -R 1000:1000 ${DATA_DIR}"
        fi
    fi
    VOLUME_FLAG="-v \"${DATA_DIR}:/data\""
elif [ -n "$DATA_VOLUME" ]; then
    # Named volume mode
    echo "==> Using Docker volume: ${DATA_VOLUME}"
    if ! docker volume inspect "$DATA_VOLUME" >/dev/null 2>&1; then
        echo "==> Creating Docker volume ${DATA_VOLUME}..."
        docker volume create "$DATA_VOLUME" >/dev/null
    fi
    VOLUME_FLAG="-v \"${DATA_VOLUME}:/data\""
fi

# ---------------------------------------------------------------------------
# 3. Check if container already exists
# ---------------------------------------------------------------------------
EXISTING=$(docker ps -a -q -f name="^/${CONTAINER_NAME}$" 2>/dev/null)
if [ -n "$EXISTING" ]; then
    if [ "$IS_UPDATE" = true ]; then
        echo "==> Update flag detected. Stopping and removing old container..."
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
    else
        echo "==> WARNING: Container '${CONTAINER_NAME}' is already installed!"
        echo "==> If you want to update/recreate it, run this script with the 'update' argument:"
        echo "==>   curl -fsSL https://fyr.guide/install.sh | sh -s -- update"
        exit 1
    fi
fi

# ---------------------------------------------------------------------------
# 4. Pre-pull the image
# ---------------------------------------------------------------------------
echo "==> Pre-pulling image ${IMAGE}..."
docker pull "$IMAGE"

# ---------------------------------------------------------------------------
# 5. Build environment variables
# ---------------------------------------------------------------------------
ENV_FLAGS="-e FYR_HOST=0.0.0.0 -e DATA_DIR=/data"
if [ -n "$ADMIN_PASSWORD" ] && [ "$ADMIN_PASSWORD" != "<saved>" ]; then
    ENV_FLAGS="${ENV_FLAGS} -e FYR_ADMIN_PASSWORD=${ADMIN_PASSWORD}"
fi
if [ "$READONLY" = true ]; then
    ENV_FLAGS="${ENV_FLAGS} -e FYR_READONLY=true"
fi

# HTTPS. Container and host ports are kept equal when redirecting, so the
# redirect points at the port clients actually use.
PORT_FLAGS="-p \"${PORT}:8080\""
SCHEME="http"
if [ -n "$TLS_MODE" ]; then
    SCHEME="https"
    if [ "$TLS_MODE" = "self-signed" ]; then
        ENV_FLAGS="${ENV_FLAGS} -e FYR_TLS=self-signed"
        ALL_SANS=$(detect_host_sans)
        if [ -n "$TLS_SANS" ]; then
            ALL_SANS="${ALL_SANS:+${ALL_SANS},}${TLS_SANS}"
        fi
        if [ -n "$ALL_SANS" ]; then
            echo "==> Certificate names: ${ALL_SANS}"
            ENV_FLAGS="${ENV_FLAGS} -e \"FYR_TLS_SANS=${ALL_SANS}\""
        fi
    else
        echo "==> Using certificate ${TLS_CERT} and key ${TLS_KEY}"
        echo "==> The container runs as UID 1000: it must be able to read both files."
        ENV_FLAGS="${ENV_FLAGS} -e FYR_TLS_CERT=/run/fyr-tls/cert.pem -e FYR_TLS_KEY=/run/fyr-tls/key.pem"
        VOLUME_FLAG="${VOLUME_FLAG} -v \"${TLS_CERT}:/run/fyr-tls/cert.pem:ro\" -v \"${TLS_KEY}:/run/fyr-tls/key.pem:ro\""
    fi
    if [ -n "$REDIRECT_PORT" ]; then
        PORT_FLAGS="-p \"${PORT}:${PORT}\" -p \"${REDIRECT_PORT}:${REDIRECT_PORT}\""
        ENV_FLAGS="${ENV_FLAGS} -e FYR_PORT=${PORT} -e FYR_HTTP_REDIRECT_PORT=${REDIRECT_PORT}"
    fi
fi

# ---------------------------------------------------------------------------
# 6. Run the container
# ---------------------------------------------------------------------------
echo "==> Starting container ${CONTAINER_NAME}..."
eval docker run -d \
    --restart unless-stopped \
    ${PORT_FLAGS} \
    --name "$CONTAINER_NAME" \
    ${ENV_FLAGS} \
    ${VOLUME_FLAG} \
    "$IMAGE"

echo "==> Success! Container '${CONTAINER_NAME}' is up and running."
echo "==> Access Fyr at ${SCHEME}://localhost:${PORT} (replace localhost with the server's IP if connecting remotely)."
if [ "$TLS_MODE" = "self-signed" ]; then
    echo "==> Browsers will warn about the self-signed certificate once per device."
    echo "==> Compare the fingerprint before accepting it: docker logs ${CONTAINER_NAME} 2>&1 | grep fingerprint"
fi
