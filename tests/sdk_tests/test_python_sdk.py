#!/usr/bin/env python3
"""
Python SDK End-to-End Tests
Tests multiple scenarios using the Python SDK
"""

import sys
import os
import time
import asyncio

# Add SDK to path
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '../../sdk-python/src'))

from workspace_sdk import WorkspaceClient, AsyncWorkspaceClient
from workspace_sdk.types import CreateSandboxParams, RunCommandOptions

# Configuration
API_URL = os.environ.get("WORKSPACE_API_URL", "http://localhost:8080")
TEST_IMAGE = os.environ.get("TEST_IMAGE", "workspace-test:latest")

# Test results
PASSED = 0
FAILED = 0

def log_pass(msg: str):
    global PASSED
    PASSED += 1
    print(f"  \033[32m[PASS]\033[0m {msg}")

def log_fail(msg: str):
    global FAILED
    FAILED += 1
    print(f"  \033[31m[FAIL]\033[0m {msg}")

def log_section(title: str):
    print(f"\n\033[1;33m{'='*50}\033[0m")
    print(f"\033[1;33m{title}\033[0m")
    print(f"\033[1;33m{'='*50}\033[0m")


# ========================================
# Sync Client Tests
# ========================================

def test_sync_sandbox_lifecycle():
    """Test 1: Sandbox create, get, list, delete (sync)"""
    log_section("Test 1: Sync Sandbox Lifecycle")

    with WorkspaceClient.create(API_URL) as client:
        # Create sandbox
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = client.sandbox.create(params)

        if sandbox.id and sandbox.state == "running":
            log_pass(f"Created sandbox: {sandbox.id}")
        else:
            log_fail(f"Failed to create sandbox: {sandbox}")
            return None

        # Get sandbox
        fetched = client.sandbox.get(sandbox.id)
        if fetched.id == sandbox.id:
            log_pass(f"Got sandbox info: state={fetched.state}")
        else:
            log_fail("Failed to get sandbox")

        # List sandboxes
        sandboxes = client.sandbox.list()
        if any(s.id == sandbox.id for s in sandboxes):
            log_pass(f"Listed sandboxes: found {len(sandboxes)} total")
        else:
            log_fail("Sandbox not in list")

        # Delete sandbox
        client.sandbox.delete(sandbox.id, force=True)
        log_pass(f"Deleted sandbox: {sandbox.id}")

        return sandbox.id


def test_sync_process_execution():
    """Test 2: Process execution (sync)"""
    log_section("Test 2: Sync Process Execution")

    with WorkspaceClient.create(API_URL) as client:
        # Create sandbox
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = client.sandbox.create(params)

        try:
            # Simple echo command
            result = client.process.run(sandbox.id, "echo", RunCommandOptions(args=["Hello", "World"]))
            if result.exit_code == 0 and "Hello World" in result.stdout:
                log_pass(f"Echo command: stdout='{result.stdout.strip()}'")
            else:
                log_fail(f"Echo failed: {result}")

            # Command with arguments
            result = client.process.run(sandbox.id, "ls", RunCommandOptions(args=["-la", "/workspace"]))
            if result.exit_code == 0:
                log_pass("ls -la command executed successfully")
            else:
                log_fail(f"ls failed: exit_code={result.exit_code}")

            # Failing command
            result = client.process.run(sandbox.id, "bash", RunCommandOptions(args=["-c", "exit 42"]))
            if result.exit_code == 42:
                log_pass(f"Failing command returned correct exit code: {result.exit_code}")
            else:
                log_fail(f"Expected exit code 42, got {result.exit_code}")

            # Command with environment variable
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "echo $MY_VAR"], env={"MY_VAR": "test_value"})
            )
            if result.exit_code == 0 and "test_value" in result.stdout:
                log_pass(f"Env var command: stdout='{result.stdout.strip()}'")
            else:
                log_fail(f"Env var failed: {result}")

            # Write and read file
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "echo 'test content' > /workspace/test.txt && cat /workspace/test.txt"])
            )
            if result.exit_code == 0 and "test content" in result.stdout:
                log_pass("File write/read successful")
            else:
                log_fail(f"File write/read failed: {result}")

        finally:
            client.sandbox.delete(sandbox.id, force=True)


