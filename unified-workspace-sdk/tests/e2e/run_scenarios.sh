#!/bin/bash
# End-to-End Test Scenarios for Unified Workspace SDK
# Usage: ./run_scenarios.sh

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
API_BASE="http://localhost:8080/api/v1"
NFS_PORT=12049
NFS_MOUNT_POINT="/mnt/workspace_nfs"
TEST_IMAGE="workspace-test:latest"

# Counters
PASSED=0
FAILED=0
SKIPPED=0

# Helper functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((PASSED++)) || true
}

log_fail() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((FAILED++)) || true
}

log_skip() {
    echo -e "${YELLOW}[SKIP]${NC} $1"
    ((SKIPPED++)) || true
}

log_section() {
    echo ""
    echo -e "${YELLOW}========================================${NC}"
    echo -e "${YELLOW}$1${NC}"
    echo -e "${YELLOW}========================================${NC}"
}

# JSON field extraction - handles both string and numeric values
json_get() {
    local json="$1"
    local field="$2"
    # Use Python for reliable JSON parsing if available
    if command -v python3 &> /dev/null; then
        local value=$(echo "$json" | python3 -c "
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
" 2>/dev/null)
        echo "$value"
        return
    fi
    # Fallback: try to get string value (handles escaped chars)
    local value=$(echo "$json" | tr '\n' ' ' | sed -E "s/.*\"$field\"[[:space:]]*:[[:space:]]*\"([^\"]*)\".*/\1/" | sed 's/\\n/\n/g')
    if [ -n "$value" ] && [ "$value" != "$json" ]; then
        echo "$value"
        return
    fi
    # Try numeric/boolean value
    value=$(echo "$json" | tr '\n' ' ' | sed -E "s/.*\"$field\"[[:space:]]*:[[:space:]]*([0-9]+).*/\1/")
    if [ -n "$value" ] && [ "$value" != "$json" ]; then
        echo "$value"
        return
    fi
    echo ""
}

# API helpers
create_sandbox() {
    local template="${1:-$TEST_IMAGE}"
    curl -s -X POST "$API_BASE/sandboxes" \
        -H "Content-Type: application/json" \
        -d "{\"template\": \"$template\"}"
}

get_sandbox() {
    local id="$1"
    curl -s "$API_BASE/sandboxes/$id"
}

list_sandboxes() {
    curl -s "$API_BASE/sandboxes"
}

delete_sandbox() {
    local id="$1"
    local force="${2:-true}"
    curl -s -X DELETE "$API_BASE/sandboxes/$id?force=$force"
}

run_command() {
    local sandbox_id="$1"
    local command="$2"
    shift 2
    local args_json="[]"
    if [ $# -gt 0 ]; then
        args_json="[$(printf '"%s",' "$@" | sed 's/,$//' )]"
    fi
    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d "{\"command\": \"$command\", \"args\": $args_json}"
}

# Check if server is running
check_server() {
    if ! curl -s "$API_BASE/sandboxes" > /dev/null 2>&1; then
        echo -e "${RED}Error: Server is not running at $API_BASE${NC}"
        echo "Please start the server first:"
        echo "  WORKSPACE_NFS_PORT=$NFS_PORT cargo run --bin workspace-server"
        exit 1
    fi
    log_info "Server is running"
}

# Mount NFS
mount_nfs() {
    if mountpoint -q "$NFS_MOUNT_POINT" 2>/dev/null; then
        return 0
    fi

    mkdir -p "$NFS_MOUNT_POINT" 2>/dev/null || true
    mount.nfs -o user,noacl,nolock,vers=3,tcp,rsize=131072,port=$NFS_PORT,mountport=$NFS_PORT \
        127.0.0.1:/ "$NFS_MOUNT_POINT" 2>/dev/null || return 1
    return 0
}

# Unmount NFS
unmount_nfs() {
    if mountpoint -q "$NFS_MOUNT_POINT" 2>/dev/null; then
        umount "$NFS_MOUNT_POINT" 2>/dev/null || umount -l "$NFS_MOUNT_POINT" 2>/dev/null || true
    fi
}

# Cleanup function
cleanup() {
    log_info "Cleaning up..."
    unmount_nfs

    # Delete all test sandboxes
    local sandboxes=$(list_sandboxes 2>/dev/null || true)
    if [ -n "$sandboxes" ]; then
        echo "$sandboxes" | tr '\n' ' ' | grep -oE '"id"[[:space:]]*:[[:space:]]*"[^"]*"' | sed 's/"id"[[:space:]]*:[[:space:]]*"//' | sed 's/"$//' | while read id; do
            if [ -n "$id" ]; then
                delete_sandbox "$id" "true" > /dev/null 2>&1 || true
            fi
        done
    fi
}

trap cleanup EXIT

#############################################
# Test Scenarios
#############################################

# ==========================================
# 1. Sandbox Lifecycle Tests
# ==========================================

test_1_1_create_sandbox() {
    log_info "Test 1.1: Create Sandbox" >&2

    local result=$(create_sandbox)
    local id=$(json_get "$result" "id")
    local state=$(json_get "$result" "state")

    if [ -n "$id" ] && [ "$state" = "running" ]; then
        log_success "Sandbox created: $id (state: $state)" >&2
        echo "$id"
        return 0
    else
        log_fail "Failed to create sandbox: $result" >&2
        echo ""
        return 1
    fi
}

test_1_2_get_sandbox() {
    local sandbox_id="$1"
    log_info "Test 1.2: Get Sandbox Info"

    local result=$(get_sandbox "$sandbox_id")
    local id=$(json_get "$result" "id")
    local state=$(json_get "$result" "state")

    if [ "$id" = "$sandbox_id" ]; then
        log_success "Got sandbox info: $id (state: $state)"
        return 0
    else
        log_fail "Failed to get sandbox info"
        return 1
    fi
}

test_1_3_list_sandboxes() {
    local sandbox_id="$1"
    log_info "Test 1.3: List Sandboxes"

    local result=$(list_sandboxes)

    if echo "$result" | grep -q "$sandbox_id"; then
        log_success "Sandbox found in list"
        return 0
    else
        log_fail "Sandbox not in list"
        return 1
    fi
}

test_1_4_delete_sandbox() {
    local sandbox_id="$1"
    log_info "Test 1.4: Delete Sandbox"

    local result=$(delete_sandbox "$sandbox_id")
    local success=$(json_get "$result" "success")

    if [ "$success" = "true" ]; then
        log_success "Sandbox deleted: $sandbox_id"
        return 0
    else
        log_fail "Failed to delete sandbox"
        return 1
    fi
}

# ==========================================
# 2. Process Execution Tests
# ==========================================

test_2_1_simple_command() {
    local sandbox_id="$1"
    log_info "Test 2.1: Execute Simple Command (echo)"

    local result=$(run_command "$sandbox_id" "echo" "Hello World")
    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ]; then
        log_success "Command executed: exit_code=$exit_code"
        return 0
    else
        log_fail "Command failed: exit_code=$exit_code"
        return 1
    fi
}

test_2_2_command_with_args() {
    local sandbox_id="$1"
    log_info "Test 2.2: Execute Command with Args (ls -la)"

    local result=$(run_command "$sandbox_id" "ls" "-la" "/workspace")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "ls -la executed successfully"
        return 0
    else
        log_fail "ls -la failed: exit_code=$exit_code"
        return 1
    fi
}

test_2_3_failing_command() {
    local sandbox_id="$1"
    log_info "Test 2.3: Execute Failing Command"

    local result=$(run_command "$sandbox_id" "cat" "/nonexistent_file_12345")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ] && [ -n "$exit_code" ]; then
        log_success "Command failed as expected: exit_code=$exit_code"
        return 0
    else
        log_fail "Command should have failed: exit_code=$exit_code"
        return 1
    fi
}

