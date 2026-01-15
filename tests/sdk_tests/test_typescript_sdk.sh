#!/bin/bash
# TypeScript SDK Equivalent Tests (using curl)
# Tests the same scenarios as the TypeScript SDK would

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Configuration
API_BASE="${WORKSPACE_API_URL:-http://localhost:8080}/api/v1"
TEST_IMAGE="${TEST_IMAGE:-workspace-test:latest}"

# Counters
PASSED=0
FAILED=0

log_pass() {
    echo -e "  ${GREEN}[PASS]${NC} $1"
    ((PASSED++)) || true
}

log_fail() {
    echo -e "  ${RED}[FAIL]${NC} $1"
    ((FAILED++)) || true
}

log_section() {
    echo ""
    echo -e "${YELLOW}==================================================${NC}"
    echo -e "${YELLOW}$1${NC}"
    echo -e "${YELLOW}==================================================${NC}"
}

# JSON helper using Python
json_get() {
    local json="$1"
    local field="$2"
    echo "$json" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    v = d.get('$field', '')
    if isinstance(v, bool):
        print(str(v).lower())
    else:
        print(v)
except:
    print('')
" 2>/dev/null
}

# API helpers
create_sandbox() {
    local template="${1:-$TEST_IMAGE}"
    local name="$2"
    local metadata="$3"

    local data="{\"template\": \"$template\""
    [ -n "$name" ] && data="$data, \"name\": \"$name\""
    [ -n "$metadata" ] && data="$data, \"metadata\": $metadata"
    data="$data}"

    curl -s -X POST "$API_BASE/sandboxes" \
        -H "Content-Type: application/json" \
        -d "$data"
}

get_sandbox() {
    curl -s "$API_BASE/sandboxes/$1"
}

list_sandboxes() {
    curl -s "$API_BASE/sandboxes"
}

delete_sandbox() {
    curl -s -X DELETE "$API_BASE/sandboxes/$1?force=true"
}

run_command() {
    local sandbox_id="$1"
    local command="$2"
    shift 2
    local args="[]"
    local env="{}"

    if [ $# -gt 0 ]; then
        # Build JSON array properly
        local arg_list=""
        for arg in "$@"; do
            [ -n "$arg_list" ] && arg_list="$arg_list,"
            arg_list="$arg_list\"$arg\""
        done
        args="[$arg_list]"
    fi

    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d "{\"command\": \"$command\", \"args\": $args, \"env\": $env}"
}

run_command_with_env() {
    local sandbox_id="$1"
    local command="$2"
    local args="$3"
    local env="$4"

    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d "{\"command\": \"$command\", \"args\": $args, \"env\": $env}"
}

# ========================================
# Tests
# ========================================

test_1_sandbox_lifecycle() {
    log_section "Test 1: Sandbox Lifecycle"

    # Create
    local result=$(create_sandbox "$TEST_IMAGE")
    local id=$(json_get "$result" "id")
    local state=$(json_get "$result" "state")

    if [ -n "$id" ] && [ "$state" = "running" ]; then
        log_pass "Created sandbox: $id"
    else
        log_fail "Failed to create sandbox: $result"
        return 1
    fi

    # Get
    result=$(get_sandbox "$id")
    local fetched_id=$(json_get "$result" "id")
    if [ "$fetched_id" = "$id" ]; then
        log_pass "Got sandbox info: state=$(json_get "$result" "state")"
    else
        log_fail "Failed to get sandbox"
    fi

    # List
    result=$(list_sandboxes)
    if echo "$result" | grep -q "$id"; then
        log_pass "Listed sandboxes: found in list"
    else
        log_fail "Sandbox not in list"
    fi

    # Delete
    delete_sandbox "$id" > /dev/null
    log_pass "Deleted sandbox: $id"
}

test_2_process_execution() {
    log_section "Test 2: Process Execution"

    local sandbox=$(create_sandbox)
    local sandbox_id=$(json_get "$sandbox" "id")

    # Echo
    local result=$(run_command "$sandbox_id" "echo" "Hello" "TypeScript")
    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "Hello TypeScript"; then
        log_pass "Echo command: stdout='$(echo "$stdout" | tr -d '\n')'"
    else
        log_fail "Echo failed: exit_code=$exit_code"
    fi

    # ls -la
    result=$(run_command "$sandbox_id" "ls" "-la" "/workspace")
    exit_code=$(json_get "$result" "exit_code")
    if [ "$exit_code" = "0" ]; then
        log_pass "ls -la command executed successfully"
    else
        log_fail "ls failed: exit_code=$exit_code"
    fi

    # Failing command
    result=$(run_command_with_env "$sandbox_id" "bash" '[ "-c", "exit 99"]' "{}")
    exit_code=$(json_get "$result" "exit_code")
    if [ "$exit_code" = "99" ]; then
        log_pass "Failing command returned correct exit code: $exit_code"
    else
        log_fail "Expected exit code 99, got $exit_code"
    fi

    # Env var
    result=$(run_command_with_env "$sandbox_id" "bash" '["-c", "echo $TS_VAR"]' '{"TS_VAR": "typescript_value"}')
    exit_code=$(json_get "$result" "exit_code")
    stdout=$(json_get "$result" "stdout")
    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "typescript_value"; then
        log_pass "Env var command: stdout='$(echo "$stdout" | tr -d '\n')'"
    else
        log_fail "Env var failed"
    fi

    # File write/read
    result=$(run_command_with_env "$sandbox_id" "bash" '["-c", "echo ts_content > /workspace/test.txt && cat /workspace/test.txt"]' "{}")
    exit_code=$(json_get "$result" "exit_code")
    stdout=$(json_get "$result" "stdout")
    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "ts_content"; then
        log_pass "File write/read successful"
    else
        log_fail "File write/read failed"
    fi

    delete_sandbox "$sandbox_id" > /dev/null
}

test_3_sandbox_isolation() {
    log_section "Test 3: Multiple Sandboxes Isolation"

    local sandbox_a=$(create_sandbox)
    local id_a=$(json_get "$sandbox_a" "id")

    local sandbox_b=$(create_sandbox)
    local id_b=$(json_get "$sandbox_b" "id")

    # Write file in A
    run_command_with_env "$id_a" "bash" '["-c", "echo secret_ts > /workspace/secret.txt"]' "{}" > /dev/null
    log_pass "Created file in sandbox A: $id_a"

    # Try read from B
    local result=$(run_command "$id_b" "cat" "/workspace/secret.txt")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ]; then
        log_pass "Sandbox isolation verified: B cannot read A's files"
    else
        log_fail "Isolation broken: B can read A's files!"
    fi

    delete_sandbox "$id_a" > /dev/null
    delete_sandbox "$id_b" > /dev/null
}

