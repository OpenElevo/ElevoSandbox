#!/usr/bin/env bash
# =============================================================================
# Comprehensive integration test for namespace-share-admin API
# Tests all admin + tenant self-service endpoints
# =============================================================================
set -euo pipefail

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
fail() { FAIL=$((FAIL+1)); echo -e "  ${RED}✗ $1${NC}"; echo -e "    ${RED}$2${NC}"; }
section() { echo -e "\n${CYAN}━━━ $1 ━━━${NC}"; }

# Helper: HTTP request returning "body\nstatus_code"
req() {
  local method=$1 url=$2
  shift 2
  curl -s -w '\n%{http_code}' -X "$method" "$url" "$@"
}

get_status() { echo "$1" | tail -1; }
get_body() { echo "$1" | sed '$d'; }
jf() { echo "$1" | jq -r "$2" 2>/dev/null; }

# =============================================================================
section "1. Health Check"
# =============================================================================
resp=$(req GET "$BASE/health")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "GET /health → 200"
else
  fail "GET /health → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "2. Admin Auth — Login"
# =============================================================================
resp=$(req POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"password":"'"$ADMIN_PASSWORD"'"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  ADMIN_TOKEN=$(jf "$body" '.token')
  if [[ -n "$ADMIN_TOKEN" && "$ADMIN_TOKEN" != "null" ]]; then
    pass "POST /auth/login → 200, got JWT"
  else
    fail "POST /auth/login → 200 but no token" "$body"
    exit 1
  fi
else
  fail "POST /auth/login → $status" "$body"
  exit 1
fi

# Wrong password
resp=$(req POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"password":"wrong"}')
status=$(get_status "$resp")
if [[ "$status" == "401" ]]; then
  pass "POST /auth/login wrong password → 401"
else
  fail "POST /auth/login wrong password → $status" "$(get_body "$resp")"
fi

AUTH=(-H "Authorization: Bearer $ADMIN_TOKEN")

# =============================================================================
section "3. Dashboard Stats"
# =============================================================================
resp=$(req GET "$BASE/dashboard/stats" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "GET /dashboard/stats → 200"
else
  fail "GET /dashboard/stats → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "4. Tenant CRUD"
# =============================================================================

# Create tenant A (with initial API key)
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TenantA","description":"Test tenant A","initial_api_key":{"name":"key-a"}}')
status=$(get_status "$resp")
body=$(get_body "$resp")
TENANT_A_ID=$(jf "$body" '.tenant.id')
TENANT_A_TOKEN=$(jf "$body" '.api_key.token')
if [[ "$status" == "201" && -n "$TENANT_A_ID" && "$TENANT_A_ID" != "null" ]]; then
  pass "POST /tenants (TenantA) → 201, id=${TENANT_A_ID:0:8}..."
else
  fail "POST /tenants (TenantA) → $status" "$body"
fi

# Create tenant B (with initial API key)
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TenantB","description":"Test tenant B","initial_api_key":{"name":"key-b"}}')
status=$(get_status "$resp")
body=$(get_body "$resp")
TENANT_B_ID=$(jf "$body" '.tenant.id')
TENANT_B_TOKEN=$(jf "$body" '.api_key.token')
if [[ "$status" == "201" && -n "$TENANT_B_ID" && "$TENANT_B_ID" != "null" ]]; then
  pass "POST /tenants (TenantB) → 201, id=${TENANT_B_ID:0:8}..."
else
  fail "POST /tenants (TenantB) → $status" "$body"
fi

# Duplicate name
resp=$(req POST "$BASE/tenants" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TenantA","description":"dup"}')
status=$(get_status "$resp")
if [[ "$status" == "409" || "$status" == "500" ]]; then
  pass "POST /tenants duplicate name → $status (rejected)"
else
  fail "POST /tenants duplicate → $status" "$(get_body "$resp")"
fi

# List tenants
resp=$(req GET "$BASE/tenants" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total" -ge 2 ]]; then
  pass "GET /tenants → 200, total=$total"
else
  fail "GET /tenants → $status, total=$total" "$body"
fi

# List with pagination
resp=$(req GET "$BASE/tenants?page=1&page_size=1" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.items | length')
if [[ "$status" == "200" && "$count" == "1" ]]; then
  pass "GET /tenants?page_size=1 → count=1"
else
  fail "GET /tenants?page_size=1 → $status, count=$count" "$body"
fi

# List with search
resp=$(req GET "$BASE/tenants?search=TenantA" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.items | length')
if [[ "$status" == "200" && "$count" == "1" ]]; then
  pass "GET /tenants?search=TenantA → found 1"
else
  fail "GET /tenants?search=TenantA → $status, count=$count" "$body"
fi

# Get tenant
resp=$(req GET "$BASE/tenants/$TENANT_A_ID" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
name=$(jf "$body" '.tenant.name')
if [[ "$status" == "200" && "$name" == "TenantA" ]]; then
  pass "GET /tenants/:id → name=TenantA"
else
  fail "GET /tenants/:id → $status, name=$name" "$body"
fi

# Update tenant
resp=$(req PUT "$BASE/tenants/$TENANT_A_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TenantA-Upd","description":"updated"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
name=$(jf "$body" '.tenant.name')
if [[ "$status" == "200" && "$name" == "TenantA-Upd" ]]; then
  pass "PUT /tenants/:id → name updated"
else
  fail "PUT /tenants/:id → $status, name=$name" "$body"
fi

# Rename back
req PUT "$BASE/tenants/$TENANT_A_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TenantA"}' > /dev/null 2>&1

# Deactivate
resp=$(req POST "$BASE/tenants/$TENANT_A_ID/deactivate" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
is_active=$(jf "$body" '.tenant.is_active')
if [[ "$status" == "200" && "$is_active" == "false" ]]; then
  pass "POST deactivate → is_active=false"
else
  fail "POST deactivate → $status, is_active=$is_active" "$body"
fi

# Activate
resp=$(req POST "$BASE/tenants/$TENANT_A_ID/activate" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
is_active=$(jf "$body" '.tenant.is_active')
if [[ "$status" == "200" && "$is_active" == "true" ]]; then
  pass "POST activate → is_active=true"
else
  fail "POST activate → $status, is_active=$is_active" "$body"
fi

# =============================================================================
section "5. API Key Management"
# =============================================================================

# Create extra API key
resp=$(req POST "$BASE/tenants/$TENANT_A_ID/keys" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"extra-key"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
EXTRA_KEY_ID=$(jf "$body" '.key.id')
if [[ "$status" == "201" && -n "$EXTRA_KEY_ID" && "$EXTRA_KEY_ID" != "null" ]]; then
  pass "POST /tenants/:id/keys → 201"
else
  fail "POST /tenants/:id/keys → $status" "$body"
fi

# List keys
resp=$(req GET "$BASE/tenants/$TENANT_A_ID/keys" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.keys | length')
if [[ "$status" == "200" && "$count" -ge 2 ]]; then
  pass "GET /tenants/:id/keys → $count keys"
else
  fail "GET /tenants/:id/keys → $status, count=$count" "$body"
fi

# Revoke extra key (returns 204)
resp=$(req DELETE "$BASE/tenants/$TENANT_A_ID/keys/$EXTRA_KEY_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "DELETE /tenants/:id/keys/:kid → $status (revoked)"
else
  fail "DELETE /tenants/:id/keys/:kid → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "6. Tenant Self-Service — /me"
# =============================================================================

TENANT_AUTH_A=(-H "Authorization: Bearer $TENANT_A_TOKEN")
TENANT_AUTH_B=(-H "Authorization: Bearer $TENANT_B_TOKEN")

# GET /me as tenant
resp=$(req GET "$BASE/me" "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
me_id=$(jf "$body" '.tenant.id')
if [[ "$status" == "200" && "$me_id" == "$TENANT_A_ID" ]]; then
  pass "GET /me (TenantA) → correct id"
else
  fail "GET /me (TenantA) → $status, id=$me_id" "$body"
fi

# GET /me as admin
resp=$(req GET "$BASE/me" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
me_type=$(jf "$body" '.type')
if [[ "$status" == "200" && "$me_type" == "admin" ]]; then
  pass "GET /me (Admin) → type=admin"
else
  fail "GET /me (Admin) → $status, type=$me_type" "$body"
fi

# GET /me unauthenticated
resp=$(req GET "$BASE/me")
status=$(get_status "$resp")
if [[ "$status" == "401" ]]; then
  pass "GET /me (no auth) → 401"
else
  fail "GET /me (no auth) → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "7. Namespace File Operations (Admin)"
# =============================================================================

# List files (root, initially empty)
resp=$(req GET "$BASE/namespaces/$TENANT_A_ID/files/list?path=." "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  pass "GET /namespaces/:id/files/list → 200"
else
  fail "GET /namespaces/:id/files/list → $status" "$body"
fi

# Write file (JSON body with content field)
resp=$(req PUT "$BASE/namespaces/$TENANT_A_ID/files?path=hello.txt" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Hello from admin!"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  pass "PUT /namespaces/:id/files (hello.txt) → 200"
else
  fail "PUT /namespaces/:id/files (hello.txt) → $status" "$body"
fi

# Read file back
resp=$(req GET "$BASE/namespaces/$TENANT_A_ID/files?path=hello.txt" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "Hello from admin!" ]]; then
  pass "GET /namespaces/:id/files (hello.txt) → content matches"
else
  fail "GET /namespaces/:id/files (hello.txt) → $status" "content=$content body=$body"
fi

# Create directory
resp=$(req POST "$BASE/namespaces/$TENANT_A_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"subdir"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "POST mkdir (subdir) → 200"
else
  fail "POST mkdir → $status" "$(get_body "$resp")"
fi

# Write file in subdir
resp=$(req PUT "$BASE/namespaces/$TENANT_A_ID/files?path=subdir/nested.txt" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"nested content"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "PUT subdir/nested.txt → 200"
else
  fail "PUT subdir/nested.txt → $status" "$(get_body "$resp")"
fi

# List files (root) — should have hello.txt and subdir
resp=$(req GET "$BASE/namespaces/$TENANT_A_ID/files/list?path=." "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$count" -ge 2 ]]; then
  pass "GET files/list (root) → $count entries"
else
  fail "GET files/list → $status, count=$count" "$body"
fi

# File info
resp=$(req GET "$BASE/namespaces/$TENANT_A_ID/files/info?path=hello.txt" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
ftype=$(jf "$body" '.type')
if [[ "$status" == "200" && "$ftype" == "file" ]]; then
  pass "GET files/info (hello.txt) → type=file"
else
  fail "GET files/info → $status, type=$ftype" "$body"
fi

# Copy file
resp=$(req POST "$BASE/namespaces/$TENANT_A_ID/files/copy" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"source":"hello.txt","destination":"hello_copy.txt"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "POST files/copy → 200"
else
  fail "POST files/copy → $status" "$(get_body "$resp")"
fi

# Move file
resp=$(req POST "$BASE/namespaces/$TENANT_A_ID/files/move" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"source":"hello_copy.txt","destination":"hello_moved.txt"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "POST files/move → 200"
else
  fail "POST files/move → $status" "$(get_body "$resp")"
fi

# Delete file
resp=$(req DELETE "$BASE/namespaces/$TENANT_A_ID/files?path=hello_moved.txt" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "DELETE files (hello_moved.txt) → 200"
else
  fail "DELETE files → $status" "$(get_body "$resp")"
fi

# Path traversal rejection
resp=$(req GET "$BASE/namespaces/$TENANT_A_ID/files?path=../../../etc/passwd" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "400" || "$status" == "403" ]]; then
  pass "GET path traversal → $status (rejected)"
else
  fail "GET path traversal → $status" "$(get_body "$resp")"
fi

# TenantB cannot access TenantA's namespace
resp=$(req GET "$BASE/namespaces/$TENANT_A_ID/files/list?path=." "${TENANT_AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "TenantB → TenantA namespace → 403"
else
  fail "TenantB → TenantA namespace → $status (expected 403)" "$(get_body "$resp")"
fi

# =============================================================================
section "8. /me File Operations (Tenant)"
# =============================================================================

# Write via /me/files/*path (JSON body)
resp=$(req PUT "$BASE/me/files/my-file.txt" "${TENANT_AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Written by tenant A"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "PUT /me/files/my-file.txt → 200"
else
  fail "PUT /me/files/my-file.txt → $status" "$(get_body "$resp")"
fi

# Read via /me/files/*path
resp=$(req GET "$BASE/me/files/my-file.txt" "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "Written by tenant A" ]]; then
  pass "GET /me/files/my-file.txt → content matches"
else
  fail "GET /me/files/my-file.txt → $status" "content=$content"
fi

# List files via /me/files (needs ?path=)
resp=$(req GET "$BASE/me/files?path=." "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.files | length')
if [[ "$status" == "200" && "$count" -ge 1 ]]; then
  pass "GET /me/files?path=. → $count files"
else
  fail "GET /me/files?path=. → $status, count=$count" "$body"
fi

# Create file via POST /me/files/*path
resp=$(req POST "$BASE/me/files/created.txt" "${TENANT_AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"directory":false,"content":"created via POST"}')
status=$(get_status "$resp")
if [[ "$status" == "201" ]]; then
  pass "POST /me/files/created.txt → 201"
else
  fail "POST /me/files/created.txt → $status" "$(get_body "$resp")"
fi

# Delete via /me/files/*path
resp=$(req DELETE "$BASE/me/files/created.txt" "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "DELETE /me/files/created.txt → 200"
else
  fail "DELETE /me/files/created.txt → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "9. Share Management"
# =============================================================================

# Prepare share source directory with a file
req POST "$BASE/namespaces/$TENANT_A_ID/files/mkdir" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"path":"shared-data"}' > /dev/null 2>&1
req PUT "$BASE/namespaces/$TENANT_A_ID/files?path=shared-data/readme.txt" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"This is shared data"}' > /dev/null 2>&1

# Create private share (as tenant A)
resp=$(req POST "$BASE/shares" "${TENANT_AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TestShare","source_path":"shared-data","description":"A test share","visibility":"private"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
SHARE_ID=$(jf "$body" '.share.id')
if [[ "$status" == "201" && -n "$SHARE_ID" && "$SHARE_ID" != "null" ]]; then
  pass "POST /shares (private) → 201, id=${SHARE_ID:0:8}..."
else
  fail "POST /shares (private) → $status" "$body"
fi

# Create public share (as tenant A, source=root)
resp=$(req POST "$BASE/shares" "${TENANT_AUTH_A[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"PublicShare","source_path":".","description":"public share","visibility":"public"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
PUB_SHARE_ID=$(jf "$body" '.share.id')
if [[ "$status" == "201" && -n "$PUB_SHARE_ID" && "$PUB_SHARE_ID" != "null" ]]; then
  pass "POST /shares (public) → 201"
else
  fail "POST /shares (public) → $status" "$body"
fi

# Get share
resp=$(req GET "$BASE/shares/$SHARE_ID" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
sname=$(jf "$body" '.share.name')
if [[ "$status" == "200" && "$sname" == "TestShare" ]]; then
  pass "GET /shares/:id → name=TestShare"
else
  fail "GET /shares/:id → $status, name=$sname" "$body"
fi

# List shares (admin) — all
resp=$(req GET "$BASE/shares" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total" -ge 2 ]]; then
  pass "GET /shares (admin) → total=$total"
else
  fail "GET /shares (admin) → $status, total=$total" "$body"
fi

# List shares (TenantA — owner)
resp=$(req GET "$BASE/shares" "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total_a=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total_a" -ge 2 ]]; then
  pass "GET /shares (TenantA) → total=$total_a"
else
  fail "GET /shares (TenantA) → $status, total=$total_a" "$body"
fi

# List shares (TenantB) — only public visible
resp=$(req GET "$BASE/shares" "${TENANT_AUTH_B[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total_b=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total_b" -ge 1 ]]; then
  pass "GET /shares (TenantB, no perm) → total=$total_b (sees public)"
else
  fail "GET /shares (TenantB) → $status, total=$total_b" "$body"
fi

# Update share
resp=$(req PUT "$BASE/shares/$SHARE_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TestShare-Updated","description":"updated"}')
status=$(get_status "$resp")
body=$(get_body "$resp")
new_name=$(jf "$body" '.share.name')
if [[ "$status" == "200" && "$new_name" == "TestShare-Updated" ]]; then
  pass "PUT /shares/:id → name updated"
else
  fail "PUT /shares/:id → $status, name=$new_name" "$body"
fi

# Rename back
req PUT "$BASE/shares/$SHARE_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"name":"TestShare"}' > /dev/null 2>&1

# =============================================================================
section "10. Share Permission Management"
# =============================================================================

# Grant read to TenantB
resp=$(req POST "$BASE/shares/$SHARE_ID/permissions" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"'"$TENANT_B_ID"'","permission":"read"}')
status=$(get_status "$resp")
if [[ "$status" == "201" ]]; then
  pass "POST grant read → TenantB → 201"
else
  fail "POST grant → $status" "$(get_body "$resp")"
fi

# List permissions
resp=$(req GET "$BASE/shares/$SHARE_ID/permissions" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
pcount=$(jf "$body" '.permissions | length')
if [[ "$status" == "200" && "$pcount" -ge 1 ]]; then
  pass "GET /shares/:id/permissions → $pcount entries"
else
  fail "GET permissions → $status, count=$pcount" "$body"
fi

# Update permission to write
resp=$(req PUT "$BASE/shares/$SHARE_ID/permissions/$TENANT_B_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"permission":"write"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "PUT permission → write → 200"
else
  fail "PUT permission → $status" "$(get_body "$resp")"
fi

# List tenant B's permissions
resp=$(req GET "$BASE/tenants/$TENANT_B_ID/permissions" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
pcount2=$(jf "$body" '.permissions | length')
if [[ "$status" == "200" && "$pcount2" -ge 1 ]]; then
  pass "GET /tenants/:id/permissions → $pcount2 entries"
else
  fail "GET tenant permissions → $status" "$body"
fi

# TenantB should now see private share
resp=$(req GET "$BASE/shares" "${TENANT_AUTH_B[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
total_b2=$(jf "$body" '.total')
if [[ "$status" == "200" && "$total_b2" -ge 2 ]]; then
  pass "GET /shares (TenantB after grant) → total=$total_b2"
else
  fail "GET /shares (TenantB after grant) → $status, total=$total_b2" "$body"
fi

# Revoke permission
resp=$(req DELETE "$BASE/shares/$SHARE_ID/permissions/$TENANT_B_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "DELETE permission → $status (revoked)"
else
  fail "DELETE permission → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "11. Share File Operations"
# =============================================================================

# Re-grant read for file access tests
req POST "$BASE/shares/$SHARE_ID/permissions" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"tenant_id":"'"$TENANT_B_ID"'","permission":"read"}' > /dev/null 2>&1

# List share files
resp=$(req GET "$BASE/shares/$SHARE_ID/files/list?path=." "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  fcount=$(jf "$body" '.files | length')
  pass "GET share files/list → $fcount files"
else
  fail "GET share files/list → $status" "$body"
fi

# Read share file
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "This is shared data" ]]; then
  pass "GET share file → content matches"
else
  fail "GET share file → $status" "content=$content"
fi

# TenantB reads share file (has read perm)
resp=$(req GET "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${TENANT_AUTH_B[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
content=$(jf "$body" '.content')
if [[ "$status" == "200" && "$content" == "This is shared data" ]]; then
  pass "TenantB reads share file → matches"
else
  fail "TenantB reads share file → $status" "content=$content"
fi

# TenantB tries write (has read only) → should be rejected
resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=readme.txt" "${TENANT_AUTH_B[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"unauthorized"}')
status=$(get_status "$resp")
if [[ "$status" == "403" ]]; then
  pass "TenantB write (read-only) → 403"
else
  fail "TenantB write (read-only) → $status (expected 403)" "$(get_body "$resp")"
fi

# Upgrade to write, then write should work
req PUT "$BASE/shares/$SHARE_ID/permissions/$TENANT_B_ID" "${AUTH[@]}" \
  -H "Content-Type: application/json" \
  -d '{"permission":"write"}' > /dev/null 2>&1

resp=$(req PUT "$BASE/shares/$SHARE_ID/files?path=from-b.txt" "${TENANT_AUTH_B[@]}" \
  -H "Content-Type: application/json" \
  -d '{"content":"Written by tenant B"}')
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "TenantB write (with write perm) → 200"
else
  fail "TenantB write (write perm) → $status" "$(get_body "$resp")"
fi

# Delete share file
resp=$(req DELETE "$BASE/shares/$SHARE_ID/files?path=from-b.txt" "${TENANT_AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "DELETE share file → 200"
else
  fail "DELETE share file → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "12. /me — Shares & Sandboxes"
# =============================================================================

# List my shares (TenantA owns shares)
resp=$(req GET "$BASE/me/shares" "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  total=$(jf "$body" '.total')
  pass "GET /me/shares (TenantA) → total=$total"
else
  fail "GET /me/shares → $status" "$body"
fi

# List my accessible shares (TenantB)
resp=$(req GET "$BASE/me/accessible-shares" "${TENANT_AUTH_B[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "GET /me/accessible-shares (TenantB) → 200"
else
  fail "GET /me/accessible-shares → $status" "$(get_body "$resp")"
fi

# List my sandboxes
resp=$(req GET "$BASE/me/sandboxes" "${TENANT_AUTH_A[@]}")
status=$(get_status "$resp")
if [[ "$status" == "200" ]]; then
  pass "GET /me/sandboxes → 200"
else
  fail "GET /me/sandboxes → $status" "$(get_body "$resp")"
fi

# =============================================================================
section "13. Audit Logs"
# =============================================================================

resp=$(req GET "$BASE/audit-logs" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  total=$(jf "$body" '.total')
  pass "GET /audit-logs → total=$total"
else
  fail "GET /audit-logs → $status" "$body"
fi

# Filter by action
resp=$(req GET "$BASE/audit-logs?action=tenant.create" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
if [[ "$status" == "200" ]]; then
  total=$(jf "$body" '.total')
  pass "GET /audit-logs?action=tenant.create → total=$total"
else
  fail "GET /audit-logs?action=tenant.create → $status" "$body"
fi

# Pagination
resp=$(req GET "$BASE/audit-logs?page=1&page_size=2" "${AUTH[@]}")
status=$(get_status "$resp")
body=$(get_body "$resp")
count=$(jf "$body" '.items | length')
if [[ "$status" == "200" ]]; then
  pass "GET /audit-logs?page_size=2 → count=$count"
else
  fail "GET /audit-logs?page_size=2 → $status" "$body"
fi

# =============================================================================
section "14. Cleanup"
# =============================================================================

# Delete shares first
resp=$(req DELETE "$BASE/shares/$PUB_SHARE_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "DELETE public share → $status"
else
  fail "DELETE public share → $status" "$(get_body "$resp")"
fi

resp=$(req DELETE "$BASE/shares/$SHARE_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "DELETE private share → $status"
else
  fail "DELETE private share → $status" "$(get_body "$resp")"
fi

# Delete tenants (force=true to bypass active API key check)
resp=$(req DELETE "$BASE/tenants/$TENANT_B_ID?force=true" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "DELETE TenantB → $status"
else
  fail "DELETE TenantB → $status" "$(get_body "$resp")"
fi

resp=$(req DELETE "$BASE/tenants/$TENANT_A_ID?force=true" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "204" || "$status" == "200" ]]; then
  pass "DELETE TenantA → $status"
else
  fail "DELETE TenantA → $status" "$(get_body "$resp")"
fi

# Verify deletion
resp=$(req GET "$BASE/tenants/$TENANT_A_ID" "${AUTH[@]}")
status=$(get_status "$resp")
if [[ "$status" == "404" ]]; then
  pass "GET deleted tenant → 404"
else
  fail "GET deleted tenant → $status" "$(get_body "$resp")"
fi

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
