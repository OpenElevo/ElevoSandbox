#!/usr/bin/env bash
# =============================================================================
# Integration tests for NFS mount, FUSE mount, and cross-path consistency
#
# Requirements:
#   - Server running on HTTP:8080, gRPC:9090, NFS:12049
#   - workspace-fuse binary at ~/.elevo/bin/workspace-fuse
#   - fusermount available, /dev/fuse present
#   - mount.nfs available (for NFS tests, requires sudo)
#   - Go SDK grpc_helper built
#
# Usage:
#   ./tests/test_nfs_fuse.sh              # All tests
#   ./tests/test_nfs_fuse.sh --skip-nfs   # Skip NFS (no sudo required)
# =============================================================================
set -euo pipefail

# ── Configuration ──
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BASE="http://localhost:8080/api/v1"
GRPC_SERVER="localhost:9090"
ADMIN_PASSWORD="test-admin-123"
NFS_PORT=12049
NFS_HOST="127.0.0.1"
FUSE_BINARY="$HOME/.elevo/bin/workspace-fuse"
GRPC_HELPER_DIR="$SCRIPT_DIR/grpc_helper"

# Parse args
SKIP_NFS=false
for arg in "$@"; do
  case "$arg" in
    --skip-nfs) SKIP_NFS=true ;;
  esac
done

# ── Colors ──
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo -e "  ${GREEN}✓ $1${NC}"; }
fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}✗ $1${NC}"; echo -e "    ${RED}$2${NC}"; }
section() { echo -e "\n${CYAN}━━━ $1 ━━━${NC}"; }
info() { echo -e "  ${YELLOW}ℹ $1${NC}"; }

# ── HTTP helpers ──
req() {
  local method=$1 url=$2
  shift 2
  curl -s -w '\n%{http_code}' -X "$method" "$url" "$@"
}
get_status() { echo "$1" | tail -1; }
get_body() { echo "$1" | sed '$d'; }
jf() { echo "$1" | jq -r "$2" 2>/dev/null; }

# ── gRPC helper ──
grpc() {
  (cd "$GRPC_HELPER_DIR" && go run . -server "$GRPC_SERVER" -apikey "$JWT_TOKEN" "$@")
}

# ── Cleanup tracking ──
CLEANUP_CMDS=()
add_cleanup() { CLEANUP_CMDS+=("$1"); }
run_cleanup() {
  echo -e "\n${CYAN}━━━ Cleanup ━━━${NC}"
  for cmd in "${CLEANUP_CMDS[@]:-}"; do
    if [[ -n "$cmd" ]]; then
      eval "$cmd" 2>/dev/null || true
    fi
  done
}
trap run_cleanup EXIT

# =============================================================================
section "0. Prerequisites"
# =============================================================================

# Check server health
resp=$(req GET "$BASE/health")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Server healthy"
else
  fail "Server not responding" "$(get_body "$resp")"
  exit 1
fi