test_4_long_running() {
    log_section "Test 4: Long Running Command"

    local sandbox=$(create_sandbox)
    local sandbox_id=$(json_get "$sandbox" "id")

    local start=$(date +%s)
    local result=$(run_command_with_env "$sandbox_id" "bash" '["-c", "sleep 3 && echo complete"]' "{}")
    local end=$(date +%s)
    local elapsed=$((end - start))

    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "complete" && [ "$elapsed" -ge 3 ]; then
        log_pass "Long running command completed in ${elapsed}s"
    else
        log_fail "Long running command failed"
    fi

    delete_sandbox "$sandbox_id" > /dev/null
}

test_5_script_execution() {
    log_section "Test 5: Script Execution"

    local sandbox=$(create_sandbox)
    local sandbox_id=$(json_get "$sandbox" "id")

    # Bash loop
    local result=$(run_command_with_env "$sandbox_id" "bash" '["-c", "for i in a b c; do echo item_$i; done"]' "{}")
    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "item_a" && echo "$stdout" | grep -q "item_c"; then
        log_pass "Bash script executed with loop output"
    else
        log_fail "Bash script failed"
    fi

    # Pipe
    result=$(run_command_with_env "$sandbox_id" "bash" '["-c", "echo typescript sdk | tr a-z A-Z"]' "{}")
    exit_code=$(json_get "$result" "exit_code")
    stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "TYPESCRIPT SDK"; then
        log_pass "Pipe command success: $(echo "$stdout" | tr -d '\n')"
    else
        log_fail "Pipe command failed"
    fi

    delete_sandbox "$sandbox_id" > /dev/null
}

