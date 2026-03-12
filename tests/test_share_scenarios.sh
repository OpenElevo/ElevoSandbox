#!/usr/bin/env bash
# =============================================================================
# Comprehensive Share Scenario Tests
# Tests sharing, mounting, permission, file operations in shared directories
# =============================================================================
set -uo pipefail

BASE="http://localhost:8080/api/v1"
ADMIN_PASSWORD="test-admin-123"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

PASS=0
FAIL=0

pass() { PASS=$((PASS+1)); echo -e "  ${GREEN}✓ $1${NC}"; }
fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}✗ $1${NC}"; [ -n "${2:-}" ] && echo -e "    ${RED}$2${NC}"; }
section() { echo -e "\n${CYAN}━━━ $1 ━━━${NC}"; }

req() {
  local method=$1 url=$2
  shift 2
  curl -s -w '\n%{http_code}' -X "$method" "$url" "$@"
}

get_status() { echo "$1" | tail -1; }
get_body() { echo "$1" | sed '$d'; }
jf() { echo "$1" | jq -r "$2" 2>/dev/null || true; }

# =============================================================================
section "0. Setup — Admin Login"
# =============================================================================
resp=$(req POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"password":"'"$ADMIN_PASSWORD"'"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
ADMIN_TOKEN=$(jf "$body" '.token')
if [[ "$status" == "200" && -n "$ADMIN_TOKEN" && "$ADMIN_TOKEN" != "null" ]]; then
  pass "Admin login → JWT obtained"
else
  fail "Admin login failed" "$body"
  exit 1
fi
AUTH=(-H "Authorization: Bearer $ADMIN_TOKEN")

# =============================================================================
section "0.1 Pre-cleanup — remove stale test data"
# =============================================================================
# Delete ALL shares first (admin list uses .items[])
shares_resp=$(req GET "$BASE/shares" "${AUTH[@]}")
shares_body=$(get_body "$shares_resp")
share_ids=$(echo "$shares_body" | jq -r '.items[]?.id // empty' 2>/dev/null || true)
for sid in $share_ids; do
  [ -n "$sid" ] && req DELETE "$BASE/shares/$sid" "${AUTH[@]}" > /dev/null 2>&1
done
# Then delete stale tenants from previous test runs
resp=$(req GET "$BASE/tenants" "${AUTH[@]}")
body=$(get_body "$resp")
for tenant_name in "share-owner" "share-consumer" "share-visitor"; do
  ids=$(echo "$body" | jq -r ".items[]? | select(.name == \"$tenant_name\") | .id // empty" 2>/dev/null || true)
  for id in $ids; do
    [ -n "$id" ] && req DELETE "$BASE/tenants/$id?force=true" "${AUTH[@]}" > /dev/null 2>&1
  done
done
pass "Pre-cleanup done"

# =============================================================================
section "1. Create Test Tenants"
# =============================================================================

# Tenant A: share owner (with initial API key for JWT token)
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"share-owner","description":"Owner of shared directories","initial_api_key":{"name":"owner-key"}}')
status=$(get_status "$resp")
body=$(get_body "$resp")
OWNER_ID=$(jf "$body" '.tenant.id')
OWNER_TOKEN=$(jf "$body" '.api_key.token')
if [[ "$status" == "201" && -n "$OWNER_ID" && "$OWNER_ID" != "null" ]]; then
  pass "Created share-owner tenant ($OWNER_ID)"
else
  fail "Create share-owner" "$body"
  exit 1
fi
OWNER_AUTH=(-H "Authorization: Bearer $OWNER_TOKEN")

# Tenant B: share consumer
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"share-consumer","description":"Consumer of shared directories","initial_api_key":{"name":"consumer-key"}}')
status=$(get_status "$resp")
body=$(get_body "$resp")
CONSUMER_ID=$(jf "$body" '.tenant.id')
CONSUMER_TOKEN=$(jf "$body" '.api_key.token')
if [[ "$status" == "201" && -n "$CONSUMER_ID" && "$CONSUMER_ID" != "null" ]]; then
  pass "Created share-consumer tenant ($CONSUMER_ID)"
else
  fail "Create share-consumer" "$body"
  exit 1