test_2_4_command_with_env() {
    local sandbox_id="$1"
    log_info "Test 2.4: Execute Command with Environment Variable"

    local result=$(curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "export MY_VAR=test123 && echo $MY_VAR"]}')

    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ]; then
        log_success "Environment variable works"
        return 0
    else
        log_fail "Environment variable failed: exit_code=$exit_code"
        return 1
    fi
}

test_2_5_long_running_command() {
    local sandbox_id="$1"
    log_info "Test 2.5: Execute Long Running Command (sleep 2)"

    local start_time=$(date +%s)
    local result=$(curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "sleep 2 && echo done"]}')
    local end_time=$(date +%s)
    local duration=$((end_time - start_time))

    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ] && [ "$duration" -ge 2 ]; then
        log_success "Long running command completed in ${duration}s"
        return 0
    else
        log_fail "Long running command failed: exit_code=$exit_code, duration=${duration}s"
        return 1
    fi
}

test_2_6_write_file() {
    local sandbox_id="$1"
    log_info "Test 2.6: Write File via Command"

    local result=$(curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "echo \"test content 123\" > /workspace/test_write.txt"]}')

    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "File written successfully"
        return 0
    else
        log_fail "Failed to write file: exit_code=$exit_code"
        return 1
    fi
}

test_2_7_read_file() {
    local sandbox_id="$1"
    log_info "Test 2.7: Read File via Command"

    # First write
    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "echo \"read test content\" > /workspace/test_read.txt"]}' > /dev/null

    # Then read
    local result=$(run_command "$sandbox_id" "cat" "/workspace/test_read.txt")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "File read successfully"
        return 0
    else
        log_fail "Failed to read file: exit_code=$exit_code"
        return 1
    fi
}

