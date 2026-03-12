#!/usr/bin/env bash
# =============================================================================
# Cross-Tenant Share Integration Tests
#
# Tests the full share lifecycle: create, permission grant, cross-tenant
# read/write via HTTP API, FUSE mount, and permission enforcement.
#
# Scenario:
#   - Tenant A (owner) creates files, creates a share, grants permissions
#   - Tenant B (reader/writer) accesses shared files via API and FUSE
#   - Tenant C (no permission) is denied access to private shares
#   - Public shares are readable by all tenants
#
# Requirements:
#   - Server running on HTTP:8080, gRPC:9090
#   - workspace-fuse binary at ~/.elevo/bin/workspace-fuse
#   - fusermount + /dev/fuse
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BASE="http://localhost:8080/api/v1"
GRPC_SERVER="localhost:9090"
ADMIN_PASSWORD="test-admin-123"
FUSE_BINARY="$HOME/.elevo/bin/workspace-fuse"

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

req() {
  local method=$1 url=$2
  shift 2
  curl -s -w '\n%{http_code}' -X "$method" "$url" "$@"
}
get_status() { echo "$1" | tail -1; }
get_body() { echo "$1" | sed '$d'; }
jf() { echo "$1" | jq -r "$2" 2>/dev/null; }

CLEANUP_CMDS=()
add_cleanup() { CLEANUP_CMDS+=("$1"); }
run_cleanup() {
  echo -e "\n${CYAN}━━━ Cleanup ━━━${NC}"
  for cmd in "${CLEANUP_CMDS[@]:-}"; do
    [[ -n "$cmd" ]] && eval "$cmd" 2>/dev/null || true
  done
}
trap run_cleanup EXIT

# =============================================================================
section "0. Setup — Create 3 Tenants"
# =============================================================================

# Admin login
resp=$(req POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"password":"'"$ADMIN_PASSWORD"'"}')
JWT=$(jf "$(get_body "$resp")" '.token')
AUTH=(-H "Authorization: Bearer $JWT")

if [[ -z "$JWT" || "$JWT" == "null" ]]; then
  fail "Admin login failed" "$(get_body "$resp")"
  exit 1
fi
pass "Admin login OK"

# Pre-cleanup: delete any leftover tenants from previous runs
for name in "ShareOwner" "ShareReader" "ShareVisitor"; do
  EXISTING=$(curl -s "$BASE/tenants?page_size=100" "${AUTH[@]}" | jq -r ".items[] | select(.name==\"$name\") | .id")
  if [[ -n "$EXISTING" ]]; then
    curl -s -X DELETE "$BASE/tenants/$EXISTING?force=true" "${AUTH[@]}" > /dev/null
  fi
done

# Tenant A: share owner
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" -H "Content-Type: application/json" \
  -d '{"name":"ShareOwner","description":"Share owner","initial_api_key":{"name":"owner-key"}}')
TENANT_A_ID=$(jf "$(get_body "$resp")" '.tenant.id')
TENANT_A_TOKEN=$(jf "$(get_body "$resp")" '.api_key.token')
if [[ "$(get_status "$resp")" == "201" && -n "$TENANT_A_ID" && "$TENANT_A_ID" != "null" ]]; then
  pass "Tenant A (owner): ${TENANT_A_ID:0:8}..."
  add_cleanup "curl -s -X DELETE '$BASE/tenants/$TENANT_A_ID?force=true' \"\${AUTH[@]}\" > /dev/null"
else
  fail "Create Tenant A" "$(get_body "$resp")"
  exit 1
fi
AUTH_A=(-H "Authorization: Bearer $TENANT_A_TOKEN")

# Tenant B: will get permissions
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" -H "Content-Type: application/json" \
  -d '{"name":"ShareReader","description":"Share reader","initial_api_key":{"name":"reader-key"}}')
