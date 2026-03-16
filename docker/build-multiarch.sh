#!/bin/bash
# =============================================================================
# ElevoWorkspace Multi-Architecture Docker Build Script
# =============================================================================
#
# Builds two images:
#   - elevosandbox-server  (workspace server)
#   - elevosandbox-base    (sandbox base image with agent)
#
# Usage:
#   # Build and push for current architecture:
#   ./docker/build-multiarch.sh build --registry docker.example.com --tag v0.3.0
#
#   # Build without pushing:
#   ./docker/build-multiarch.sh build --registry docker.example.com --tag v0.3.0 --no-push
#
#   # Create multi-arch manifest (after building on both architectures):
#   ./docker/build-multiarch.sh manifest --registry docker.example.com --tag v0.3.0
#
# Environment variables:
#   REGISTRY              - Docker registry (required, or use --registry)
#   IMAGE_TAG             - Image tag (default: latest)
#   RUST_IMAGE            - Rust builder image for x86 (default: rust:1.92.0)
#   RUST_IMAGE_ARM        - Rust builder image for arm64 (default: rust:1.92.0)
#   CACHE_DIR             - Cache directory for cargo
#   CARGO_REGISTRY_MIRROR - Cargo registry mirror URL (default: USTC mirror)
#
# =============================================================================

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Default values
REGISTRY="${REGISTRY:-}"
SERVER_IMAGE_NAME="${SERVER_IMAGE_NAME:-elevosandbox-server}"
BASE_IMAGE_NAME="${BASE_IMAGE_NAME:-elevosandbox-base}"
IMAGE_TAG="${IMAGE_TAG:-latest}"
RUST_IMAGE_X86="${RUST_IMAGE:-rust:1.92.0}"
RUST_IMAGE_ARM="${RUST_IMAGE_ARM:-rust:1.92.0}"
CACHE_DIR="${CACHE_DIR:-/data/cache/elevosandbox}"
CARGO_REGISTRY_MIRROR="${CARGO_REGISTRY_MIRROR:-sparse+https://mirrors.ustc.edu.cn/crates.io-index/}"

# Auto-detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64)
        ARCH_SUFFIX="amd64"
        RUST_IMAGE="${RUST_IMAGE_X86}"
        ;;
    aarch64|arm64)
        ARCH_SUFFIX="arm64"
        RUST_IMAGE="${RUST_IMAGE_ARM}"
        ;;
    *)
        echo -e "${RED}[ERROR]${NC} Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

# Print helpers
info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; }

print_banner() {
    echo -e "${BLUE}"
    echo "=============================================="
    echo "   ElevoWorkspace Multi-Arch Build Script"
    echo "=============================================="
    echo -e "${NC}"
    echo "Architecture: ${ARCH} (${ARCH_SUFFIX})"
    echo ""
}

usage() {
    echo "Usage: $0 <command> [options]"
    echo ""
    echo "Commands:"
    echo "  build       Build and push architecture-specific images"
    echo "  manifest    Create multi-arch manifest (run after all builds)"
    echo ""
    echo "Options:"
    echo "  --tag TAG                Image tag (default: latest)"
    echo "  --registry URL           Docker registry URL (required)"
    echo "  --server-image NAME      Server image name (default: elevosandbox-server)"
    echo "  --base-image NAME        Base image name (default: elevosandbox-base)"
    echo "  --rust-image IMAGE       Rust builder image (default: rust:1.92.0)"
    echo "  --no-push                Build only, don't push"
    echo ""
    echo "Environment variables:"
    echo "  CACHE_DIR               Cargo cache directory (default: /data/cache/elevosandbox)"
    echo "  CARGO_REGISTRY_MIRROR   Cargo registry mirror URL (default: USTC mirror)"
    echo "  HTTP_PROXY/HTTPS_PROXY  Proxy for network access (optional, auto-forwarded)"
    echo ""
    echo "Examples:"
    echo "  # On x86 machine:"
    echo "  $0 build --registry docker.example.com --tag v0.3.0"
    echo ""
    echo "  # On arm64 machine:"
    echo "  $0 build --registry docker.example.com --tag v0.3.0"
    echo ""
    echo "  # After both builds, create manifest:"
    echo "  $0 manifest --registry docker.example.com --tag v0.3.0"
    echo ""
    echo "  # With custom Rust builder image:"
    echo "  $0 build --registry docker.example.com --tag v0.3.0 --rust-image my-registry/rust:1.92.0"
    echo ""
    exit 0
}

# ============================================================================
# Build Steps
# ============================================================================

setup_cache() {
    info "Setting up cache directories..."
    mkdir -p "${CACHE_DIR}/cargo/git" 2>/dev/null || sudo mkdir -p "${CACHE_DIR}/cargo/git"
    mkdir -p "${CACHE_DIR}/cargo/registry" 2>/dev/null || sudo mkdir -p "${CACHE_DIR}/cargo/registry"
    mkdir -p "${CACHE_DIR}/target" 2>/dev/null || sudo mkdir -p "${CACHE_DIR}/target"
    sudo chown -R "$(id -u):$(id -g)" "${CACHE_DIR}" 2>/dev/null || true
}