test_6_concurrent_operations() {
    log_section "Test 6: Concurrent Operations"

    # Create 3 sandboxes
    local start=$(date +%s.%N)
    local s1=$(create_sandbox) &
    local s2=$(create_sandbox) &
    local s3=$(create_sandbox) &
    wait
    s1=$(create_sandbox)
    s2=$(create_sandbox)
    s3=$(create_sandbox)
    local end=$(date +%s.%N)

    local id1=$(json_get "$s1" "id")
    local id2=$(json_get "$s2" "id")
    local id3=$(json_get "$s3" "id")

    if [ -n "$id1" ] && [ -n "$id2" ] && [ -n "$id3" ]; then
        log_pass "Created 3 sandboxes"
    else
        log_fail "Failed to create sandboxes"
        return 1
    fi

    # Run commands
    local r1=$(run_command "$id1" "echo" "s1") &
    local r2=$(run_command "$id2" "echo" "s2") &
    local r3=$(run_command "$id3" "echo" "s3") &
    wait

    r1=$(run_command "$id1" "echo" "s1")
    r2=$(run_command "$id2" "echo" "s2")
    r3=$(run_command "$id3" "echo" "s3")

    local e1=$(json_get "$r1" "exit_code")
    local e2=$(json_get "$r2" "exit_code")
    local e3=$(json_get "$r3" "exit_code")

    if [ "$e1" = "0" ] && [ "$e2" = "0" ] && [ "$e3" = "0" ]; then
        log_pass "Ran 3 commands successfully"
    else
        log_fail "Some commands failed"
    fi

    delete_sandbox "$id1" > /dev/null
    delete_sandbox "$id2" > /dev/null
    delete_sandbox "$id3" > /dev/null
    log_pass "Deleted 3 sandboxes"
}

test_7_error_handling() {
    log_section "Test 7: Error Handling"

    # Non-existent sandbox
    local result=$(curl -s "$API_BASE/sandboxes/non-existent-id")
    if echo "$result" | grep -qi "error\|not found"; then
        log_pass "Correct error for non-existent sandbox"
    else
        log_fail "No proper error for non-existent sandbox"
    fi

    # Command with missing file
    local sandbox=$(create_sandbox)
    local sandbox_id=$(json_get "$sandbox" "id")

    result=$(run_command "$sandbox_id" "cat" "/nonexistent/file.txt")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ]; then
        log_pass "Correct error for missing file: exit_code=$exit_code"
    else
        log_fail "Expected non-zero exit code"
    fi

    delete_sandbox "$sandbox_id" > /dev/null
}

test_8_sandbox_metadata() {
    log_section "Test 8: Sandbox Metadata"

    local result=$(create_sandbox "$TEST_IMAGE" "test-sandbox-ts" '{"purpose": "testing"}')
    local sandbox_id=$(json_get "$result" "id")
    local name=$(json_get "$result" "name")

    if [ "$name" = "test-sandbox-ts" ]; then
        log_pass "Sandbox created with custom name: $name"
    else
        log_fail "Name not set correctly: $name"
    fi

    # Command-level env
    result=$(run_command_with_env "$sandbox_id" "bash" '["-c", "echo $CMD_ENV"]' '{"CMD_ENV": "cmd-value"}')
    local stdout=$(json_get "$result" "stdout")

    if echo "$stdout" | grep -q "cmd-value"; then
        log_pass "Command-level env works correctly"
    else
        log_fail "Command env failed"
    fi

    delete_sandbox "$sandbox_id" > /dev/null
}

test_9_rapid_operations() {
    log_section "Test 9: Rapid Sandbox Operations"

    local success=0
    for i in 1 2 3 4 5; do
        local sandbox=$(create_sandbox)
        local sandbox_id=$(json_get "$sandbox" "id")
        local state=$(json_get "$sandbox" "state")
        if [ "$state" = "running" ]; then
            delete_sandbox "$sandbox_id" > /dev/null
            ((success++)) || true
        fi
    done

    if [ "$success" -eq 5 ]; then
        log_pass "Rapid create/delete test passed ($success/5)"
    else
        log_fail "Rapid create/delete test failed ($success/5)"
    fi
}

# ========================================
# Main
# ========================================

main() {
    echo ""
    echo "============================================================"
    echo "  TypeScript SDK Equivalent Test Suite (curl-based)"
    echo "============================================================"
    echo "API URL: $API_BASE"
    echo "Test Image: $TEST_IMAGE"

    test_1_sandbox_lifecycle
    test_2_process_execution
    test_3_sandbox_isolation
    test_4_long_running
    test_5_script_execution
    test_6_concurrent_operations
    test_7_error_handling
    test_8_sandbox_metadata
    test_9_rapid_operations

    echo ""
    echo "============================================================"
    echo "                 TEST SUMMARY"
    echo "============================================================"
    echo -e "  ${GREEN}PASSED:${NC}  $PASSED"
    echo -e "  ${RED}FAILED:${NC}  $FAILED"
    echo "============================================================"
    echo ""

    [ "$FAILED" -eq 0 ] && exit 0 || exit 1
}

main "$@"