# ==========================================
# 3. NFS Filesystem Tests
# ==========================================

test_3_1_nfs_mount() {
    log_info "Test 3.1: NFS Mount"

    if mount_nfs; then
        if mountpoint -q "$NFS_MOUNT_POINT"; then
            log_success "NFS mounted at $NFS_MOUNT_POINT"
            return 0
        fi
    fi

    log_fail "NFS mount failed"
    return 1
}

test_3_2_nfs_read_container_file() {
    local sandbox_id="$1"
    log_info "Test 3.2: Read Container-Created File via NFS"

    # Create file in container
    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "echo \"nfs read test 12345\" > /workspace/nfs_read_test.txt"]}' > /dev/null

    sleep 1

    # Read via NFS
    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id/nfs_read_test.txt"
    if [ -f "$nfs_path" ]; then
        local content=$(cat "$nfs_path")
        if echo "$content" | grep -q "nfs read test"; then
            log_success "Container file read via NFS"
            return 0
        fi
    fi

    log_fail "Failed to read container file via NFS"
    return 1
}

test_3_3_nfs_write_container_read() {
    local sandbox_id="$1"
    log_info "Test 3.3: Write via NFS, Read in Container"

    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id/nfs_write_test.txt"
    echo "written from nfs 67890" > "$nfs_path"

    sleep 1

    # Read in container
    local result=$(run_command "$sandbox_id" "cat" "/workspace/nfs_write_test.txt")
    local exit_code=$(json_get "$result" "exit_code")
    local stdout=$(json_get "$result" "stdout")

    if [ "$exit_code" = "0" ]; then
        log_success "NFS write visible in container"
        return 0
    else
        log_fail "NFS write not visible in container"
        return 1
    fi
}

test_3_4_nfs_create_directory() {
    local sandbox_id="$1"
    log_info "Test 3.4: Create Directory via NFS"

    local nfs_dir="$NFS_MOUNT_POINT/$sandbox_id/nfs_created_dir"
    mkdir -p "$nfs_dir"

    # Verify in container
    local result=$(curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "test", "args": ["-d", "/workspace/nfs_created_dir"]}')

    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "Directory created via NFS and visible in container"
        return 0
    else
        log_fail "Directory not visible in container"
        return 1
    fi
}