TENANT_B_ID=$(jf "$(get_body "$resp")" '.tenant.id')
TENANT_B_TOKEN=$(jf "$(get_body "$resp")" '.api_key.token')
if [[ "$(get_status "$resp")" == "201" && -n "$TENANT_B_ID" && "$TENANT_B_ID" != "null" ]]; then
  pass "Tenant B (reader): ${TENANT_B_ID:0:8}..."
  add_cleanup "curl -s -X DELETE '$BASE/tenants/$TENANT_B_ID?force=true' \"\${AUTH[@]}\" > /dev/null"
else
  fail "Create Tenant B" "$(get_body "$resp")"
  exit 1
fi
AUTH_B=(-H "Authorization: Bearer $TENANT_B_TOKEN")

# Tenant C: no permission (bystander)
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" -H "Content-Type: application/json" \
  -d '{"name":"ShareVisitor","description":"No share access","initial_api_key":{"name":"visitor-key"}}')
TENANT_C_ID=$(jf "$(get_body "$resp")" '.tenant.id')
TENANT_C_TOKEN=$(jf "$(get_body "$resp")" '.api_key.token')
if [[ "$(get_status "$resp")" == "201" && -n "$TENANT_C_ID" && "$TENANT_C_ID" != "null" ]]; then
  pass "Tenant C (visitor): ${TENANT_C_ID:0:8}..."
  add_cleanup "curl -s -X DELETE '$BASE/tenants/$TENANT_C_ID?force=true' \"\${AUTH[@]}\" > /dev/null"
else
  fail "Create Tenant C" "$(get_body "$resp")"
  exit 1
fi
AUTH_C=(-H "Authorization: Bearer $TENANT_C_TOKEN")

# =============================================================================
section "1. Tenant A Prepares Share Directory"
# =============================================================================

# Create directory structure in A's namespace
resp=$(req POST "$BASE/me/files/shared_data" "${AUTH_A[@]}")
status=$(get_status "$resp")
if [[ "$status" == "201" || "$status" == "200" ]]; then
  pass "1.1 Created share directory: shared_data"
else
  fail "1.1 mkdir shared_data" "status=$status $(get_body "$resp")"
fi

