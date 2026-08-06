#!/bin/sh
# Fyr installer — https://fyr.guide/install.sh
# Usage: curl -fsSL https://fyr.guide/install.sh | sh
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- update
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- --data-dir /srv/fyr --port 9090
#        curl -fsSL https://fyr.guide/install.sh | sh -s -- --legacy
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

# ---------------------------------------------------------------------------
# 6. Run the container
# ---------------------------------------------------------------------------
echo "==> Starting container ${CONTAINER_NAME}..."
eval docker run -d \
    --restart unless-stopped \
    -p "${PORT}:8080" \
    --name "$CONTAINER_NAME" \
    ${ENV_FLAGS} \
    ${VOLUME_FLAG} \
    "$IMAGE"

echo "==> Success! Container '${CONTAINER_NAME}' is up and running."
echo "==> Access Fyr at http://localhost:${PORT} (replace localhost with the server's IP if connecting remotely)."
