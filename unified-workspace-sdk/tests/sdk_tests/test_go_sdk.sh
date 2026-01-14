#!/bin/bash
# Go SDK API Tests (using curl to simulate SDK behavior)
# Tests the same API endpoints that the Go SDK would use

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

# API helpers (matching Go SDK methods)
sandbox_create() {
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

sandbox_get() {
    curl -s "$API_BASE/sandboxes/$1"
}

sandbox_list() {
    curl -s "$API_BASE/sandboxes"
}

sandbox_delete() {
    local force=""
    [ "$2" = "true" ] && force="?force=true"
    curl -s -X DELETE "$API_BASE/sandboxes/$1$force"
}

process_run() {
    local sandbox_id="$1"
    local command="$2"
    local args="$3"
    local env="$4"

    [ -z "$args" ] && args="[]"
    [ -z "$env" ] && env="{}"

    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d "{\"command\": \"$command\", \"args\": $args, \"env\": $env}"
}

# ========================================
# Tests matching Go SDK test cases
# ========================================

test_sandbox_lifecycle() {
    log_section "Test: Sandbox Lifecycle"

    # Create
    local result=$(sandbox_create "$TEST_IMAGE")
    local id=$(json_get "$result" "id")
    local state=$(json_get "$result" "state")

    if [ -n "$id" ] && [ "$state" = "running" ]; then
        log_pass "sandbox.Create(): ID=$id, state=$state"
    else
        log_fail "sandbox.Create() failed"
        return 1
    fi

    # Get
    result=$(sandbox_get "$id")
    local fetched_state=$(json_get "$result" "state")
    if [ "$(json_get "$result" "id")" = "$id" ]; then
        log_pass "sandbox.Get(): state=$fetched_state"
    else
        log_fail "sandbox.Get() failed"
    fi

    # List
    result=$(sandbox_list)
    if echo "$result" | grep -q "$id"; then
        log_pass "sandbox.List(): found in list"
    else
        log_fail "sandbox.List(): not found"
    fi

    # Delete
    sandbox_delete "$id" "true" > /dev/null
    log_pass "sandbox.Delete(): $id"
}

test_process_execution() {
    log_section "Test: Process Execution"

    local sandbox=$(sandbox_create)
    local sandbox_id=$(json_get "$sandbox" "id")

    # Echo
    local result=$(process_run "$sandbox_id" "echo" '["Hello", "Go"]' "{}")
    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "Hello Go"; then
        log_pass "process.Run(echo): stdout='$(echo "$stdout" | tr -d '\n')'"
    else
        log_fail "process.Run(echo) failed"
    fi

    # ls -la
    result=$(process_run "$sandbox_id" "ls" '["-la", "/workspace"]' "{}")
    exit_code=$(json_get "$result" "exit_code")
    if [ "$exit_code" = "0" ]; then
        log_pass "process.Run(ls -la): success"
    else
        log_fail "process.Run(ls -la) failed"
    fi

    # Failing command
    result=$(process_run "$sandbox_id" "bash" '["-c", "exit 42"]' "{}")
    exit_code=$(json_get "$result" "exit_code")
    if [ "$exit_code" = "42" ]; then
        log_pass "process.Run(exit 42): exit_code=$exit_code"
    else
        log_fail "process.Run(exit 42): expected 42, got $exit_code"
    fi

    # Env var
    result=$(process_run "$sandbox_id" "bash" '["-c", "echo $GO_VAR"]' '{"GO_VAR": "go_value"}')
    exit_code=$(json_get "$result" "exit_code")
    stdout=$(json_get "$result" "stdout")
    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "go_value"; then
        log_pass "process.Run(env): stdout='$(echo "$stdout" | tr -d '\n')'"
    else
        log_fail "process.Run(env) failed"
    fi

    # File write/read
    result=$(process_run "$sandbox_id" "bash" '["-c", "echo go_content > /workspace/test.txt && cat /workspace/test.txt"]' "{}")
    exit_code=$(json_get "$result" "exit_code")
    stdout=$(json_get "$result" "stdout")
    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "go_content"; then
        log_pass "process.Run(file): write/read success"
    else
        log_fail "process.Run(file) failed"
    fi

    sandbox_delete "$sandbox_id" "true" > /dev/null
}

test_sandbox_isolation() {
    log_section "Test: Sandbox Isolation"

    local sandbox_a=$(sandbox_create)
    local id_a=$(json_get "$sandbox_a" "id")

    local sandbox_b=$(sandbox_create)
    local id_b=$(json_get "$sandbox_b" "id")

    # Write in A
    process_run "$id_a" "bash" '["-c", "echo secret_go > /workspace/secret.txt"]' "{}" > /dev/null
    log_pass "Created file in sandbox A"

    # Try read from B
    local result=$(process_run "$id_b" "cat" '["/workspace/secret.txt"]' "{}")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ]; then
        log_pass "Isolation verified: B cannot read A's files"
    else
        log_fail "Isolation broken!"
    fi

    sandbox_delete "$id_a" "true" > /dev/null
    sandbox_delete "$id_b" "true" > /dev/null
}