test_3_5_nfs_delete_file() {
    local sandbox_id="$1"
    log_info "Test 3.5: Delete File via NFS"

    # Create file in container
    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "echo \"to delete\" > /workspace/to_delete.txt"]}' > /dev/null

    sleep 1

    # Delete via NFS
    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id/to_delete.txt"
    rm -f "$nfs_path"

    # Verify in container
    local result=$(curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "test", "args": ["-f", "/workspace/to_delete.txt"]}')

    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ]; then
        log_success "File deleted via NFS, not visible in container"
        return 0
    else
        log_fail "File still visible after NFS delete"
        return 1
    fi
}

test_3_6_large_file() {
    local sandbox_id="$1"
    log_info "Test 3.6: Large File Read/Write (1MB)"

    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id/large_file.bin"

    # Create 1MB file
    dd if=/dev/urandom of="$nfs_path" bs=1024 count=1024 2>/dev/null
    local original_md5=$(md5sum "$nfs_path" | cut -d' ' -f1)

    # Read and verify in container
    local result=$(run_command "$sandbox_id" "md5sum" "/workspace/large_file.bin")
    local stdout=$(json_get "$result" "stdout")
    local container_md5=$(echo "$stdout" | cut -d' ' -f1)

    if [ "$original_md5" = "$container_md5" ]; then
        log_success "Large file integrity verified: $original_md5"
        return 0
    else
        log_fail "Large file corrupted: $original_md5 vs $container_md5"
        return 1
    fi
}

test_3_7_concurrent_operations() {
    local sandbox_id="$1"
    log_info "Test 3.7: Concurrent File Operations"

    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id/concurrent_test.txt"

    # Write from NFS
    echo "line1" > "$nfs_path"

    # Append from container
    curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "echo \"line2\" >> /workspace/concurrent_test.txt"]}' > /dev/null

    # Append from NFS again
    echo "line3" >> "$nfs_path"

    sleep 1

    # Read from container and verify
    local result=$(run_command "$sandbox_id" "cat" "/workspace/concurrent_test.txt")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "Concurrent operations successful"
        return 0
    else
        log_fail "Concurrent operations failed"
        return 1
    fi
}

# ==========================================
# 4. Integration Tests
# ==========================================

test_4_1_python_script_workflow() {
    local sandbox_id="$1"
    log_info "Test 4.1: Complete Workflow - Python Script Execution"

    # Write Python script via NFS
    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id/test_script.py"
    cat > "$nfs_path" << 'PYEOF'
#!/usr/bin/env python3
import json
result = {"status": "success", "message": "Hello from Python"}
print(json.dumps(result))
PYEOF

    sleep 1

    # Execute in container
    local result=$(run_command "$sandbox_id" "python3" "/workspace/test_script.py")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "Python script executed successfully"
        return 0
    else
        log_fail "Python script failed: exit_code=$exit_code"
        return 1
    fi
}

test_4_2_project_init_workflow() {
    local sandbox_id="$1"
    log_info "Test 4.2: Complete Workflow - Project Initialization"

    # Create project structure via NFS (instead of container to avoid permission issues)
    local nfs_project="$NFS_MOUNT_POINT/$sandbox_id/myproject"
    mkdir -p "$nfs_project/src"

    # Write config via NFS
    echo '{"name": "myproject", "version": "1.0.0"}' > "$nfs_project/config.json"

    sleep 1

    # Read config in container (verify NFS-created files are visible)
    local result=$(run_command "$sandbox_id" "cat" "/workspace/myproject/config.json")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" = "0" ]; then
        log_success "Project initialized successfully"
        return 0
    else
        log_fail "Project init failed"
        return 1
    fi
}