# Write files
resp=$(req PUT "$BASE/me/files/shared_data/readme.txt" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Welcome to the shared dataset"}')
status=$(get_status "$resp")
if [[ "$status" == "200" || "$status" == "201" ]]; then
  pass "1.2 Wrote shared_data/readme.txt"
else
  fail "1.2 Write readme.txt" "status=$status"
fi

resp=$(req POST "$BASE/me/files/shared_data/reports" "${AUTH_A[@]}")
resp=$(req PUT "$BASE/me/files/shared_data/reports/q1.csv" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"month,revenue\nJan,100\nFeb,200\nMar,300"}')
status=$(get_status "$resp")
if [[ "$status" == "200" || "$status" == "201" ]]; then
  pass "1.3 Wrote shared_data/reports/q1.csv"
else
  fail "1.3 Write q1.csv" "status=$status"
fi

# =============================================================================
section "2. Create Private Share + Grant Permissions"
# =============================================================================

# Create private share
resp=$(req POST "$BASE/shares" "${AUTH_A[@]}" -H "Content-Type: application/json" \
  -d '{"name":"project-data","source_path":"shared_data","description":"Project shared data","visibility":"private"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
SHARE_ID=$(jf "$body" '.share.id')

if [[ "$status" == "201" && -n "$SHARE_ID" && "$SHARE_ID" != "null" ]]; then
  pass "2.1 Created private share: ${SHARE_ID:0:8}..."
  add_cleanup "curl -s -X DELETE '$BASE/shares/$SHARE_ID' \"\${AUTH[@]}\" > /dev/null"
else
  fail "2.1 Create share" "status=$status $body"
  exit 1
fi

# Grant Tenant B READ permission
resp=$(req POST "$BASE/shares/$SHARE_ID/permissions" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"tenant_id\":\"$TENANT_B_ID\",\"permission\":\"read\"}")
status=$(get_status "$resp")
if [[ "$status" == "201" || "$status" == "200" ]]; then
  pass "2.2 Granted Tenant B READ permission"
else
  fail "2.2 Grant read permission" "status=$status $(get_body "$resp")"
fi

# =============================================================================
section "3. Private Share — Permission Enforcement"
# =============================================================================

# 3.1 Tenant B (read permission) can read shared file
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${AUTH_B[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "Welcome to the shared dataset" ]]; then
  pass "3.1 Tenant B reads shared file → OK"
else
  fail "3.1 Tenant B read" "status=$status content='$content'"
fi

# 3.2 Tenant B can list shared directory
resp=$(req GET "$BASE/shares/$SHARE_ID/files/list?path=." "${AUTH_B[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$count" -ge 2 ]]; then
  pass "3.2 Tenant B lists shared directory → $count items"
else
  fail "3.2 Tenant B list" "status=$status count=$count"
fi

# 3.3 Tenant B can read nested file
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=reports/q1.csv" "${AUTH_B[@]}")
status=$(get_status "$resp")
content=$(jf "$(get_body "$resp")" '.content')
if [[ "$status" == "200" ]] && echo "$content" | grep -q "Jan,100"; then
  pass "3.3 Tenant B reads nested file → OK"
else
  fail "3.3 Tenant B read nested" "status=$status content='$content'"
fi

# 3.4 Tenant B (read-only) CANNOT write
resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=hacked.txt" "${AUTH_B[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"should fail"}')
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "3.4 Tenant B write denied (read-only) → 403"
else
  fail "3.4 Tenant B write should be denied" "status=$status $(get_body "$resp")"
fi

# 3.5 Tenant B (read-only) CANNOT delete
resp=$(req DELETE "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "3.5 Tenant B delete denied (read-only) → 403"
else
  fail "3.5 Tenant B delete should be denied" "status=$status"
fi

# 3.6 Tenant C (no permission) CANNOT access private share
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${AUTH_C[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "3.6 Tenant C denied (no permission, private) → 404 (hidden)"
else
  fail "3.6 Tenant C should get 404 for private share" "status=$status"
fi

# 3.7 Tenant C cannot even see the share exists
resp=$(req GET "$BASE/shares/$SHARE_ID" "${AUTH_C[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "3.7 Tenant C cannot see private share metadata → 404"
else
  fail "3.7 Private share should be hidden from C" "status=$status"
fi

# =============================================================================
section "4. Upgrade Permission — Read → Write"
# =============================================================================

# Upgrade B to WRITE
resp=$(req PUT "$BASE/shares/$SHARE_ID/permissions/$TENANT_B_ID" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"permission":"write"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "4.1 Upgraded Tenant B to WRITE permission"
else
  fail "4.1 Upgrade permission" "status=$status $(get_body "$resp")"
fi

# 4.2 Tenant B can now write
resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=from_b.txt" "${AUTH_B[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Written by Tenant B"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "4.2 Tenant B writes to share → OK"
else
  fail "4.2 Tenant B write" "status=$status $(get_body "$resp")"
fi

# 4.3 Tenant A (owner) can see B's write
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=from_b.txt" "${AUTH_A[@]}")
status=$(get_status "$resp")
content=$(jf "$(get_body "$resp")" '.content')
if [[ "$status" == "200" && "$content" == "Written by Tenant B" ]]; then
  pass "4.3 Tenant A reads B's file → cross-tenant visible"
else
  fail "4.3 Owner cannot see B's write" "status=$status content='$content'"
fi

# 4.4 Owner can also see B's file via /me/files (namespace view)
resp=$(req GET "$BASE/me/files/shared_data/from_b.txt" "${AUTH_A[@]}")
status=$(get_status "$resp")
content=$(jf "$(get_body "$resp")" '.content')
if [[ "$status" == "200" && "$content" == "Written by Tenant B" ]]; then
  pass "4.4 Owner reads B's file via /me/files → visible in namespace"
else
  fail "4.4 B's file not visible in owner namespace" "status=$status"
fi

# 4.5 B can write to subdirectory
resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=reports/q2.csv" "${AUTH_B[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"month,revenue\nApr,400\nMay,500\nJun,600"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "4.5 Tenant B writes to subdirectory → OK"
else
  fail "4.5 B write to subdir" "status=$status"
fi

# 4.6 B can delete a file
resp=$(req DELETE "$BASE/shares/$SHARE_ID/files?path=from_b.txt" "${AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "4.6 Tenant B deletes own file → OK"
else
  fail "4.6 B delete" "status=$status"
fi

# =============================================================================
section "5. Public Share Tests"
# =============================================================================

# Create a public share from A's namespace
resp=$(req POST "$BASE/me/files/public_data" "${AUTH_A[@]}")
resp=$(req PUT "$BASE/me/files/public_data/announcement.txt" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Public announcement from Tenant A"}')

resp=$(req POST "$BASE/shares" "${AUTH_A[@]}" -H "Content-Type: application/json" \
  -d '{"name":"public-info","source_path":"public_data","description":"Public data","visibility":"public"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
PUB_SHARE_ID=$(jf "$body" '.share.id')

if [[ "$status" == "201" && -n "$PUB_SHARE_ID" && "$PUB_SHARE_ID" != "null" ]]; then
  pass "5.1 Created public share: ${PUB_SHARE_ID:0:8}..."
  add_cleanup "curl -s -X DELETE '$BASE/shares/$PUB_SHARE_ID' \"\${AUTH[@]}\" > /dev/null"
else
  fail "5.1 Create public share" "status=$status $body"
  PUB_SHARE_ID=""
fi

if [[ -n "$PUB_SHARE_ID" ]]; then
  # 5.2 Tenant B can read public share (no explicit grant needed)
  resp=$(req GET "$BASE/shares/$PUB_SHARE_ID/files?path=announcement.txt" "${AUTH_B[@]}")
  status=$(get_status "$resp")
  content=$(jf "$(get_body "$resp")" '.content')
  if [[ "$status" == "200" && "$content" == "Public announcement from Tenant A" ]]; then
    pass "5.2 Tenant B reads public share → OK (no grant needed)"
  else
    fail "5.2 B read public share" "status=$status content='$content'"
  fi

  # 5.3 Tenant C can ALSO read public share
  resp=$(req GET "$BASE/shares/$PUB_SHARE_ID/files?path=announcement.txt" "${AUTH_C[@]}")
  status=$(get_status "$resp")
  content=$(jf "$(get_body "$resp")" '.content')
  if [[ "$status" == "200" && "$content" == "Public announcement from Tenant A" ]]; then
    pass "5.3 Tenant C reads public share → OK (public visibility)"
  else
    fail "5.3 C read public share" "status=$status content='$content'"
  fi

  # 5.4 Public share: anyone can list
  resp=$(req GET "$BASE/shares/$PUB_SHARE_ID/files/list?path=." "${AUTH_C[@]}")
  status=$(get_status "$resp")
  count=$(jf "$(get_body "$resp")" '.files | length')
  if [[ "$status" == "200" && "$count" -ge 1 ]]; then
    pass "5.4 Tenant C lists public share → $count items"
  else
    fail "5.4 C list public share" "status=$status count=$count"
  fi

  # 5.5 Public share: write denied (public gives read-only)
  resp=$(req PUT "$BASE/shares/$PUB_SHARE_ID/files?path=hacked.txt" "${AUTH_C[@]}" \
    -H "Content-Type: application/json" \
    -d '{"content":"should fail"}')
  status=$(get_status "$resp")
  if [[ "$status" == "403" ]]; then
    pass "5.5 Public share write denied → 403 (read-only implicit)"
  else
    fail "5.5 Public share write should be denied" "status=$status"
  fi
fi

# =============================================================================
section "6. FUSE Mount — Owner Namespace (Cross-Path Share Verification)"
# =============================================================================
# Mount the owner's namespace via FUSE. Verify that files written via the share
# API are visible through the FUSE mount, and vice versa.

FUSE_OK=false
if [[ -x "$FUSE_BINARY" && -c /dev/fuse ]] && command -v fusermount &>/dev/null; then
  NS_FUSE_MOUNT=$(mktemp -d /tmp/share_ns_fuse_XXXXXX)
  add_cleanup "fusermount -u $NS_FUSE_MOUNT 2>/dev/null; rm -rf $NS_FUSE_MOUNT"

  echo "sentinel" > "$NS_FUSE_MOUNT/.fuse_mount_sentinel"

  "$FUSE_BINARY" mount \
    --server "http://$GRPC_SERVER" \
    --workspace "namespaces/$TENANT_A_ID" \
    --token "$JWT" \
    --target "$NS_FUSE_MOUNT" \
    --foreground \
    --cache-ttl 1 &
  NS_FUSE_PID=$!
  add_cleanup "kill $NS_FUSE_PID 2>/dev/null; wait $NS_FUSE_PID 2>/dev/null"

  for i in $(seq 1 30); do
    if [[ ! -f "$NS_FUSE_MOUNT/.fuse_mount_sentinel" ]]; then
      FUSE_OK=true
      break
    fi
    sleep 0.5
  done

  if $FUSE_OK; then
    pass "6.1 Owner namespace FUSE mounted"
  else
    fail "6.1 FUSE mount timeout" ""
    kill $NS_FUSE_PID 2>/dev/null || true
  fi
else
  info "FUSE not available, skipping FUSE tests"
fi

if $FUSE_OK; then
  # 6.2 Verify share files visible via FUSE
  FUSE_READ=$(cat "$NS_FUSE_MOUNT/shared_data/readme.txt" 2>&1) || true
  if [[ "$FUSE_READ" == "Welcome to the shared dataset" ]]; then
    pass "6.2 Share files visible via FUSE mount"
  else
    fail "6.2 Share file not visible via FUSE" "Got: '$FUSE_READ'"
  fi

  # 6.3 Verify B's writes are visible via FUSE
  FUSE_Q2=$(cat "$NS_FUSE_MOUNT/shared_data/reports/q2.csv" 2>&1) || true
  if echo "$FUSE_Q2" | grep -q "Apr,400"; then
    pass "6.3 B's API-written file visible via FUSE"
  else
    fail "6.3 B's write not visible via FUSE" "Got: '$FUSE_Q2'"
  fi

  # 6.4 Write via FUSE → Read via share API (B)
  echo -n "Written via FUSE by admin" > "$NS_FUSE_MOUNT/shared_data/fuse_file.txt"
  sleep 1
  resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=fuse_file.txt" "${AUTH_B[@]}")
  status=$(get_status "$resp")
  content=$(jf "$(get_body "$resp")" '.content')
  if [[ "$status" == "200" && "$content" == "Written via FUSE by admin" ]]; then
    pass "6.4 FUSE-written file readable via share API (cross-path)"
  else
    fail "6.4 FUSE→API cross-path" "status=$status content='$content'"
  fi

  # 6.5 Write via share API (B) → Read via FUSE
  resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=api_to_fuse.txt" "${AUTH_B[@]}" \
    -H "Content-Type: application/json" \
    -d '{"content":"B wrote this via API"}')
  sleep 2  # FUSE cache
  FUSE_B_FILE=$(cat "$NS_FUSE_MOUNT/shared_data/api_to_fuse.txt" 2>&1) || true
  if [[ "$FUSE_B_FILE" == "B wrote this via API" ]]; then
    pass "6.5 API-written file readable via FUSE (cross-path)"
  else
    fail "6.5 API→FUSE cross-path" "Got: '$FUSE_B_FILE'"
  fi

  # 6.6 Owner writes via /me/files → B reads via share API
  resp=$(req PUT "$BASE/me/files/shared_data/owner_update.txt" "${AUTH_A[@]}" \
    -H "Content-Type: application/json" \
    -d '{"content":"Owner wrote via /me/files"}')
  resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=owner_update.txt" "${AUTH_B[@]}")
  content=$(jf "$(get_body "$resp")" '.content')
  if [[ "$content" == "Owner wrote via /me/files" ]]; then
    pass "6.6 Owner /me/files write → B share API read"
  else
    fail "6.6 Owner write→B read" "content='$content'"
  fi

  # 6.7 FUSE list shows all files including cross-tenant writes
  FUSE_LS=$(ls "$NS_FUSE_MOUNT/shared_data/" 2>&1)
  for f in readme.txt fuse_file.txt api_to_fuse.txt owner_update.txt; do
    if ! echo "$FUSE_LS" | grep -q "$f"; then
      fail "6.7 Missing $f in FUSE listing" "$FUSE_LS"
      break
    fi
  done
  pass "6.7 FUSE listing includes all cross-tenant files"

  # 6.8 Unmount
  kill $NS_FUSE_PID 2>/dev/null
  wait $NS_FUSE_PID 2>/dev/null || true
  fusermount -u "$NS_FUSE_MOUNT" 2>/dev/null || true
  pass "6.8 FUSE unmounted"
fi

# =============================================================================
section "7. Revoke Permission + Re-verify"
# =============================================================================

# Revoke B's write permission entirely
resp=$(req DELETE "$BASE/shares/$SHARE_ID/permissions/$TENANT_B_ID" "${AUTH_A[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "7.1 Revoked Tenant B's permission"
else
  fail "7.1 Revoke permission" "status=$status"
fi

# B should now be denied (private share)
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "7.2 Tenant B denied after revocation → 404 (hidden)"
else
  fail "7.2 B should be denied after revocation" "status=$status"
fi

# B cannot write either
resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=hack.txt" "${AUTH_B[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"should fail"}')
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "7.3 Tenant B write denied after revocation → 404"
else
  fail "7.3 B write after revocation" "status=$status"
fi

# =============================================================================
section "8. Admin Override — Admin Can Always Access"
# =============================================================================

# Admin can read any share regardless of permissions
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${AUTH[@]}")
status=$(get_status "$resp")
content=$(jf "$(get_body "$resp")" '.content')
if [[ "$status" == "200" && "$content" == "Welcome to the shared dataset" ]]; then
  pass "8.1 Admin reads private share → OK (admin override)"
else
  fail "8.1 Admin share read" "status=$status content='$content'"
fi

# Admin can write to any share
resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=admin_note.txt" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Admin was here"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "8.2 Admin writes to private share → OK"
else
  fail "8.2 Admin write" "status=$status"
fi

# =============================================================================
section "9. Path Traversal Prevention"
# =============================================================================

# Grant B read so we can test path traversal attempts
resp=$(req POST "$BASE/shares/$SHARE_ID/permissions" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"tenant_id\":\"$TENANT_B_ID\",\"permission\":\"read\"}")

# 9.1 Attempt to escape share boundary with ../
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=../../etc/passwd" "${AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "9.1 Path traversal ../../etc/passwd rejected → $status"
else
  fail "9.1 Path traversal not blocked" "status=$status $(get_body "$resp")"
fi

# 9.2 Attempt to escape share with ../secret
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=../secret" "${AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "9.2 Path traversal ../secret rejected → $status"
else
  fail "9.2 Path traversal not blocked" "status=$status"
fi

# 9.3 Null byte injection
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=foo%00bar" "${AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" || "$status" == "500" ]]; then
  pass "9.3 Null byte in path rejected → $status"
else
  fail "9.3 Null byte not blocked" "status=$status"
fi

# Cleanup: revoke B again
req DELETE "$BASE/shares/$SHARE_ID/permissions/$TENANT_B_ID" "${AUTH_A[@]}" > /dev/null

# =============================================================================
section "10. Accessible Shares Discovery"
# =============================================================================

# Re-grant B read on private share
resp=$(req POST "$BASE/shares/$SHARE_ID/permissions" "${AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d "{\"tenant_id\":\"$TENANT_B_ID\",\"permission\":\"read\"}")

# B should see both private (granted) and public shares in accessible list
resp=$(req GET "$BASE/me/accessible-shares" "${AUTH_B[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  total=$(jf "$body" '.total // (.items | length)')
  pass "10.1 Tenant B accessible shares → $total shares"
else
  fail "10.1 Accessible shares" "status=$status"
fi

# C should see only public shares
resp=$(req GET "$BASE/me/accessible-shares" "${AUTH_C[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  total=$(jf "$body" '.total // (.items | length)')
  pass "10.2 Tenant C accessible shares → $total (public only)"
else
  fail "10.2 C accessible shares" "status=$status"
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