build_rust_binaries() {
    info "Building Rust binaries in Docker container..."
    info "Using Rust image: ${RUST_IMAGE}"

    local docker_args=(
        --rm
        -v "${PROJECT_ROOT}":/workspace
        -v "${CACHE_DIR}/cargo/git":/usr/local/cargo/git
        -v "${CACHE_DIR}/cargo/registry":/usr/local/cargo/registry
        -v "${CACHE_DIR}/target":/workspace/target
        -w /workspace
    )

    # Forward proxy settings from host environment if present
    for var in HTTP_PROXY HTTPS_PROXY http_proxy https_proxy NO_PROXY no_proxy; do
        if [ -n "${!var}" ]; then
            docker_args+=(-e "${var}=${!var}")
        fi
    done

    # Forward cargo mirror setting into container
    if [ -n "${CARGO_REGISTRY_MIRROR}" ]; then
        docker_args+=(-e "CARGO_REGISTRY_MIRROR=${CARGO_REGISTRY_MIRROR}")
    fi

    docker run "${docker_args[@]}" \
        "$RUST_IMAGE" \
        bash -c '
            # Configure cargo registry mirror if specified
            if [ -n "${CARGO_REGISTRY_MIRROR}" ]; then
                rm -rf ~/.cargo/config.toml /root/.cargo/config.toml /usr/local/cargo/config.toml 2>/dev/null || true
                mkdir -p ~/.cargo
                cat > ~/.cargo/config.toml << EOF
[source.crates-io]
replace-with = "mirror"

[source.mirror]
registry = "${CARGO_REGISTRY_MIRROR}"

[net]
git-fetch-with-cli = true
EOF
            fi
            # Build both server and agent
            cargo build --release --package workspace-server --package workspace-agent --package workspace-fuse
        '

    if [ $? -ne 0 ]; then
        error "Failed to build Rust binaries"
        exit 1
    fi

    # Verify binaries
    local missing=false
    for bin in workspace-server workspace-agent workspace-fuse; do
        if [ ! -f "${CACHE_DIR}/target/release/${bin}" ]; then
            error "Binary not found: ${CACHE_DIR}/target/release/${bin}"
            missing=true
        fi
    done
    [ "$missing" = true ] && exit 1

    info "Binaries built successfully!"
}

prepare_files() {
    info "Copying binaries to project directory..."
    mkdir -p "${PROJECT_ROOT}/target/release"

    for bin in workspace-server workspace-agent workspace-fuse; do
        rm -f "${PROJECT_ROOT}/target/release/${bin}" 2>/dev/null || sudo rm -f "${PROJECT_ROOT}/target/release/${bin}"
        cp "${CACHE_DIR}/target/release/${bin}" "${PROJECT_ROOT}/target/release/"
        chmod +x "${PROJECT_ROOT}/target/release/${bin}"
        info "  ${bin} ready"
    done

    # Copy workspace-fuse with platform-specific name for Dockerfile.server
    local fuse_platform_name="workspace-fuse-linux-${ARCH_SUFFIX}"
    cp "${PROJECT_ROOT}/target/release/workspace-fuse" "${PROJECT_ROOT}/target/${fuse_platform_name}"
    chmod +x "${PROJECT_ROOT}/target/${fuse_platform_name}"
    info "  ${fuse_platform_name} ready"
}

build_docker_images() {
    local no_push="$1"
    local server_full="${REGISTRY}/${SERVER_IMAGE_NAME}"
    local base_full="${REGISTRY}/${BASE_IMAGE_NAME}"
    local server_tag="${server_full}:${IMAGE_TAG}-${ARCH_SUFFIX}"
    local base_tag="${base_full}:${IMAGE_TAG}-${ARCH_SUFFIX}"

    cd "${PROJECT_ROOT}"

    # Build server image
    info "Building server image: ${server_tag}"
    DOCKER_BUILDKIT=0 docker build \
        -f docker/Dockerfile.server \
        -t "${server_tag}" \
        .

    # Build base image
    info "Building base image: ${base_tag}"
    DOCKER_BUILDKIT=0 docker build \
        -f images/workspace-base/Dockerfile \
        -t "${base_tag}" \
        .

    # Push
    if [ "$no_push" = false ]; then
        info "Pushing ${server_tag}"
        docker push "${server_tag}"
        info "Pushing ${base_tag}"
        docker push "${base_tag}"
    fi

    echo ""
    echo -e "${GREEN}=============================================${NC}"
    echo -e "${GREEN}  Build Complete: ${ARCH_SUFFIX}${NC}"
    echo -e "${GREEN}=============================================${NC}"
    echo ""
    echo "Server: ${server_tag}"
    echo "Base:   ${base_tag}"
    echo ""
    if [ "$no_push" = true ]; then
        echo "To push:"
        echo "  docker push ${server_tag}"
        echo "  docker push ${base_tag}"
    fi
}

# ============================================================================
# Commands
# ============================================================================