def test_sync_multiple_sandboxes():
    """Test 3: Multiple sandboxes isolation (sync)"""
    log_section("Test 3: Sync Multiple Sandboxes Isolation")

    with WorkspaceClient.create(API_URL) as client:
        # Create two sandboxes
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox_a = client.sandbox.create(params)
        sandbox_b = client.sandbox.create(params)

        try:
            # Write file in sandbox A
            client.process.run(
                sandbox_a.id,
                "bash",
                RunCommandOptions(args=["-c", "echo 'secret_data' > /workspace/secret.txt"])
            )
            log_pass(f"Created file in sandbox A: {sandbox_a.id}")

            # Try to read from sandbox B (should fail)
            result = client.process.run(
                sandbox_b.id,
                "cat",
                RunCommandOptions(args=["/workspace/secret.txt"])
            )

            if result.exit_code != 0:
                log_pass("Sandbox isolation verified: B cannot read A's files")
            else:
                log_fail("Isolation broken: B can read A's files!")

        finally:
            client.sandbox.delete(sandbox_a.id, force=True)
            client.sandbox.delete(sandbox_b.id, force=True)


def test_sync_long_running_command():
    """Test 4: Long running command (sync)"""
    log_section("Test 4: Sync Long Running Command")

    with WorkspaceClient.create(API_URL, timeout=60.0) as client:
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = client.sandbox.create(params)

        try:
            start_time = time.time()
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "sleep 3 && echo 'done'"])
            )
            elapsed = time.time() - start_time

            if result.exit_code == 0 and "done" in result.stdout and elapsed >= 3:
                log_pass(f"Long running command completed in {elapsed:.1f}s")
            else:
                log_fail(f"Long running command failed: {result}")

        finally:
            client.sandbox.delete(sandbox.id, force=True)


def test_sync_python_execution():
    """Test 5: Execute Python script in sandbox (sync)"""
    log_section("Test 5: Sync Script Execution (bash)")

    with WorkspaceClient.create(API_URL) as client:
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = client.sandbox.create(params)

        try:
            # Execute a bash script that does some computation
            script = "for i in 1 2 3; do echo \"item_$i\"; done"
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", script])
            )

            if result.exit_code == 0 and "item_1" in result.stdout and "item_3" in result.stdout:
                log_pass(f"Bash script executed with loop output")
            else:
                log_fail(f"Bash script failed: {result}")

            # Test complex command with pipes
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "echo 'hello world' | tr 'a-z' 'A-Z'"])
            )

            if result.exit_code == 0 and "HELLO WORLD" in result.stdout:
                log_pass(f"Pipe command success: {result.stdout.strip()}")
            else:
                log_fail(f"Pipe command failed: {result}")

        finally:
            client.sandbox.delete(sandbox.id, force=True)


# ========================================
# Async Client Tests
# ========================================

async def test_async_sandbox_lifecycle():
    """Test 6: Sandbox create, get, list, delete (async)"""
    log_section("Test 6: Async Sandbox Lifecycle")

    async with AsyncWorkspaceClient.create(API_URL) as client:
        # Create sandbox
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = await client.sandbox.create(params)

        if sandbox.id and sandbox.state == "running":
            log_pass(f"Created sandbox (async): {sandbox.id}")
        else:
            log_fail(f"Failed to create sandbox: {sandbox}")
            return

        # Get sandbox
        fetched = await client.sandbox.get(sandbox.id)
        if fetched.id == sandbox.id:
            log_pass(f"Got sandbox info (async): state={fetched.state}")
        else:
            log_fail("Failed to get sandbox")

        # List sandboxes
        sandboxes = await client.sandbox.list()
        if any(s.id == sandbox.id for s in sandboxes):
            log_pass(f"Listed sandboxes (async): found {len(sandboxes)} total")
        else:
            log_fail("Sandbox not in list")

        # Delete sandbox
        await client.sandbox.delete(sandbox.id, force=True)
        log_pass(f"Deleted sandbox (async): {sandbox.id}")