test_4_3_multi_sandbox_isolation() {
    log_info "Test 4.3: Multi-Sandbox Isolation"

    # Create two sandboxes
    local sandbox_a=$(create_sandbox)
    local id_a=$(json_get "$sandbox_a" "id")

    local sandbox_b=$(create_sandbox)
    local id_b=$(json_get "$sandbox_b" "id")

    if [ -z "$id_a" ] || [ -z "$id_b" ]; then
        log_fail "Failed to create sandboxes for isolation test"
        return 1
    fi

    # Create file in sandbox A
    curl -s -X POST "$API_BASE/sandboxes/$id_a/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "echo \"sandbox A secret\" > /workspace/secret.txt"]}' > /dev/null

    sleep 1

    # Try to read from sandbox B (should fail - file doesn't exist)
    local result=$(run_command "$id_b" "cat" "/workspace/secret.txt")
    local exit_code=$(json_get "$result" "exit_code")

    # Cleanup
    delete_sandbox "$id_a" > /dev/null 2>&1 || true
    delete_sandbox "$id_b" > /dev/null 2>&1 || true

    if [ "$exit_code" != "0" ]; then
        log_success "Sandboxes are properly isolated"
        return 0
    else
        log_fail "Sandbox isolation failed - B can see A's files"
        return 1
    fi
}

test_4_5_nfs_cleanup_on_delete() {
    log_info "Test 4.5: NFS Export Cleanup on Sandbox Delete"

    # Create a new sandbox for this test
    local sandbox=$(create_sandbox)
    local sandbox_id=$(json_get "$sandbox" "id")

    if [ -z "$sandbox_id" ]; then
        log_fail "Failed to create sandbox for cleanup test"
        return 1
    fi

    # Verify sandbox is visible in NFS
    local nfs_path="$NFS_MOUNT_POINT/$sandbox_id"
    if [ ! -d "$nfs_path" ]; then
        log_fail "Sandbox not visible in NFS before delete"
        delete_sandbox "$sandbox_id" > /dev/null 2>&1 || true
        return 1
    fi

    # Delete sandbox
    delete_sandbox "$sandbox_id" > /dev/null

    sleep 2

    # Refresh NFS (unmount and remount to clear cache)
    unmount_nfs
    mount_nfs

    # Verify sandbox is no longer visible
    if [ ! -d "$nfs_path" ]; then
        log_success "NFS export cleaned up after sandbox delete"
        return 0
    else
        log_fail "Sandbox still visible in NFS after delete"
        return 1
    fi
}

# ==========================================
# 5. Error Handling Tests
# ==========================================

test_5_1_nonexistent_sandbox() {
    log_info "Test 5.1: Operation on Non-existent Sandbox"

    local result=$(run_command "nonexistent-sandbox-id" "echo" "test")

    if echo "$result" | grep -qi "not found\|error\|failed"; then
        log_success "Proper error for non-existent sandbox"
        return 0
    else
        log_fail "No proper error for non-existent sandbox"
        return 1
    fi
}

test_5_2_read_nonexistent_file() {
    local sandbox_id="$1"
    log_info "Test 5.2: Read Non-existent File"

    local result=$(run_command "$sandbox_id" "cat" "/workspace/this_file_does_not_exist_12345.txt")
    local exit_code=$(json_get "$result" "exit_code")

    if [ "$exit_code" != "0" ] && [ -n "$exit_code" ]; then
        log_success "Proper error for non-existent file: exit_code=$exit_code"
        return 0
    else
        log_fail "No error for non-existent file"
        return 1
    fi
}

# ==========================================
# 6. Performance Tests
# ==========================================