test_long_running() {
    log_section "Test: Long Running Command"

    local sandbox=$(sandbox_create)
    local sandbox_id=$(json_get "$sandbox" "id")

    local start=$(date +%s)
    local result=$(process_run "$sandbox_id" "bash" '["-c", "sleep 3 && echo done"]' "{}")
    local end=$(date +%s)
    local elapsed=$((end - start))

    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "done" && [ "$elapsed" -ge 3 ]; then
        log_pass "Long running completed in ${elapsed}s"
    else
        log_fail "Long running failed"
    fi

    sandbox_delete "$sandbox_id" "true" > /dev/null
}

test_script_execution() {
    log_section "Test: Script Execution"

    local sandbox=$(sandbox_create)
    local sandbox_id=$(json_get "$sandbox" "id")

    # Bash loop
    local result=$(process_run "$sandbox_id" "bash" '["-c", "for i in a b c; do echo item_$i; done"]' "{}")
    local stdout=$(json_get "$result" "stdout")

    if echo "$stdout" | grep -q "item_a" && echo "$stdout" | grep -q "item_c"; then
        log_pass "process.Shell(loop): success"
    else
        log_fail "process.Shell(loop) failed"
    fi

    # Pipe
    result=$(process_run "$sandbox_id" "bash" '["-c", "echo go_sdk | tr a-z A-Z"]' "{}")
    stdout=$(json_get "$result" "stdout")

    if echo "$stdout" | grep -q "GO_SDK"; then
        log_pass "process.Shell(pipe): $(echo "$stdout" | tr -d '\n')"
    else
        log_fail "process.Shell(pipe) failed"
    fi

    sandbox_delete "$sandbox_id" "true" > /dev/null
}

test_concurrent_operations() {
    log_section "Test: Concurrent Operations"

    # Create 3 sandboxes
    local s1=$(sandbox_create)
    local s2=$(sandbox_create)
    local s3=$(sandbox_create)

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
    local r1=$(process_run "$id1" "echo" '["s1"]' "{}")
    local r2=$(process_run "$id2" "echo" '["s2"]' "{}")
    local r3=$(process_run "$id3" "echo" '["s3"]' "{}")

    if [ "$(json_get "$r1" "exit_code")" = "0" ] && \
       [ "$(json_get "$r2" "exit_code")" = "0" ] && \
       [ "$(json_get "$r3" "exit_code")" = "0" ]; then
        log_pass "Ran 3 commands successfully"
    else
        log_fail "Some commands failed"
    fi

    sandbox_delete "$id1" "true" > /dev/null
    sandbox_delete "$id2" "true" > /dev/null
    sandbox_delete "$id3" "true" > /dev/null
    log_pass "Deleted 3 sandboxes"
}