async def test_async_process_execution():
    """Test 7: Process execution (async)"""
    log_section("Test 7: Async Process Execution")

    async with AsyncWorkspaceClient.create(API_URL) as client:
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = await client.sandbox.create(params)

        try:
            # Simple echo command
            result = await client.process.run(sandbox.id, "echo", RunCommandOptions(args=["Async", "Test"]))
            if result.exit_code == 0 and "Async Test" in result.stdout:
                log_pass(f"Async echo: stdout='{result.stdout.strip()}'")
            else:
                log_fail(f"Async echo failed: {result}")

            # Multiple commands
            result = await client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "pwd && whoami && hostname"])
            )
            if result.exit_code == 0:
                log_pass(f"Async multi-command success")
            else:
                log_fail(f"Async multi-command failed: {result}")

        finally:
            await client.sandbox.delete(sandbox.id, force=True)


async def test_async_concurrent_sandboxes():
    """Test 8: Concurrent sandbox operations (async)"""
    log_section("Test 8: Async Concurrent Sandbox Operations")

    async with AsyncWorkspaceClient.create(API_URL) as client:
        # Create multiple sandboxes concurrently
        params = CreateSandboxParams(template=TEST_IMAGE)

        start_time = time.time()
        sandboxes = await asyncio.gather(
            client.sandbox.create(params),
            client.sandbox.create(params),
            client.sandbox.create(params),
        )
        create_time = time.time() - start_time

        sandbox_ids = [s.id for s in sandboxes]
        if all(s.state == "running" for s in sandboxes):
            log_pass(f"Created 3 sandboxes concurrently in {create_time:.2f}s")
        else:
            log_fail("Some sandboxes failed to start")

        try:
            # Run commands concurrently
            start_time = time.time()
            results = await asyncio.gather(
                client.process.run(sandbox_ids[0], "echo", RunCommandOptions(args=["sandbox1"])),
                client.process.run(sandbox_ids[1], "echo", RunCommandOptions(args=["sandbox2"])),
                client.process.run(sandbox_ids[2], "echo", RunCommandOptions(args=["sandbox3"])),
            )
            cmd_time = time.time() - start_time

            if all(r.exit_code == 0 for r in results):
                log_pass(f"Ran 3 commands concurrently in {cmd_time:.2f}s")
            else:
                log_fail("Some concurrent commands failed")

        finally:
            # Delete concurrently
            await asyncio.gather(*[
                client.sandbox.delete(sid, force=True) for sid in sandbox_ids
            ])
            log_pass("Deleted 3 sandboxes concurrently")


async def test_async_error_handling():
    """Test 9: Error handling (async)"""
    log_section("Test 9: Async Error Handling")

    async with AsyncWorkspaceClient.create(API_URL) as client:
        # Try to get non-existent sandbox
        try:
            await client.sandbox.get("non-existent-sandbox-id")
            log_fail("Should have raised error for non-existent sandbox")
        except Exception as e:
            log_pass(f"Correct error for non-existent sandbox: {type(e).__name__}")

        # Create sandbox for more tests
        params = CreateSandboxParams(template=TEST_IMAGE)
        sandbox = await client.sandbox.create(params)

        try:
            # Command that returns non-zero exit code
            result = await client.process.run(
                sandbox.id,
                "cat",
                RunCommandOptions(args=["/nonexistent/file.txt"])
            )
            if result.exit_code != 0 and result.stderr:
                log_pass(f"Correct error for missing file: exit_code={result.exit_code}")
            else:
                log_fail("Expected non-zero exit code for missing file")

        finally:
            await client.sandbox.delete(sandbox.id, force=True)


# ========================================
# Main
# ========================================

def main():
    print("\n" + "=" * 60)
    print("  Python SDK End-to-End Test Suite")
    print("=" * 60)
    print(f"API URL: {API_URL}")
    print(f"Test Image: {TEST_IMAGE}")

    # Sync tests
    test_sync_sandbox_lifecycle()
    test_sync_process_execution()
    test_sync_multiple_sandboxes()
    test_sync_long_running_command()
    test_sync_python_execution()

    # Async tests
    asyncio.run(test_async_sandbox_lifecycle())
    asyncio.run(test_async_process_execution())
    asyncio.run(test_async_concurrent_sandboxes())
    asyncio.run(test_async_error_handling())

    # Summary
    print("\n" + "=" * 60)
    print("                 TEST SUMMARY")
    print("=" * 60)
    print(f"  \033[32mPASSED:\033[0m  {PASSED}")
    print(f"  \033[31mFAILED:\033[0m  {FAILED}")
    print("=" * 60 + "\n")

    return 0 if FAILED == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