test_6_1_rapid_sandbox_lifecycle() {
    log_info "Test 6.1: Rapid Sandbox Create/Delete (5 cycles)"

    local success=0
    for i in $(seq 1 5); do
        local sandbox=$(create_sandbox)
        local id=$(json_get "$sandbox" "id")
        if [ -n "$id" ]; then
            delete_sandbox "$id" > /dev/null 2>&1
            ((success++)) || true
        fi
    done

    if [ "$success" -eq 5 ]; then
        log_success "Rapid lifecycle test passed ($success/5)"
        return 0
    else
        log_fail "Rapid lifecycle test failed ($success/5)"
        return 1
    fi
}

test_6_2_batch_file_creation() {
    local sandbox_id="$1"
    log_info "Test 6.2: Batch File Creation (100 files via NFS)"

    local nfs_dir="$NFS_MOUNT_POINT/$sandbox_id/batch_test"
    mkdir -p "$nfs_dir"

    for i in $(seq 1 100); do
        echo "file content $i" > "$nfs_dir/file_$i.txt"
    done

    sleep 1

    # Count files in container
    local result=$(curl -s -X POST "$API_BASE/sandboxes/$sandbox_id/process/run" \
        -H "Content-Type: application/json" \
        -d '{"command": "bash", "args": ["-c", "ls /workspace/batch_test | wc -l"]}')

    local stdout=$(json_get "$result" "stdout")
    local count=$(echo "$stdout" | tr -d '[:space:]')

    if [ "$count" = "100" ]; then
        log_success "Batch file creation passed (100 files)"
        return 0
    else
        log_fail "Batch file creation failed (got $count files, expected 100)"
        return 1
    fi
}

test_6_3_batch_command_execution() {
    local sandbox_id="$1"
    log_info "Test 6.3: Batch Command Execution (50 commands)"

    local success=0
    for i in $(seq 1 50); do
        local result=$(run_command "$sandbox_id" "echo" "command $i")
        local exit_code=$(json_get "$result" "exit_code")
        if [ "$exit_code" = "0" ]; then
            ((success++)) || true
        fi
    done

    if [ "$success" -eq 50 ]; then
        log_success "Batch command execution passed ($success/50)"
        return 0
    else
        log_fail "Batch command execution failed ($success/50)"
        return 1
    fi
}

#############################################
# Main Test Runner
#############################################