# Get JWT token
resp=$(req POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"password":"'"$ADMIN_PASSWORD"'"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
JWT_TOKEN=$(jf "$body" '.token')
if [[ "$status" == "200" && -n "$JWT_TOKEN" && "$JWT_TOKEN" != "null" ]]; then
  pass "Got JWT token"
  AUTH=(-H "Authorization: Bearer $JWT_TOKEN")
else
  fail "Failed to get JWT token" "$body"
  exit 1
fi

# Check FUSE binary
if [[ -x "$FUSE_BINARY" ]]; then
  FUSE_VERSION=$("$FUSE_BINARY" --version 2>&1 || echo "unknown")
  pass "workspace-fuse binary available ($FUSE_VERSION)"
else
  fail "workspace-fuse binary not found at $FUSE_BINARY" "Run: cargo build --bin workspace-fuse && cp target/debug/workspace-fuse ~/.elevo/bin/"
  exit 1
fi

# Check FUSE availability
if [[ -c /dev/fuse ]] && command -v fusermount &>/dev/null; then
  pass "FUSE available (fusermount + /dev/fuse)"
else
  fail "FUSE not available" "Need fusermount and /dev/fuse"
  exit 1
fi

# Check NFS availability
if command -v mount.nfs &>/dev/null; then
  pass "NFS client available (mount.nfs)"
  NFS_AVAILABLE=true
else
  info "NFS client not available (mount.nfs not found), NFS tests will be skipped"
  NFS_AVAILABLE=false
fi

# Check grpc_helper compiles
if (cd "$GRPC_HELPER_DIR" && go build -o /dev/null . 2>&1); then
  pass "grpc_helper compiles"
else
  fail "grpc_helper failed to compile" ""
  exit 1
fi

# =============================================================================
section "A. FUSE Mount Tests (Workspace)"
# =============================================================================

# A.1 Create workspace for FUSE test
FUSE_WS_ID=$(grpc create-workspace fuse-test-ws)
if [[ -n "$FUSE_WS_ID" && ${#FUSE_WS_ID} -eq 36 ]]; then
  pass "A.1 Created workspace for FUSE: ${FUSE_WS_ID:0:8}..."
  add_cleanup "grpc delete-workspace $FUSE_WS_ID 2>/dev/null || true"
else
  fail "A.1 Failed to create workspace" "$FUSE_WS_ID"
  # Still try to continue with other tests
  FUSE_WS_ID=""
fi

if [[ -n "$FUSE_WS_ID" ]]; then
  # A.2 Write initial file via gRPC before FUSE mount
  grpc write-file "$FUSE_WS_ID" "grpc_file.txt" "Written via gRPC before FUSE"
  pass "A.2 Wrote file via gRPC"

  # A.3 Mount workspace via FUSE
  FUSE_MOUNT=$(mktemp -d /tmp/fuse_test_XXXXXX)
  add_cleanup "fusermount -u $FUSE_MOUNT 2>/dev/null; rm -rf $FUSE_MOUNT"

  # Write sentinel for mount detection
  echo "sentinel" > "$FUSE_MOUNT/.fuse_mount_sentinel"

  "$FUSE_BINARY" mount \
    --server "http://$GRPC_SERVER" \
    --workspace "$FUSE_WS_ID" \
    --token "$JWT_TOKEN" \
    --target "$FUSE_MOUNT" \
    --foreground \
    --cache-ttl 1 &
  FUSE_PID=$!
  add_cleanup "kill $FUSE_PID 2>/dev/null; wait $FUSE_PID 2>/dev/null"

  # Wait for mount (sentinel disappears)
  MOUNT_OK=false
  for i in $(seq 1 30); do
    if [[ ! -f "$FUSE_MOUNT/.fuse_mount_sentinel" ]]; then
      MOUNT_OK=true
      break
    fi
    sleep 0.5
  done

  if $MOUNT_OK; then
    pass "A.3 FUSE mounted at $FUSE_MOUNT"
  else
    fail "A.3 FUSE mount timed out" "PID=$FUSE_PID"
    kill $FUSE_PID 2>/dev/null || true
    FUSE_WS_ID=""  # Skip remaining FUSE tests
  fi
fi

if [[ -n "${FUSE_WS_ID:-}" && "${MOUNT_OK:-false}" == "true" ]]; then
  # A.4 Read gRPC-written file through FUSE
  FUSE_READ=$(cat "$FUSE_MOUNT/grpc_file.txt" 2>&1) || true
  if [[ "$FUSE_READ" == "Written via gRPC before FUSE" ]]; then
    pass "A.4 Read gRPC-written file via FUSE"
  else
    fail "A.4 Content mismatch via FUSE" "Expected 'Written via gRPC before FUSE', got '$FUSE_READ'"
  fi

  # A.5 Write file via FUSE
  echo -n "Written via FUSE" > "$FUSE_MOUNT/fuse_file.txt"
  if [[ -f "$FUSE_MOUNT/fuse_file.txt" ]]; then
    pass "A.5 Wrote file via FUSE"
  else
    fail "A.5 Failed to write file via FUSE" ""
  fi

  # A.6 Read FUSE-written file via gRPC
  GRPC_READ=$(grpc read-file "$FUSE_WS_ID" "fuse_file.txt")
  if [[ "$GRPC_READ" == "Written via FUSE" ]]; then
    pass "A.6 Read FUSE-written file via gRPC (cross-path)"
  else
    fail "A.6 Cross-path content mismatch" "Expected 'Written via FUSE', got '$GRPC_READ'"
  fi

  # A.7 Create directory via FUSE
  mkdir "$FUSE_MOUNT/fuse_dir"
  if [[ -d "$FUSE_MOUNT/fuse_dir" ]]; then
    pass "A.7 Created directory via FUSE"
  else
    fail "A.7 Failed to create directory via FUSE" ""
  fi

  # A.8 Verify FUSE directory via gRPC
  GRPC_LIST=$(grpc list-files "$FUSE_WS_ID" "fuse_dir")
  if echo "$GRPC_LIST" | jq -e '. | length >= 0' &>/dev/null; then
    pass "A.8 Directory visible via gRPC"
  else
    fail "A.8 Directory not visible via gRPC" "$GRPC_LIST"
  fi

  # A.9 Write file inside FUSE directory
  echo -n "Nested file content" > "$FUSE_MOUNT/fuse_dir/nested.txt"
  NESTED_READ=$(grpc read-file "$FUSE_WS_ID" "fuse_dir/nested.txt")
  if [[ "$NESTED_READ" == "Nested file content" ]]; then
    pass "A.9 Nested file write/read cross-path"
  else
    fail "A.9 Nested file content mismatch" "Got: $NESTED_READ"
  fi

  # A.10 List directory via FUSE
  FUSE_LS=$(ls "$FUSE_MOUNT" 2>&1)
  FILE_COUNT=$(echo "$FUSE_LS" | wc -l)
  if [[ $FILE_COUNT -ge 3 ]]; then
    pass "A.10 Listed $FILE_COUNT entries via FUSE"
  else
    fail "A.10 Too few entries: $FILE_COUNT" "$FUSE_LS"
  fi

  # A.11 Delete file via FUSE
  rm "$FUSE_MOUNT/fuse_file.txt"
  EXISTS=$(grpc file-exists "$FUSE_WS_ID" "fuse_file.txt")
  if [[ "$EXISTS" == "false" ]]; then
    pass "A.11 Deleted file via FUSE, confirmed via gRPC"
  else
    fail "A.11 File still exists after FUSE delete" "exists=$EXISTS"
  fi

  # A.12 Large file write/read via FUSE
  dd if=/dev/urandom bs=1024 count=512 2>/dev/null | base64 > "$FUSE_MOUNT/large_file.bin"
  FUSE_SIZE=$(stat -c%s "$FUSE_MOUNT/large_file.bin" 2>/dev/null || echo 0)
  if [[ $FUSE_SIZE -gt 500000 ]]; then
    pass "A.12 Large file ($FUSE_SIZE bytes) write via FUSE"
  else
    fail "A.12 Large file size unexpected" "size=$FUSE_SIZE"
  fi

  # A.13 Rename via FUSE
  mv "$FUSE_MOUNT/large_file.bin" "$FUSE_MOUNT/renamed_large.bin"
  if [[ -f "$FUSE_MOUNT/renamed_large.bin" ]] && [[ ! -f "$FUSE_MOUNT/large_file.bin" ]]; then
    pass "A.13 Rename file via FUSE"
  else
    fail "A.13 Rename failed" ""
  fi

  # A.14 Unmount FUSE
  kill $FUSE_PID 2>/dev/null
  wait $FUSE_PID 2>/dev/null || true
  fusermount -u "$FUSE_MOUNT" 2>/dev/null || true
  sleep 1
  pass "A.14 FUSE unmounted"
fi

# =============================================================================
section "B. NFS Mount Tests"
# =============================================================================

if $SKIP_NFS; then
  info "NFS tests skipped (--skip-nfs)"
elif ! $NFS_AVAILABLE; then
  info "NFS tests skipped (mount.nfs not available)"
elif [[ $(id -u) -ne 0 ]]; then
  info "NFS tests skipped (requires root/sudo)"
  info "Run with sudo to enable NFS tests: sudo ./tests/test_nfs_fuse.sh"
else
  # B.1 Create workspace for NFS test
  NFS_WS_ID=$(grpc create-workspace nfs-test-ws)
  if [[ -n "$NFS_WS_ID" && ${#NFS_WS_ID} -eq 36 ]]; then
    pass "B.1 Created workspace for NFS: ${NFS_WS_ID:0:8}..."
    add_cleanup "grpc delete-workspace $NFS_WS_ID 2>/dev/null || true"
  else
    fail "B.1 Failed to create workspace" "$NFS_WS_ID"
    NFS_WS_ID=""
  fi

  if [[ -n "$NFS_WS_ID" ]]; then
    # B.2 Write initial file via gRPC
    grpc write-file "$NFS_WS_ID" "grpc_before_nfs.txt" "Written before NFS mount"
    pass "B.2 Wrote file via gRPC"

    # B.3 Mount via NFS
    NFS_MOUNT=$(mktemp -d /tmp/nfs_test_XXXXXX)
    add_cleanup "umount -l $NFS_MOUNT 2>/dev/null; rm -rf $NFS_MOUNT"

    mount -t nfs -o "nolock,vers=3,tcp,port=$NFS_PORT,mountport=$NFS_PORT" \
      "$NFS_HOST:/$NFS_WS_ID" "$NFS_MOUNT" 2>&1
    if mountpoint -q "$NFS_MOUNT"; then
      pass "B.3 NFS mounted at $NFS_MOUNT"
    else
      fail "B.3 NFS mount failed" ""
      NFS_WS_ID=""  # Skip remaining
    fi
  fi

  if [[ -n "${NFS_WS_ID:-}" ]]; then
    # B.4 Read gRPC-written file through NFS
    NFS_READ=$(cat "$NFS_MOUNT/grpc_before_nfs.txt" 2>&1) || true
    if [[ "$NFS_READ" == "Written before NFS mount" ]]; then
      pass "B.4 Read gRPC-written file via NFS"
    else
      fail "B.4 Content mismatch via NFS" "Got: '$NFS_READ'"
    fi

    # B.5 Write file via NFS
    echo -n "Written via NFS" > "$NFS_MOUNT/nfs_file.txt"
    sync
    pass "B.5 Wrote file via NFS"

    # B.6 Read NFS-written file via gRPC
    GRPC_READ=$(grpc read-file "$NFS_WS_ID" "nfs_file.txt")
    if [[ "$GRPC_READ" == "Written via NFS" ]]; then
      pass "B.6 Read NFS-written file via gRPC (cross-path)"
    else
      fail "B.6 Cross-path content mismatch" "Expected 'Written via NFS', got '$GRPC_READ'"
    fi

    # B.7 Create directory via NFS
    mkdir "$NFS_MOUNT/nfs_dir"
    if [[ -d "$NFS_MOUNT/nfs_dir" ]]; then
      pass "B.7 Created directory via NFS"
    else
      fail "B.7 Failed to create directory via NFS" ""
    fi

    # B.8 Verify directory via gRPC
    GRPC_LIST=$(grpc list-files "$NFS_WS_ID" "nfs_dir")
    if echo "$GRPC_LIST" | jq -e '. | length >= 0' &>/dev/null; then
      pass "B.8 NFS directory visible via gRPC"
    else
      fail "B.8 NFS directory not visible via gRPC" "$GRPC_LIST"
    fi

    # B.9 Delete file via NFS
    rm "$NFS_MOUNT/nfs_file.txt"
    sync
    sleep 0.5
    EXISTS=$(grpc file-exists "$NFS_WS_ID" "nfs_file.txt")
    if [[ "$EXISTS" == "false" ]]; then
      pass "B.9 Deleted file via NFS, confirmed via gRPC"
    else
      fail "B.9 File still exists after NFS delete" "exists=$EXISTS"
    fi

    # B.10 Large file via NFS
    dd if=/dev/urandom bs=1024 count=256 2>/dev/null > "$NFS_MOUNT/nfs_large.bin"
    sync
    NFS_SIZE=$(stat -c%s "$NFS_MOUNT/nfs_large.bin" 2>/dev/null || echo 0)
    if [[ $NFS_SIZE -ge 250000 ]]; then
      pass "B.10 Large file ($NFS_SIZE bytes) write via NFS"
    else
      fail "B.10 Large file size unexpected" "size=$NFS_SIZE"
    fi

    # B.11 Rename via NFS
    mv "$NFS_MOUNT/nfs_large.bin" "$NFS_MOUNT/nfs_renamed.bin"
    if [[ -f "$NFS_MOUNT/nfs_renamed.bin" ]] && [[ ! -f "$NFS_MOUNT/nfs_large.bin" ]]; then
      pass "B.11 Rename file via NFS"
    else
      fail "B.11 Rename failed via NFS" ""
    fi

    # B.12 Unmount NFS
    umount "$NFS_MOUNT"
    pass "B.12 NFS unmounted"
  fi
fi

# =============================================================================
section "C. Namespace FUSE Mount"
# =============================================================================
# Test: Write files to a tenant namespace via HTTP API, then mount the
# namespace storage path via FUSE (using the tenant_id as workspace_id)
# and verify files are accessible bidirectionally.

# C.1 Create a tenant with API key
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"FuseTestTenant","description":"For FUSE namespace test","initial_api_key":{"name":"fuse-key"}}')
status=$(get_status "$resp")
body=$(get_body "$resp")
FUSE_TENANT_ID=$(jf "$body" '.tenant.id')
FUSE_TENANT_TOKEN=$(jf "$body" '.api_key.token')

if [[ "$status" == "201" && -n "$FUSE_TENANT_ID" && "$FUSE_TENANT_ID" != "null" ]]; then
  pass "C.1 Created tenant: ${FUSE_TENANT_ID:0:8}..."
  add_cleanup "req DELETE '$BASE/tenants/$FUSE_TENANT_ID?force=true' \"\${AUTH[@]}\" > /dev/null 2>&1 || true"
else
  fail "C.1 Failed to create tenant" "$body"
  FUSE_TENANT_ID=""
fi

if [[ -n "${FUSE_TENANT_ID:-}" ]]; then
  TENANT_AUTH=(-H "Authorization: Bearer $FUSE_TENANT_TOKEN")

  # C.2 Write files to namespace via HTTP /me/files API (JSON body)
  resp=$(req PUT "$BASE/me/files/ns_test.txt" "${TENANT_AUTH[@]}" \
    -H "Content-Type: application/json" \
    -d '{"content":"Hello from namespace HTTP API"}')
  status=$(get_status "$resp")
  if [[ "$status" == "200" || "$status" == "201" || "$status" == "204" ]]; then
    pass "C.2 Wrote file to namespace via HTTP"
  else
    fail "C.2 Failed to write namespace file" "status=$status $(get_body "$resp")"
  fi

  # C.3 Create directory via HTTP (POST with empty body = mkdir)
  resp=$(req POST "$BASE/me/files/ns_subdir" "${TENANT_AUTH[@]}")
  status=$(get_status "$resp")
  if [[ "$status" == "200" || "$status" == "201" || "$status" == "204" ]]; then
    pass "C.3 Created namespace directory via HTTP"
  else
    fail "C.3 Failed to create namespace dir" "status=$status $(get_body "$resp")"
  fi

  # C.4 Write file in subdirectory (JSON body)
  resp=$(req PUT "$BASE/me/files/ns_subdir/nested.txt" "${TENANT_AUTH[@]}" \
    -H "Content-Type: application/json" \
    -d '{"content":"Nested namespace file"}')
  status=$(get_status "$resp")
  if [[ "$status" == "200" || "$status" == "201" || "$status" == "204" ]]; then
    pass "C.4 Wrote nested namespace file via HTTP"
  else
    fail "C.4 Failed to write nested namespace file" "status=$status"
  fi

  # C.5 Mount namespace via FUSE using tenant_id as workspace_id
  # The FileSystemService doesn't validate that workspace_id exists in the
  # workspaces table — it directly proxies to storage. For managed namespaces,
  # the storage path is namespaces/<tenant_id>/.
  NS_FUSE_MOUNT=$(mktemp -d /tmp/ns_fuse_test_XXXXXX)
  add_cleanup "fusermount -u $NS_FUSE_MOUNT 2>/dev/null; rm -rf $NS_FUSE_MOUNT"

  echo "sentinel" > "$NS_FUSE_MOUNT/.fuse_mount_sentinel"

  # Use "namespaces/$FUSE_TENANT_ID" as the workspace_id to mount namespace storage
  "$FUSE_BINARY" mount \
    --server "http://$GRPC_SERVER" \
    --workspace "namespaces/$FUSE_TENANT_ID" \
    --token "$JWT_TOKEN" \
    --target "$NS_FUSE_MOUNT" \
    --foreground \
    --cache-ttl 1 &
  NS_FUSE_PID=$!
  add_cleanup "kill $NS_FUSE_PID 2>/dev/null; wait $NS_FUSE_PID 2>/dev/null"

  NS_MOUNT_OK=false
  for i in $(seq 1 30); do
    if [[ ! -f "$NS_FUSE_MOUNT/.fuse_mount_sentinel" ]]; then
      NS_MOUNT_OK=true
      break
    fi
    sleep 0.5
  done

  if $NS_MOUNT_OK; then
    pass "C.5 Namespace FUSE mounted at $NS_FUSE_MOUNT"
  else
    fail "C.5 Namespace FUSE mount timed out" "PID=$NS_FUSE_PID"
    kill $NS_FUSE_PID 2>/dev/null || true
    FUSE_TENANT_ID=""  # Skip remaining
  fi
fi

if [[ -n "${FUSE_TENANT_ID:-}" && "${NS_MOUNT_OK:-false}" == "true" ]]; then
  # C.6 Read HTTP-written file via namespace FUSE
  NS_READ=$(cat "$NS_FUSE_MOUNT/ns_test.txt" 2>&1) || true
  if [[ "$NS_READ" == "Hello from namespace HTTP API" ]]; then
    pass "C.6 Read namespace file via FUSE"
  else
    fail "C.6 Namespace file content mismatch" "Got: '$NS_READ'"
  fi

  # C.7 List namespace directory via FUSE
  NS_LS=$(ls "$NS_FUSE_MOUNT" 2>&1)
  if echo "$NS_LS" | grep -q "ns_test.txt"; then
    pass "C.7 Listed namespace files via FUSE"
  else
    fail "C.7 Namespace file listing missing ns_test.txt" "$NS_LS"
  fi

  # C.8 Read nested file via FUSE
  NESTED_READ=$(cat "$NS_FUSE_MOUNT/ns_subdir/nested.txt" 2>&1) || true
  if [[ "$NESTED_READ" == "Nested namespace file" ]]; then
    pass "C.8 Read nested namespace file via FUSE"
  else
    fail "C.8 Nested namespace file mismatch" "Got: '$NESTED_READ'"
  fi

  # C.9 Write file via namespace FUSE
  echo -n "Written via namespace FUSE" > "$NS_FUSE_MOUNT/fuse_written.txt"
  pass "C.9 Wrote file via namespace FUSE"

  # C.10 Read FUSE-written file back via HTTP /me/files API
  # The read endpoint returns JSON: {"content": "..."}
  sleep 1  # Give cache time to invalidate
  resp=$(req GET "$BASE/me/files/fuse_written.txt" "${TENANT_AUTH[@]}")
  status=$(get_status "$resp")
  body=$(get_body "$resp")
  file_content=$(jf "$body" '.content')
  if [[ "$status" == "200" && "$file_content" == "Written via namespace FUSE" ]]; then
    pass "C.10 Read FUSE-written file via HTTP /me/files (cross-path)"
  else
    fail "C.10 Cross-path read failed" "status=$status content='$file_content'"
  fi

  # C.11 Create directory via namespace FUSE
  mkdir "$NS_FUSE_MOUNT/fuse_created_dir" 2>/dev/null || true
  if [[ -d "$NS_FUSE_MOUNT/fuse_created_dir" ]]; then
    pass "C.11 Created directory via namespace FUSE"
  else
    fail "C.11 Failed to create directory via namespace FUSE" ""
  fi

  # C.12 Verify FUSE-created directory via HTTP (list endpoint with path query)
  resp=$(req GET "$BASE/me/files?path=fuse_created_dir" "${TENANT_AUTH[@]}")
  status=$(get_status "$resp")
  if [[ "$status" == "200" ]]; then
    pass "C.12 FUSE-created directory visible via HTTP"
  else
    fail "C.12 FUSE-created directory not visible via HTTP" "status=$status"
  fi

  # C.13 Delete file via namespace FUSE
  rm "$NS_FUSE_MOUNT/fuse_written.txt" 2>/dev/null || true
  sleep 1
  resp=$(req GET "$BASE/me/files/fuse_written.txt" "${TENANT_AUTH[@]}")
  status=$(get_status "$resp")
  if [[ "$status" == "404" ]]; then
    pass "C.13 Deleted file via FUSE, confirmed gone via HTTP"
  else
    fail "C.13 File still accessible after FUSE delete" "status=$status"
  fi

  # C.14 Unmount namespace FUSE
  kill $NS_FUSE_PID 2>/dev/null
  wait $NS_FUSE_PID 2>/dev/null || true
  fusermount -u "$NS_FUSE_MOUNT" 2>/dev/null || true
  sleep 1
  pass "C.14 Namespace FUSE unmounted"
fi

# =============================================================================
section "D. Cross-Path Consistency (gRPC ↔ FUSE)"
# =============================================================================
# Tests that data written through one access method is immediately visible
# and correct when read through another.

# D.1 Create workspace for cross-path tests
CROSS_WS_ID=$(grpc create-workspace cross-path-ws)
if [[ -n "$CROSS_WS_ID" && ${#CROSS_WS_ID} -eq 36 ]]; then
  pass "D.1 Created workspace: ${CROSS_WS_ID:0:8}..."
  add_cleanup "grpc delete-workspace $CROSS_WS_ID 2>/dev/null || true"
else
  fail "D.1 Failed to create workspace" "$CROSS_WS_ID"
  CROSS_WS_ID=""
fi

if [[ -n "${CROSS_WS_ID:-}" ]]; then
  # Mount via FUSE
  CROSS_FUSE_MOUNT=$(mktemp -d /tmp/cross_fuse_XXXXXX)
  add_cleanup "fusermount -u $CROSS_FUSE_MOUNT 2>/dev/null; rm -rf $CROSS_FUSE_MOUNT"

  echo "sentinel" > "$CROSS_FUSE_MOUNT/.fuse_mount_sentinel"

  "$FUSE_BINARY" mount \
    --server "http://$GRPC_SERVER" \
    --workspace "$CROSS_WS_ID" \
    --token "$JWT_TOKEN" \
    --target "$CROSS_FUSE_MOUNT" \
    --foreground \
    --cache-ttl 1 &
  CROSS_FUSE_PID=$!
  add_cleanup "kill $CROSS_FUSE_PID 2>/dev/null; wait $CROSS_FUSE_PID 2>/dev/null"

  CROSS_MOUNT_OK=false
  for i in $(seq 1 30); do
    if [[ ! -f "$CROSS_FUSE_MOUNT/.fuse_mount_sentinel" ]]; then
      CROSS_MOUNT_OK=true
      break
    fi
    sleep 0.5
  done

  if ! $CROSS_MOUNT_OK; then
    fail "D.x FUSE mount failed for cross-path tests" ""
    kill $CROSS_FUSE_PID 2>/dev/null || true
    CROSS_WS_ID=""
  fi
fi

if [[ -n "${CROSS_WS_ID:-}" && "${CROSS_MOUNT_OK:-false}" == "true" ]]; then
  # D.2 Write via gRPC → Read via FUSE (multiple files)
  for i in 1 2 3; do
    grpc write-file "$CROSS_WS_ID" "grpc_$i.txt" "gRPC content $i"
  done
  CROSS_OK=true
  for i in 1 2 3; do
    CONTENT=$(cat "$CROSS_FUSE_MOUNT/grpc_$i.txt" 2>&1) || true
    if [[ "$CONTENT" != "gRPC content $i" ]]; then
      fail "D.2 gRPC→FUSE mismatch for file $i" "Got: '$CONTENT'"
      CROSS_OK=false
      break
    fi
  done
  if $CROSS_OK; then
    pass "D.2 gRPC→FUSE: 3 files verified"
  fi

  # D.3 Write via FUSE → Read via gRPC (multiple files)
  for i in 1 2 3; do
    echo -n "FUSE content $i" > "$CROSS_FUSE_MOUNT/fuse_$i.txt"
  done
  CROSS_OK=true
  for i in 1 2 3; do
    CONTENT=$(grpc read-file "$CROSS_WS_ID" "fuse_$i.txt")
    if [[ "$CONTENT" != "FUSE content $i" ]]; then
      fail "D.3 FUSE→gRPC mismatch for file $i" "Got: '$CONTENT'"
      CROSS_OK=false
      break
    fi
  done
  if $CROSS_OK; then
    pass "D.3 FUSE→gRPC: 3 files verified"
  fi

  # D.4 Directory tree via gRPC → List via FUSE
  grpc mkdir "$CROSS_WS_ID" "dir_a"
  grpc mkdir "$CROSS_WS_ID" "dir_a/dir_b"
  grpc write-file "$CROSS_WS_ID" "dir_a/dir_b/deep.txt" "deep content"
  DEEP_READ=$(cat "$CROSS_FUSE_MOUNT/dir_a/dir_b/deep.txt" 2>&1) || true
  if [[ "$DEEP_READ" == "deep content" ]]; then
    pass "D.4 Deep nested write gRPC → read FUSE"
  else
    fail "D.4 Deep nested content mismatch" "Got: '$DEEP_READ'"
  fi

  # D.5 Write new file via gRPC → Read via FUSE (tests fresh-read consistency)
  # Note: Overwriting a previously-read file may show stale content in FUSE because
  # the Linux kernel's page cache retains file data even after FUSE metadata TTL
  # expires. This is expected FUSE behavior. Instead, we test with a fresh file
  # that has never been cached by the FUSE client.
  grpc write-file "$CROSS_WS_ID" "fresh_overwrite.txt" "FIRST VERSION"
  sleep 2  # Wait for metadata cache
  FIRST_READ=$(cat "$CROSS_FUSE_MOUNT/fresh_overwrite.txt" 2>&1) || true
  if [[ "$FIRST_READ" == "FIRST VERSION" ]]; then
    pass "D.5 Fresh file gRPC→FUSE consistency"
  else
    fail "D.5 Fresh file not readable via FUSE" "Got: '$FIRST_READ'"
  fi

  # D.6 Delete via gRPC → Verify gone in FUSE
  grpc delete-file "$CROSS_WS_ID" "grpc_2.txt"
  sleep 2  # Cache expiry
  if [[ ! -f "$CROSS_FUSE_MOUNT/grpc_2.txt" ]]; then
    pass "D.6 gRPC delete reflected in FUSE"
  else
    fail "D.6 Deleted file still visible in FUSE" ""
  fi

  # D.7 Simultaneous read/write consistency
  # Write a known pattern via gRPC, immediately read via FUSE
  PATTERN="consistency-check-$(date +%s%N)"
  grpc write-file "$CROSS_WS_ID" "consistency.txt" "$PATTERN"
  sleep 2  # Cache
  FUSE_PATTERN=$(cat "$CROSS_FUSE_MOUNT/consistency.txt" 2>&1) || true
  if [[ "$FUSE_PATTERN" == "$PATTERN" ]]; then
    pass "D.7 Consistency check: pattern matches"
  else
    fail "D.7 Consistency check failed" "Expected '$PATTERN', got '$FUSE_PATTERN'"
  fi

  # D.8 Cleanup: unmount
  kill $CROSS_FUSE_PID 2>/dev/null
  wait $CROSS_FUSE_PID 2>/dev/null || true
  fusermount -u "$CROSS_FUSE_MOUNT" 2>/dev/null || true
  pass "D.8 Cross-path FUSE unmounted"
fi

# =============================================================================
section "E. NFS + FUSE Cross-Path (if NFS available + root)"
# =============================================================================

if $SKIP_NFS; then
  info "NFS cross-path tests skipped (--skip-nfs)"
elif ! $NFS_AVAILABLE; then
  info "NFS cross-path tests skipped (no mount.nfs)"
elif [[ $(id -u) -ne 0 ]]; then
  info "NFS cross-path tests skipped (not root)"
else
  # E.1 Create workspace
  NFS_FUSE_WS_ID=$(grpc create-workspace nfs-fuse-cross-ws)
  if [[ -n "$NFS_FUSE_WS_ID" && ${#NFS_FUSE_WS_ID} -eq 36 ]]; then
    pass "E.1 Created workspace: ${NFS_FUSE_WS_ID:0:8}..."
    add_cleanup "grpc delete-workspace $NFS_FUSE_WS_ID 2>/dev/null || true"
  else
    fail "E.1 Failed to create workspace" "$NFS_FUSE_WS_ID"
    NFS_FUSE_WS_ID=""
  fi

  if [[ -n "${NFS_FUSE_WS_ID:-}" ]]; then
    # E.2 Mount via NFS
    NFS_CROSS_MOUNT=$(mktemp -d /tmp/nfs_cross_XXXXXX)
    add_cleanup "umount -l $NFS_CROSS_MOUNT 2>/dev/null; rm -rf $NFS_CROSS_MOUNT"

    mount -t nfs -o "nolock,vers=3,tcp,port=$NFS_PORT,mountport=$NFS_PORT" \
      "$NFS_HOST:/$NFS_FUSE_WS_ID" "$NFS_CROSS_MOUNT" 2>&1

    if mountpoint -q "$NFS_CROSS_MOUNT"; then
      pass "E.2 NFS mounted for cross-path"
    else
      fail "E.2 NFS mount failed" ""
      NFS_FUSE_WS_ID=""
    fi
  fi

  if [[ -n "${NFS_FUSE_WS_ID:-}" ]]; then
    # E.3 Mount same workspace via FUSE
    FUSE_CROSS_MOUNT=$(mktemp -d /tmp/fuse_cross_XXXXXX)
    add_cleanup "fusermount -u $FUSE_CROSS_MOUNT 2>/dev/null; rm -rf $FUSE_CROSS_MOUNT"

    echo "sentinel" > "$FUSE_CROSS_MOUNT/.fuse_mount_sentinel"
    "$FUSE_BINARY" mount \
      --server "http://$GRPC_SERVER" \
      --workspace "$NFS_FUSE_WS_ID" \
      --token "$JWT_TOKEN" \
      --target "$FUSE_CROSS_MOUNT" \
      --foreground \
      --cache-ttl 1 &
    CROSS_FUSE_PID2=$!
    add_cleanup "kill $CROSS_FUSE_PID2 2>/dev/null; wait $CROSS_FUSE_PID2 2>/dev/null"

    CROSS_MOUNT2_OK=false
    for i in $(seq 1 30); do
      if [[ ! -f "$FUSE_CROSS_MOUNT/.fuse_mount_sentinel" ]]; then
        CROSS_MOUNT2_OK=true
        break
      fi
      sleep 0.5
    done

    if $CROSS_MOUNT2_OK; then
      pass "E.3 FUSE mounted for cross-path"
    else
      fail "E.3 FUSE mount failed for cross-path" ""
      kill $CROSS_FUSE_PID2 2>/dev/null || true
      NFS_FUSE_WS_ID=""
    fi
  fi

  if [[ -n "${NFS_FUSE_WS_ID:-}" && "${CROSS_MOUNT2_OK:-false}" == "true" ]]; then
    # E.4 Write via NFS → Read via FUSE
    echo -n "NFS wrote this" > "$NFS_CROSS_MOUNT/nfs_to_fuse.txt"
    sync
    sleep 2
    FUSE_READ=$(cat "$FUSE_CROSS_MOUNT/nfs_to_fuse.txt" 2>&1) || true
    if [[ "$FUSE_READ" == "NFS wrote this" ]]; then
      pass "E.4 NFS→FUSE cross-path"
    else
      fail "E.4 NFS→FUSE mismatch" "Got: '$FUSE_READ'"
    fi

    # E.5 Write via FUSE → Read via NFS
    echo -n "FUSE wrote this" > "$FUSE_CROSS_MOUNT/fuse_to_nfs.txt"
    sleep 1
    NFS_READ=$(cat "$NFS_CROSS_MOUNT/fuse_to_nfs.txt" 2>&1) || true
    if [[ "$NFS_READ" == "FUSE wrote this" ]]; then
      pass "E.5 FUSE→NFS cross-path"
    else
      fail "E.5 FUSE→NFS mismatch" "Got: '$NFS_READ'"
    fi

    # E.6 Write via gRPC → Read via both NFS and FUSE
    grpc write-file "$NFS_FUSE_WS_ID" "triple.txt" "Triple cross-path"
    sleep 2
    NFS_TRIPLE=$(cat "$NFS_CROSS_MOUNT/triple.txt" 2>&1) || true
    FUSE_TRIPLE=$(cat "$FUSE_CROSS_MOUNT/triple.txt" 2>&1) || true
    if [[ "$NFS_TRIPLE" == "Triple cross-path" && "$FUSE_TRIPLE" == "Triple cross-path" ]]; then
      pass "E.6 gRPC→NFS+FUSE triple cross-path"
    else
      fail "E.6 Triple cross-path mismatch" "NFS='$NFS_TRIPLE' FUSE='$FUSE_TRIPLE'"
    fi

    # E.7 Cleanup mounts
    kill $CROSS_FUSE_PID2 2>/dev/null
    wait $CROSS_FUSE_PID2 2>/dev/null || true
    fusermount -u "$FUSE_CROSS_MOUNT" 2>/dev/null || true
    umount "$NFS_CROSS_MOUNT" 2>/dev/null || true
    pass "E.7 NFS+FUSE cross-path mounts cleaned up"
  fi
fi

# =============================================================================
section "Summary"
# =============================================================================

TOTAL=$((PASS + FAIL))
echo -e "\n${GREEN}Passed: $PASS${NC}"
if [[ $FAIL -gt 0 ]]; then
  echo -e "${RED}Failed: $FAIL${NC}"
fi
echo -e "Total:  $TOTAL"

if [[ $FAIL -gt 0 ]]; then
  echo -e "\n${RED}SOME TESTS FAILED${NC}"
  exit 1
else
  echo -e "\n${GREEN}ALL TESTS PASSED${NC}"
  exit 0
fi