cmd_build() {
    local no_push=false

    while [[ $# -gt 0 ]]; do
        case $1 in
            --tag) IMAGE_TAG="$2"; shift 2 ;;
            --registry) REGISTRY="$2"; shift 2 ;;
            --server-image) SERVER_IMAGE_NAME="$2"; shift 2 ;;
            --base-image) BASE_IMAGE_NAME="$2"; shift 2 ;;
            --rust-image) RUST_IMAGE="$2"; shift 2 ;;
            --no-push) no_push=true; shift ;;
            *) error "Unknown option: $1"; exit 1 ;;
        esac
    done

    if [ -z "${REGISTRY}" ]; then
        error "Registry is required. Use --registry <url> or set REGISTRY env var."
        exit 1
    fi

    local server_full="${REGISTRY}/${SERVER_IMAGE_NAME}"
    local base_full="${REGISTRY}/${BASE_IMAGE_NAME}"

    info "Configuration:"
    info "  Server image: ${server_full}:${IMAGE_TAG}-${ARCH_SUFFIX}"
    info "  Base image:   ${base_full}:${IMAGE_TAG}-${ARCH_SUFFIX}"
    info "  Rust image:   ${RUST_IMAGE}"
    info "  Push: $([ "$no_push" = true ] && echo 'No' || echo 'Yes')"
    echo ""

    setup_cache
    build_rust_binaries
    prepare_files
    build_docker_images "$no_push"
}

create_manifest_for_image() {
    local full_image="$1"
    local base_tag="${full_image}:${IMAGE_TAG}"
    local amd64_tag="${full_image}:${IMAGE_TAG}-amd64"
    local arm64_tag="${full_image}:${IMAGE_TAG}-arm64"

    info "Creating manifest: ${base_tag}"
    info "  amd64: ${amd64_tag}"
    info "  arm64: ${arm64_tag}"

    # Check images exist
    local images_found=true
    for tag in "$amd64_tag" "$arm64_tag"; do
        if ! docker manifest inspect "$tag" &>/dev/null && ! docker image inspect "$tag" &>/dev/null; then
            warn "Image not found: ${tag}"
            images_found=false
        fi
    done

    if [ "$images_found" = false ]; then
        warn "Some images not found. Continue anyway? (y/N)"
        read -p "" -n 1 -r
        echo
        [[ ! $REPLY =~ ^[Yy]$ ]] && return 1
    fi

    # Create and push manifest
    docker manifest rm "${base_tag}" 2>/dev/null || true
    docker manifest create "${base_tag}" "${amd64_tag}" "${arm64_tag}"
    docker manifest annotate "${base_tag}" "${amd64_tag}" --arch amd64
    docker manifest annotate "${base_tag}" "${arm64_tag}" --arch arm64
    docker manifest push "${base_tag}"

    # Also create latest manifest
    if [ "${IMAGE_TAG}" != "latest" ]; then
        local latest_tag="${full_image}:latest"
        docker manifest rm "${latest_tag}" 2>/dev/null || true
        docker manifest create "${latest_tag}" "${amd64_tag}" "${arm64_tag}"
        docker manifest annotate "${latest_tag}" "${amd64_tag}" --arch amd64
        docker manifest annotate "${latest_tag}" "${arm64_tag}" --arch arm64
        docker manifest push "${latest_tag}"
    fi

    info "Manifest pushed: ${base_tag}"
}

cmd_manifest() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --tag) IMAGE_TAG="$2"; shift 2 ;;
            --registry) REGISTRY="$2"; shift 2 ;;
            --server-image) SERVER_IMAGE_NAME="$2"; shift 2 ;;
            --base-image) BASE_IMAGE_NAME="$2"; shift 2 ;;
            *) error "Unknown option: $1"; exit 1 ;;
        esac
    done

    if [ -z "${REGISTRY}" ]; then
        error "Registry is required. Use --registry <url> or set REGISTRY env var."
        exit 1
    fi

    create_manifest_for_image "${REGISTRY}/${SERVER_IMAGE_NAME}"
    echo ""
    create_manifest_for_image "${REGISTRY}/${BASE_IMAGE_NAME}"

    echo ""
    echo -e "${GREEN}=============================================${NC}"
    echo -e "${GREEN}  Multi-Arch Manifests Created!${NC}"
    echo -e "${GREEN}=============================================${NC}"
    echo ""
    echo "Pull commands (auto-detect arch):"
    echo "  docker pull ${REGISTRY}/${SERVER_IMAGE_NAME}:${IMAGE_TAG}"
    echo "  docker pull ${REGISTRY}/${BASE_IMAGE_NAME}:${IMAGE_TAG}"
    echo ""
}

# ============================================================================
# Main
# ============================================================================

main() {
    if [ $# -lt 1 ]; then
        usage
    fi

    local command="$1"
    shift

    print_banner

    case "$command" in
        build)      cmd_build "$@" ;;
        manifest)   cmd_manifest "$@" ;;
        -h|--help|help) usage ;;
        *)
            error "Unknown command: ${command}"
            usage
            ;;
    esac
}

main "$@"