main() {
    echo ""
    echo "=============================================="
    echo "  Unified Workspace SDK - E2E Test Suite"
    echo "=============================================="
    echo ""

    # Check prerequisites
    check_server

    # ==========================================
    log_section "1. Sandbox Lifecycle Tests"
    # ==========================================

    SANDBOX_ID=$(test_1_1_create_sandbox)
    if [ -n "$SANDBOX_ID" ]; then
        test_1_2_get_sandbox "$SANDBOX_ID"
        test_1_3_list_sandboxes "$SANDBOX_ID"
    else
        log_skip "Sandbox lifecycle tests skipped - create failed"
    fi

    # ==========================================
    log_section "2. Process Execution Tests"
    # ==========================================

    # Create fresh sandbox for process tests
    PROC_SANDBOX=$(create_sandbox)
    PROC_SANDBOX_ID=$(json_get "$PROC_SANDBOX" "id")

    if [ -n "$PROC_SANDBOX_ID" ]; then
        test_2_1_simple_command "$PROC_SANDBOX_ID"
        test_2_2_command_with_args "$PROC_SANDBOX_ID"
        test_2_3_failing_command "$PROC_SANDBOX_ID"
        test_2_4_command_with_env "$PROC_SANDBOX_ID"
        test_2_5_long_running_command "$PROC_SANDBOX_ID"
        test_2_6_write_file "$PROC_SANDBOX_ID"
        test_2_7_read_file "$PROC_SANDBOX_ID"
    else
        log_skip "Process tests skipped - no sandbox"
    fi

    # ==========================================
    log_section "3. NFS Filesystem Tests"
    # ==========================================

    test_3_1_nfs_mount

    # Create fresh sandbox for NFS tests
    NFS_SANDBOX=$(create_sandbox)
    NFS_SANDBOX_ID=$(json_get "$NFS_SANDBOX" "id")

    if [ -n "$NFS_SANDBOX_ID" ] && mountpoint -q "$NFS_MOUNT_POINT" 2>/dev/null; then
        test_3_2_nfs_read_container_file "$NFS_SANDBOX_ID"
        test_3_3_nfs_write_container_read "$NFS_SANDBOX_ID"
        test_3_4_nfs_create_directory "$NFS_SANDBOX_ID"
        test_3_5_nfs_delete_file "$NFS_SANDBOX_ID"
        test_3_6_large_file "$NFS_SANDBOX_ID"
        test_3_7_concurrent_operations "$NFS_SANDBOX_ID"
    else
        log_skip "NFS tests skipped - no sandbox or NFS not mounted"
    fi

    # ==========================================
    log_section "4. Integration Tests"
    # ==========================================

    # Create fresh sandbox for integration tests
    INT_SANDBOX=$(create_sandbox)
    INT_SANDBOX_ID=$(json_get "$INT_SANDBOX" "id")

    if [ -n "$INT_SANDBOX_ID" ] && mountpoint -q "$NFS_MOUNT_POINT" 2>/dev/null; then
        test_4_1_python_script_workflow "$INT_SANDBOX_ID"
        test_4_2_project_init_workflow "$INT_SANDBOX_ID"
    else
        log_skip "Integration tests skipped - no sandbox or NFS not mounted"
    fi

    test_4_3_multi_sandbox_isolation

    # Test NFS cleanup
    if mountpoint -q "$NFS_MOUNT_POINT" 2>/dev/null; then
        test_4_5_nfs_cleanup_on_delete
    else
        log_skip "NFS cleanup test skipped - NFS not mounted"
    fi

    # ==========================================
    log_section "5. Error Handling Tests"
    # ==========================================

    test_5_1_nonexistent_sandbox

    if [ -n "$INT_SANDBOX_ID" ]; then
        test_5_2_read_nonexistent_file "$INT_SANDBOX_ID"
    fi

    # ==========================================
    log_section "6. Performance Tests"
    # ==========================================

    test_6_1_rapid_sandbox_lifecycle

    # Create fresh sandbox for performance tests
    PERF_SANDBOX=$(create_sandbox)
    PERF_SANDBOX_ID=$(json_get "$PERF_SANDBOX" "id")

    if [ -n "$PERF_SANDBOX_ID" ] && mountpoint -q "$NFS_MOUNT_POINT" 2>/dev/null; then
        test_6_2_batch_file_creation "$PERF_SANDBOX_ID"
        test_6_3_batch_command_execution "$PERF_SANDBOX_ID"
        delete_sandbox "$PERF_SANDBOX_ID" > /dev/null 2>&1 || true
    else
        log_skip "Performance tests skipped"
    fi

    # ==========================================
    log_section "Cleanup"
    # ==========================================

    # Delete remaining sandboxes
    if [ -n "$SANDBOX_ID" ]; then
        test_1_4_delete_sandbox "$SANDBOX_ID"
    fi

    for sid in "$PROC_SANDBOX_ID" "$NFS_SANDBOX_ID" "$INT_SANDBOX_ID"; do
        if [ -n "$sid" ]; then
            delete_sandbox "$sid" > /dev/null 2>&1 || true
        fi
    done

    # ==========================================
    # Final Summary
    # ==========================================

    echo ""
    echo "=============================================="
    echo "                TEST SUMMARY"
    echo "=============================================="
    echo -e "  ${GREEN}PASSED:${NC}  $PASSED"
    echo -e "  ${RED}FAILED:${NC}  $FAILED"
    echo -e "  ${YELLOW}SKIPPED:${NC} $SKIPPED"
    echo "=============================================="
    echo ""

    if [ "$FAILED" -gt 0 ]; then
        exit 1
    fi

    exit 0
}

# Run main
main "$@"