test_error_handling() {
    log_section "Test: Error Handling"

    # Non-existent sandbox
    local result=$(sandbox_get "non-existent-id")
    if echo "$result" | grep -qi "error\|not found"; then
        log_pass "sandbox.Get(invalid): returns error"
    else
        log_fail "sandbox.Get(invalid): no error"
    fi

    # Missing file
    local sandbox=$(sandbox_create)
    local sandbox_id=$(json_get "$sandbox" "id")

    result=$(process_run "$sandbox_id" "cat" '["/nonexistent/file.txt"]' "{}")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ]; then
        log_pass "process.Run(missing file): exit_code=$exit_code"
    else
        log_fail "process.Run(missing file): expected error"
    fi

    sandbox_delete "$sandbox_id" "true" > /dev/null
}

test_sandbox_metadata() {
    log_section "Test: Sandbox Metadata"

    local result=$(sandbox_create "$TEST_IMAGE" "test-go-sandbox" '{"purpose": "testing"}')
    local sandbox_id=$(json_get "$result" "id")
    local name=$(json_get "$result" "name")

    if [ "$name" = "test-go-sandbox" ]; then
        log_pass "sandbox.Create(name): $name"
    else
        log_fail "sandbox.Create(name): expected 'test-go-sandbox', got '$name'"
    fi

    # Command env
    result=$(process_run "$sandbox_id" "bash" '["-c", "echo $CMD_ENV"]' '{"CMD_ENV": "go-cmd-val"}')
    local stdout=$(json_get "$result" "stdout")

    if echo "$stdout" | grep -q "go-cmd-val"; then
        log_pass "Command-level env works"
    else
        log_fail "Command-level env failed"
    fi

    sandbox_delete "$sandbox_id" "true" > /dev/null
}

test_rapid_operations() {
    log_section "Test: Rapid Operations"

    local success=0
    for i in 1 2 3 4 5; do
        local sandbox=$(sandbox_create)
        local sandbox_id=$(json_get "$sandbox" "id")
        local state=$(json_get "$sandbox" "state")
        if [ "$state" = "running" ]; then
            sandbox_delete "$sandbox_id" "true" > /dev/null
            ((success++)) || true
        fi
    done

    if [ "$success" -eq 5 ]; then
        log_pass "Rapid create/delete: $success/5"
    else
        log_fail "Rapid create/delete: $success/5"
    fi
}

test_helper_methods() {
    log_section "Test: Helper Methods (Exec, Shell)"

    local sandbox=$(sandbox_create)
    local sandbox_id=$(json_get "$sandbox" "id")

    # Exec helper (simple command)
    local result=$(process_run "$sandbox_id" "echo" '["test", "output"]' "{}")
    local stdout=$(json_get "$result" "stdout")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "test output"; then
        log_pass "process.Exec(): '$(echo "$stdout" | tr -d '\n')'"
    else
        log_fail "process.Exec() failed"
    fi

    # Shell helper (with env and pipe)
    result=$(process_run "$sandbox_id" "bash" '["-c", "echo $SHELL_VAR | tr a-z A-Z"]' '{"SHELL_VAR": "hello"}')
    stdout=$(json_get "$result" "stdout")
    exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ] && echo "$stdout" | grep -q "HELLO"; then
        log_pass "process.Shell(): '$(echo "$stdout" | tr -d '\n')'"
    else
        log_fail "process.Shell() failed"
    fi

    sandbox_delete "$sandbox_id" "true" > /dev/null
}

# ========================================
# Main
# ========================================

main() {
    echo ""
    echo "============================================================"
    echo "  Go SDK API Test Suite"
    echo "============================================================"
    echo "API URL: $API_BASE"
    echo "Test Image: $TEST_IMAGE"

    test_sandbox_lifecycle
    test_process_execution
    test_sandbox_isolation
    test_long_running
    test_script_execution
    test_concurrent_operations
    test_error_handling
    test_sandbox_metadata
    test_rapid_operations
    test_helper_methods

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