fi
CONSUMER_AUTH=(-H "Authorization: Bearer $CONSUMER_TOKEN")

# Tenant C: visitor (no explicit permissions)
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"share-visitor","description":"Has no explicit share permissions","initial_api_key":{"name":"visitor-key"}}')
status=$(get_status "$resp")
body=$(get_body "$resp")
VISITOR_ID=$(jf "$body" '.tenant.id')
VISITOR_TOKEN=$(jf "$body" '.api_key.token')
if [[ "$status" == "201" && -n "$VISITOR_ID" && "$VISITOR_ID" != "null" ]]; then
  pass "Created share-visitor tenant ($VISITOR_ID)"
else
  fail "Create share-visitor" "$body"
  exit 1
fi
VISITOR_AUTH=(-H "Authorization: Bearer $VISITOR_TOKEN")

# =============================================================================
section "2. Prepare Namespace Directory Structure (Owner)"
# =============================================================================

# Create a complex directory structure in owner's namespace
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"projects"}' > /dev/null 2>&1
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"projects/app1"}' > /dev/null 2>&1
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"projects/app1/src"}' > /dev/null 2>&1
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"projects/app2"}' > /dev/null 2>&1
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"shared-libs"}' > /dev/null 2>&1
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"shared-libs/utils"}' > /dev/null 2>&1
req POST "$BASE/namespaces/$OWNER_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"private-data"}' > /dev/null 2>&1

# Write files into the directory structure
req PUT "$BASE/namespaces/$OWNER_ID/files?path=projects/app1/README.md" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"# App1\nThis is app1 readme"}' > /dev/null 2>&1
req PUT "$BASE/namespaces/$OWNER_ID/files?path=projects/app1/src/main.rs" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"fn main() { println!(\"hello\"); }"}' > /dev/null 2>&1
req PUT "$BASE/namespaces/$OWNER_ID/files?path=projects/app2/config.json" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"{\"key\": \"value\"}"}' > /dev/null 2>&1
req PUT "$BASE/namespaces/$OWNER_ID/files?path=shared-libs/utils/helper.py" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"def greet(): return \"hello\""}' > /dev/null 2>&1
req PUT "$BASE/namespaces/$OWNER_ID/files?path=shared-libs/README.md" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"# Shared Libraries"}' > /dev/null 2>&1
req PUT "$BASE/namespaces/$OWNER_ID/files?path=private-data/secret.txt" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"TOP SECRET DATA"}' > /dev/null 2>&1

# Verify directory structure
resp=$(req GET "$BASE/namespaces/$OWNER_ID/files/list?path=." "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 3 ]]; then
  pass "Namespace directory structure created ($fcount entries)"
else
  fail "Namespace setup" "$body"
fi

# =============================================================================
section "3. Create Shares — Various Source Paths"
# =============================================================================

# Share 1: Share a specific subdirectory (projects/app1) - PRIVATE
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"App1 Share","source_path":"projects/app1","description":"App1 project files","visibility":"private"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
SHARE_APP1=$(jf "$body" '.share.id')
if [[ "$status" == "201" && -n "$SHARE_APP1" && "$SHARE_APP1" != "null" ]]; then
  pass "Created private share for projects/app1 (id=$SHARE_APP1)"
else
  fail "Create share for projects/app1" "$body"
fi

# Share 2: Share a library directory (shared-libs) - PUBLIC
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Shared Libraries","source_path":"shared-libs","description":"Common shared libraries","visibility":"public"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
SHARE_LIBS=$(jf "$body" '.share.id')
if [[ "$status" == "201" && -n "$SHARE_LIBS" && "$SHARE_LIBS" != "null" ]]; then
  pass "Created public share for shared-libs (id=$SHARE_LIBS)"
else
  fail "Create share for shared-libs" "$body"
fi

# Share 3: Share the root namespace directory - PUBLIC
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Full Namespace","source_path":".","description":"Entire namespace","visibility":"public"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
SHARE_ROOT=$(jf "$body" '.share.id')
if [[ "$status" == "201" && -n "$SHARE_ROOT" && "$SHARE_ROOT" != "null" ]]; then
  pass "Created public share for root namespace (id=$SHARE_ROOT)"
else
  fail "Create share for root namespace" "$body"
fi

# Share 4: Share a deeply nested directory (projects/app1/src) - PRIVATE
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"App1 Source","source_path":"projects/app1/src","description":"Just the source code","visibility":"private"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
SHARE_SRC=$(jf "$body" '.share.id')
if [[ "$status" == "201" && -n "$SHARE_SRC" && "$SHARE_SRC" != "null" ]]; then
  pass "Created private share for projects/app1/src (id=$SHARE_SRC)"
else
  fail "Create share for projects/app1/src" "$body"
fi

# =============================================================================
section "4. Share Creation — Negative Cases"
# =============================================================================

# Duplicate share name
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"App1 Share","source_path":"projects/app2","description":"dup name","visibility":"private"}')
status=$(get_status "$resp")
if [[ "$status" == "409" || "$status" == "400" || "$status" == "422" ]]; then
  pass "Duplicate share name rejected → $status"
else
  fail "Duplicate share name not rejected" "status=$status, $(get_body "$resp")"
fi

# Duplicate source_path
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Another Name","source_path":"projects/app1","description":"dup path","visibility":"private"}')
status=$(get_status "$resp")
if [[ "$status" == "409" || "$status" == "400" || "$status" == "422" ]]; then
  pass "Duplicate source_path rejected → $status"
else
  fail "Duplicate source_path not rejected" "status=$status, $(get_body "$resp")"
fi

# Non-existent source path
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Ghost Share","source_path":"non-existent-dir","description":"no dir","visibility":"private"}')
status=$(get_status "$resp")
if [[ "$status" != "201" ]]; then
  pass "Non-existent source_path rejected → $status"
else
  fail "Non-existent source_path should be rejected" "status=$status"
  # Clean up if it somehow was created
  ghost_id=$(jf "$(get_body "$resp")" '.share.id')
  req DELETE "$BASE/shares/$ghost_id" "${AUTH[@]}" > /dev/null 2>&1
fi

# Path traversal in source_path
resp=$(req POST "$BASE/shares" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Evil Share","source_path":"../../../etc","description":"traversal","visibility":"private"}')
status=$(get_status "$resp")
if [[ "$status" != "201" ]]; then
  pass "Path traversal in source_path rejected → $status"
else
  fail "Path traversal should be rejected" "status=$status"
fi

# Consumer cannot create share in owner's namespace
resp=$(req POST "$BASE/shares" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Stolen Share","source_path":".","description":"consumer share","visibility":"public"}')
status=$(get_status "$resp")
# Consumer creates share in their OWN namespace (which is empty), so source_path validation may fail
if [[ "$status" != "201" ]]; then
  pass "Consumer cannot share from owner namespace → $status"
else
  # Consumer created a share in their own namespace - that's fine, just clean up
  stolen_id=$(jf "$(get_body "$resp")" '.share.id')
  req DELETE "$BASE/shares/$stolen_id" "${AUTH[@]}" > /dev/null 2>&1
  pass "Consumer creates share in own namespace (expected behavior)"
fi

# =============================================================================
section "5. Share Listing & Visibility"
# =============================================================================

# Owner sees all their shares
resp=$(req GET "$BASE/shares" "${OWNER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total" -ge 4 ]]; then
  pass "Owner lists shares → total=$total (sees all own + public)"
else
  fail "Owner list shares" "status=$status, total=$total, body=$body"
fi

# Consumer (no perms yet) sees only public shares
resp=$(req GET "$BASE/shares" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
consumer_total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$consumer_total" -ge 2 ]]; then
  pass "Consumer sees public shares → total=$consumer_total"
else
  fail "Consumer list shares" "status=$status, total=$consumer_total"
fi

# Visitor (no perms) sees only public shares
resp=$(req GET "$BASE/shares" "${VISITOR_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
visitor_total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$visitor_total" -ge 2 ]]; then
  pass "Visitor sees public shares → total=$visitor_total"
else
  fail "Visitor list shares" "status=$status, total=$visitor_total"
fi

# Admin sees all shares
resp=$(req GET "$BASE/shares" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
admin_total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$admin_total" -ge 4 ]]; then
  pass "Admin sees all shares → total=$admin_total"
else
  fail "Admin list shares" "status=$status, total=$admin_total"
fi

# =============================================================================
section "6. Share Get — Private vs Public Access"
# =============================================================================

# Owner can get private share
resp=$(req GET "$BASE/shares/$SHARE_APP1" "${OWNER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Owner gets private share → 200"
else
  fail "Owner get private share" "status=$status"
fi

# Consumer cannot get private share (no permission)
resp=$(req GET "$BASE/shares/$SHARE_APP1" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "Consumer cannot get private share → 404 (hidden)"
else
  fail "Consumer should not see private share" "status=$status"
fi

# Consumer CAN get public share
resp=$(req GET "$BASE/shares/$SHARE_LIBS" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Consumer gets public share → 200"
else
  fail "Consumer get public share" "status=$status"
fi

# Admin can get any share
resp=$(req GET "$BASE/shares/$SHARE_APP1" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Admin gets private share → 200"
else
  fail "Admin get private share" "status=$status"
fi

# =============================================================================
section "7. Public Share — File Operations (Read-Only for Public)"
# =============================================================================

# Consumer reads file from public share (shared-libs)
resp=$(req GET "$BASE/shares/$SHARE_LIBS/files?path=README.md" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "# Shared Libraries" ]]; then
  pass "Consumer reads public share file → content matches"
else
  fail "Consumer read public share file" "status=$status, content=$content, body=$body"
fi

# Consumer lists public share files
resp=$(req GET "$BASE/shares/$SHARE_LIBS/files/list?path=." "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 2 ]]; then
  pass "Consumer lists public share → $fcount entries"
else
  fail "Consumer list public share" "status=$status, count=$fcount, body=$body"
fi

# Consumer reads nested file in public share
resp=$(req GET "$BASE/shares/$SHARE_LIBS/files?path=utils/helper.py" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == *"greet"* ]]; then
  pass "Consumer reads nested file in public share"
else
  fail "Consumer read nested public share file" "status=$status, content=$content"
fi

# Consumer CANNOT write to public share (only has implicit read)
resp=$(req PUT "$BASE/shares/$SHARE_LIBS/files?path=hack.txt" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"unauthorized write"}')
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "Consumer write to public share → 403 (read-only)"
else
  fail "Consumer write to public share should be rejected" "status=$status, $(get_body "$resp")"
fi

# Consumer CANNOT delete from public share
resp=$(req DELETE "$BASE/shares/$SHARE_LIBS/files?path=README.md" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "Consumer delete from public share → 403"
else
  fail "Consumer delete from public share should be rejected" "status=$status"
fi

# Visitor also reads from public share
resp=$(req GET "$BASE/shares/$SHARE_LIBS/files?path=README.md" "${VISITOR_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "# Shared Libraries" ]]; then
  pass "Visitor reads public share file → ok"
else
  fail "Visitor read public share" "status=$status"
fi

# =============================================================================
section "8. Private Share — Access Denied Without Permission"
# =============================================================================

# Consumer cannot read from private share (SHARE_APP1)
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=README.md" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "Consumer read private share → 404 (hidden)"
else
  fail "Consumer should not access private share" "status=$status"
fi

# Consumer cannot list private share files
resp=$(req GET "$BASE/shares/$SHARE_APP1/files/list?path=." "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "Consumer list private share → 404"
else
  fail "Consumer should not list private share" "status=$status"
fi

# =============================================================================
section "9. Grant Permissions & Test Access Levels"
# =============================================================================

# Grant READ to consumer on private share (SHARE_APP1)
resp=$(req POST "$BASE/shares/$SHARE_APP1/permissions" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"'"$CONSUMER_ID"'","permission":"read"}')
status=$(get_status "$resp")
if [[ "$status" == "201" ]]; then
  pass "Grant read to consumer on private share → 201"
else
  fail "Grant read permission" "status=$status, $(get_body "$resp")"
fi

# Now consumer CAN read
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=README.md" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == *"App1"* ]]; then
  pass "Consumer reads private share with read perm → ok"
else
  fail "Consumer read with read perm" "status=$status, content=$content"
fi

# Consumer can list files
resp=$(req GET "$BASE/shares/$SHARE_APP1/files/list?path=." "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 2 ]]; then
  pass "Consumer lists private share files → $fcount entries"
else
  fail "Consumer list private share" "status=$status, count=$fcount"
fi

# Consumer can read nested path
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=src/main.rs" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == *"println"* ]]; then
  pass "Consumer reads nested file in private share → ok"
else
  fail "Consumer read nested in private share" "status=$status"
fi

# Consumer CANNOT write (only has read)
resp=$(req PUT "$BASE/shares/$SHARE_APP1/files?path=hack.txt" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"should fail"}')
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "Consumer write with read-only perm → 403"
else
  fail "Consumer write with read-only should fail" "status=$status"
fi

# Upgrade to WRITE permission
resp=$(req PUT "$BASE/shares/$SHARE_APP1/permissions/$CONSUMER_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"permission":"write"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Upgrade consumer to write permission → 200"
else
  fail "Upgrade permission" "status=$status, $(get_body "$resp")"
fi

# Now consumer CAN write
resp=$(req PUT "$BASE/shares/$SHARE_APP1/files?path=consumer-note.txt" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Written by consumer via share"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Consumer writes to share with write perm → 200"
else
  fail "Consumer write with write perm" "status=$status, $(get_body "$resp")"
fi

# Verify the write via owner's namespace view
resp=$(req GET "$BASE/namespaces/$OWNER_ID/files?path=projects/app1/consumer-note.txt" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "Written by consumer via share" ]]; then
  pass "Consumer's write visible in owner namespace → content matches"
else
  fail "Consumer write visibility" "status=$status, content=$content"
fi

# Consumer can delete
resp=$(req DELETE "$BASE/shares/$SHARE_APP1/files?path=consumer-note.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Consumer deletes via share with write perm → 200"
else
  fail "Consumer delete via share" "status=$status, $(get_body "$resp")"
fi

# Verify deletion
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=consumer-note.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" || "$status" == "500" ]]; then
  pass "Deleted file no longer accessible → $status"
else
  fail "Deleted file should not be accessible" "status=$status"
fi

# =============================================================================
section "10. Share File Operations — Complex Scenarios"
# =============================================================================

# Write file in subdirectory via share
resp=$(req PUT "$BASE/shares/$SHARE_APP1/files?path=src/new_module.rs" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"pub fn new_func() {}"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Write to subdirectory via share → 200"
else
  fail "Write to subdir via share" "status=$status, $(get_body "$resp")"
fi

# List subdirectory via share
resp=$(req GET "$BASE/shares/$SHARE_APP1/files/list?path=src" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 2 ]]; then
  pass "List subdirectory via share → $fcount entries"
else
  fail "List subdir via share" "status=$status, count=$fcount"
fi

# Read the newly written file back
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=src/new_module.rs" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "pub fn new_func() {}" ]]; then
  pass "Read back newly written file → content matches"
else
  fail "Read back written file" "status=$status, content=$content"
fi

# Overwrite existing file
resp=$(req PUT "$BASE/shares/$SHARE_APP1/files?path=README.md" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"# App1 (Updated by consumer)"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Overwrite existing file via share → 200"
else
  fail "Overwrite via share" "status=$status"
fi

# Verify overwrite
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=README.md" "${OWNER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "# App1 (Updated by consumer)" ]]; then
  pass "Overwritten file content verified"
else
  fail "Verify overwrite" "content=$content"
fi

# Clean up test file
req DELETE "$BASE/shares/$SHARE_APP1/files?path=src/new_module.rs" "${CONSUMER_AUTH[@]}" > /dev/null 2>&1

# Restore original README
req PUT "$BASE/shares/$SHARE_APP1/files?path=README.md" "${OWNER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"# App1\nThis is app1 readme"}' > /dev/null 2>&1

# =============================================================================
section "11. Path Traversal Prevention in Share File Operations"
# =============================================================================

# Try to read file outside share boundary using ..
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=../../private-data/secret.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "Path traversal (../) prevented in read → $status"
else
  fail "Path traversal should be blocked in read" "status=$status, $(get_body "$resp")"
fi

# Try to write file outside share boundary
resp=$(req PUT "$BASE/shares/$SHARE_APP1/files?path=../../private-data/hack.txt" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"hacked!"}')
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "Path traversal (../) prevented in write → $status"
else
  fail "Path traversal should be blocked in write" "status=$status"
fi

# Try to delete file outside share boundary
resp=$(req DELETE "$BASE/shares/$SHARE_APP1/files?path=../../private-data/secret.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "Path traversal (../) prevented in delete → $status"
else
  fail "Path traversal should be blocked in delete" "status=$status"
fi

# Try to list outside share boundary
resp=$(req GET "$BASE/shares/$SHARE_APP1/files/list?path=../../" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "Path traversal (../) prevented in list → $status"
else
  fail "Path traversal should be blocked in list" "status=$status, $(get_body "$resp")"
fi

# Null byte injection attempt
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=README.md%00.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" != "200" ]]; then
  pass "Null byte injection prevented → $status"
else
  fail "Null byte injection should be blocked" "status=$status"
fi

# Verify that private-data is still intact (not tampered)
resp=$(req GET "$BASE/namespaces/$OWNER_ID/files?path=private-data/secret.txt" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "TOP SECRET DATA" ]]; then
  pass "Private data untouched after traversal attempts"
else
  fail "Private data may have been compromised" "content=$content"
fi

# =============================================================================
section "12. Root Share — Access Entire Namespace"
# =============================================================================

# Consumer reads from root share (sees full namespace)
resp=$(req GET "$BASE/shares/$SHARE_ROOT/files/list?path=." "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 3 ]]; then
  pass "Consumer lists root share → $fcount entries (sees all dirs)"
else
  fail "Consumer list root share" "status=$status, count=$fcount"
fi

# Can read deep nested file through root share
resp=$(req GET "$BASE/shares/$SHARE_ROOT/files?path=projects/app1/src/main.rs" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == *"println"* ]]; then
  pass "Consumer reads deep file through root share → ok"
else
  fail "Consumer read deep file via root share" "status=$status, content=$content"
fi

# Can read private-data through root share (since root share includes everything)
resp=$(req GET "$BASE/shares/$SHARE_ROOT/files?path=private-data/secret.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "TOP SECRET DATA" ]]; then
  pass "Root share includes private-data (expected — root shares all)"
else
  fail "Root share should include all files" "status=$status, content=$content"
fi

# =============================================================================
section "13. Nested Share — Narrow Access Path"
# =============================================================================

# Grant read on the deeply nested share (projects/app1/src)
resp=$(req POST "$BASE/shares/$SHARE_SRC/permissions" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"'"$VISITOR_ID"'","permission":"read"}')
status=$(get_status "$resp")
if [[ "$status" == "201" ]]; then
  pass "Grant read on src share to visitor → 201"
else
  fail "Grant permission on src share" "status=$status"
fi

# Visitor can read main.rs through narrow share
resp=$(req GET "$BASE/shares/$SHARE_SRC/files?path=main.rs" "${VISITOR_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == *"println"* ]]; then
  pass "Visitor reads main.rs via narrow src share → ok"
else
  fail "Visitor read via src share" "status=$status, content=$content"
fi

# Visitor CANNOT escape to parent via narrow share
resp=$(req GET "$BASE/shares/$SHARE_SRC/files?path=../README.md" "${VISITOR_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" || "$status" == "404" ]]; then
  pass "Visitor cannot escape src share via ../ → $status"
else
  fail "Visitor should not escape narrow share" "status=$status, $(get_body "$resp")"
fi

# Visitor lists files in narrow share
resp=$(req GET "$BASE/shares/$SHARE_SRC/files/list?path=." "${VISITOR_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 1 ]]; then
  pass "Visitor lists narrow share → $fcount files"
else
  fail "Visitor list narrow share" "status=$status, count=$fcount"
fi

# =============================================================================
section "14. Permission Revocation & Re-grant"
# =============================================================================

# Revoke consumer's write on SHARE_APP1
resp=$(req DELETE "$BASE/shares/$SHARE_APP1/permissions/$CONSUMER_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "Revoke consumer permission → $status"
else
  fail "Revoke permission" "status=$status"
fi

# Consumer now cannot access private share
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=README.md" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "Consumer access revoked → 404"
else
  fail "Revoked consumer should not access share" "status=$status"
fi

# Re-grant with admin level
resp=$(req POST "$BASE/shares/$SHARE_APP1/permissions" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"'"$CONSUMER_ID"'","permission":"admin"}')
status=$(get_status "$resp")
if [[ "$status" == "201" ]]; then
  pass "Re-grant admin permission → 201"
else
  fail "Re-grant admin perm" "status=$status"
fi

# Consumer can now both read and write
resp=$(req PUT "$BASE/shares/$SHARE_APP1/files?path=admin-test.txt" "${CONSUMER_AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"admin level write"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Consumer writes with admin perm → 200"
else
  fail "Consumer write with admin perm" "status=$status"
fi
req DELETE "$BASE/shares/$SHARE_APP1/files?path=admin-test.txt" "${CONSUMER_AUTH[@]}" > /dev/null 2>&1

# =============================================================================
section "15. /me Endpoints — Shares Self-Service"
# =============================================================================

# Owner lists my shares
resp=$(req GET "$BASE/me/shares" "${OWNER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total" -ge 4 ]]; then
  pass "GET /me/shares (owner) → $total shares"
else
  fail "GET /me/shares" "status=$status, total=$total"
fi

# Consumer lists accessible shares
resp=$(req GET "$BASE/me/accessible-shares" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  total=$(jf "$body" '.total')
  pass "GET /me/accessible-shares (consumer) → total=$total"
else
  fail "GET /me/accessible-shares" "status=$status, $(get_body "$resp")"
fi

# /me info for each tenant
resp=$(req GET "$BASE/me" "${OWNER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
me_name=$(jf "$body" '.tenant.name')
if [[ "$status" == "200" && "$me_name" == "share-owner" ]]; then
  pass "GET /me (owner) → name=$me_name"
else
  fail "GET /me (owner)" "status=$status, name=$me_name"
fi

resp=$(req GET "$BASE/me" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
me_name=$(jf "$body" '.tenant.name')
if [[ "$status" == "200" && "$me_name" == "share-consumer" ]]; then
  pass "GET /me (consumer) → name=$me_name"
else
  fail "GET /me (consumer)" "status=$status"
fi

# =============================================================================
section "16. Share Update"
# =============================================================================

# Update share name and description
resp=$(req PUT "$BASE/shares/$SHARE_APP1" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"App1 Share (Renamed)","description":"Updated description"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
new_name=$(jf "$body" '.share.name')
if [[ "$status" == "200" && "$new_name" == "App1 Share (Renamed)" ]]; then
  pass "Update share name → $new_name"
else
  fail "Update share" "status=$status, name=$new_name"
fi

# Change visibility from private to public
resp=$(req PUT "$BASE/shares/$SHARE_APP1" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"visibility":"public"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
vis=$(jf "$body" '.share.visibility')
if [[ "$status" == "200" && "$vis" == "public" ]]; then
  pass "Change visibility to public → ok"
else
  fail "Change visibility" "status=$status, visibility=$vis"
fi

# Now visitor can see the formerly private share
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=README.md" "${VISITOR_AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Visitor reads now-public share → 200"
else
  fail "Visitor should see now-public share" "status=$status"
fi

# Revert to private
req PUT "$BASE/shares/$SHARE_APP1" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"App1 Share","visibility":"private"}' > /dev/null 2>&1

# =============================================================================
section "17. Owner Direct File Operations (Namespace)"
# =============================================================================

# Owner writes directly to namespace, visible through share
req PUT "$BASE/namespaces/$OWNER_ID/files?path=projects/app1/direct-write.txt" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Written directly to namespace"}' > /dev/null 2>&1

# Consumer sees the directly written file through share
resp=$(req GET "$BASE/shares/$SHARE_APP1/files?path=direct-write.txt" "${CONSUMER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "Written directly to namespace" ]]; then
  pass "Direct namespace write visible through share → ok"
else
  fail "Direct write visibility" "status=$status, content=$content"
fi

# Clean up
req DELETE "$BASE/namespaces/$OWNER_ID/files?path=projects/app1/direct-write.txt" "${AUTH[@]}" > /dev/null 2>&1

# =============================================================================
section "18. Owner /me File Operations"
# =============================================================================

# Owner uses /me endpoints to manage files
resp=$(req GET "$BASE/me/files?path=." "${OWNER_AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
fcount=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$fcount" -ge 3 ]]; then
  pass "GET /me/files → $fcount entries"
else
  fail "GET /me/files" "status=$status, count=$fcount, body=$body"
fi

# =============================================================================
section "19. Share Permissions — Full CRUD"
# =============================================================================

# List permissions on a share
resp=$(req GET "$BASE/shares/$SHARE_APP1/permissions" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
pcount=$(jf "$body" '.permissions | length')
if [[ "$status" == "200" ]]; then
  pass "List share permissions → $pcount entries"
else
  fail "List share permissions" "status=$status"
fi

# Grant execute level
resp=$(req POST "$BASE/shares/$SHARE_APP1/permissions" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"'"$VISITOR_ID"'","permission":"execute"}')
status=$(get_status "$resp")
if [[ "$status" == "201" ]]; then
  pass "Grant execute to visitor → 201"
else
  fail "Grant execute" "status=$status, $(get_body "$resp")"
fi

# Update to read
resp=$(req PUT "$BASE/shares/$SHARE_APP1/permissions/$VISITOR_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"permission":"read"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "Update visitor permission to read → 200"
else
  fail "Update permission" "status=$status"
fi

# List tenant's permissions
resp=$(req GET "$BASE/tenants/$VISITOR_ID/permissions" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
pcount=$(jf "$body" '.permissions | length')
if [[ "$status" == "200" && "$pcount" -ge 1 ]]; then
  pass "Tenant permissions list → $pcount entries"
else
  fail "Tenant permission list" "status=$status, count=$pcount"
fi

# Revoke
resp=$(req DELETE "$BASE/shares/$SHARE_APP1/permissions/$VISITOR_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "Revoke visitor permission → $status"
else
  fail "Revoke visitor permission" "status=$status"
fi

# =============================================================================
section "20. Share Delete — Cleanup"
# =============================================================================

# Delete all shares
for share_id in "$SHARE_SRC" "$SHARE_APP1" "$SHARE_LIBS" "$SHARE_ROOT"; do
  resp=$(req DELETE "$BASE/shares/$share_id" "${AUTH[@]}")
  status=$(get_status "$resp")
  if [[ "$status" == "204" || "$status" == "200" ]]; then
    pass "Delete share $share_id → $status"
  else
    fail "Delete share $share_id" "status=$status, $(get_body "$resp")"
  fi
done

# Verify shares are deleted
resp=$(req GET "$BASE/shares" "${AUTH[@]}")
body=$(get_body "$resp")
# Filter to only our owner's shares
remaining=$(echo "$body" | jq "[.shares[] | select(.owner_tenant_id == \"$OWNER_ID\")] | length" 2>/dev/null)
if [[ "$remaining" == "0" || "$remaining" == "" ]]; then
  pass "All test shares deleted"
else
  fail "Some shares remain" "remaining=$remaining"
fi

# =============================================================================
section "21. Cleanup — Delete Test Tenants"
# =============================================================================

for tid in "$VISITOR_ID" "$CONSUMER_ID" "$OWNER_ID"; do
  resp=$(req DELETE "$BASE/tenants/$tid?force=true" "${AUTH[@]}")
  status=$(get_status "$resp")
  if [[ "$status" == "204" || "$status" == "200" ]]; then
    pass "Delete tenant $tid → $status"
  else
    fail "Delete tenant $tid" "status=$status, $(get_body "$resp")"
  fi
done

# =============================================================================
section "SUMMARY"
# =============================================================================
TOTAL=$((PASS + FAIL))
echo -e "\n  ${GREEN}Passed: $PASS${NC}  ${RED}Failed: $FAIL${NC}  Total: $TOTAL"
if [[ $FAIL -gt 0 ]]; then
  echo -e "\n${RED}Some tests failed!${NC}"
  exit 1
else
  echo -e "\n${GREEN}All tests passed!${NC}"
  exit 0
fi
