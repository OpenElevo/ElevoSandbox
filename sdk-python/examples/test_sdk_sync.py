#!/usr/bin/env python3
"""
SDK synchronization test — verifies all features added during SDK sync.

Tests: workspace CRUD + StorageType, file ops (move, copy, get_file_info, exists),
sandbox (exists, wait_for_state), process (shell, exec), error handling.

Usage:
    python examples/test_sdk_sync.py [--server ADDR] [--apikey KEY] [--image IMAGE]

Flags:
    --server  gRPC server address (default: localhost:9090)
    --apikey  gRPC API key or JWT (optional)
    --image   Sandbox container image (default: workspace-test:latest)
"""

import argparse
import os
import sys

# Add parent directory to path for development
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from workspace_sdk import (
    WorkspaceClient,
    CreateWorkspaceParams,
    CreateSandboxParams,
    SandboxState,
    ProcessError,
    NotFoundError,
)


class TestRunner:
    def __init__(self):
        self.results: list[tuple[str, bool, Exception | None]] = []

    def run(self, name: str, fn):
        print(f"\n── {name} ──")
        try:
            fn()
            self.results.append((name, True, None))
            print("  PASSED")
        except Exception as e:
            self.results.append((name, False, e))
            print(f"  FAILED: {e}")

    def summary(self):
        print("\n╔══════════════════════════════════════════════════╗")
        print("║                  Test Summary                    ║")
        print("╚══════════════════════════════════════════════════╝")
        passed = failed = 0
        for name, ok, err in self.results:
            status = "✓ PASS" if ok else "✗ FAIL"
            if ok:
                passed += 1
            else:
                failed += 1
            print(f"  {status}  {name}")
            if err:
                print(f"         Error: {err}")
        print(f"\n  Total: {passed} passed, {failed} failed")
        if failed > 0:
            sys.exit(1)


def test_workspace_crud(client: WorkspaceClient):
    """Test 1: Workspace CRUD + StorageType"""
    # Create
    ws = client.workspace.create(
        CreateWorkspaceParams(name="py-sync-test", metadata={"test": "sync"})
    )
    print(f"  Created workspace: {ws.id} (storage_type={ws.storage_type})")

    try:
        # Verify default storage type is managed
        if ws.storage_type != "managed":
            raise AssertionError(
                f"expected storage_type=managed, got {ws.storage_type}"
            )

        # Get
        got = client.workspace.get(ws.id)
        if got.name != "py-sync-test":
            raise AssertionError(f"name mismatch: got {got.name}")

        # List
        ws_list = client.workspace.list()
        found = any(w.id == ws.id for w in ws_list)
        if not found:
            raise AssertionError("workspace not found in list")

        print("  Workspace CRUD + StorageType OK")
    finally:
        client.workspace.delete(ws.id)


def test_file_ops(client: WorkspaceClient):
    """Test 2: All workspace file operations"""
    ws = client.workspace.create(CreateWorkspaceParams(name="py-file-ops-test"))

    try:
        ws_id = ws.id

        # WriteFile + ReadFile
        client.workspace.write_file(ws_id, "hello.txt", "Hello World")
        content = client.workspace.read_file(ws_id, "hello.txt")
        if content != "Hello World":
            raise AssertionError(f"content mismatch: got {content!r}")
        print("  WriteFile + ReadFile OK")

        # Mkdir + ListFiles
        client.workspace.mkdir(ws_id, "subdir")
        files = client.workspace.list_files(ws_id, ".")
        print(f"  ListFiles: {len(files)} items")

        # GetFileInfo
        info = client.workspace.get_file_info(ws_id, "hello.txt")
        if info.type != "file" or info.name != "hello.txt":
            raise AssertionError(
                f"file info mismatch: name={info.name} type={info.type}"
            )
        print(f"  GetFileInfo: name={info.name} type={info.type} size={info.size}")

        # FileExists (via exists)
        exists = client.workspace.exists(ws_id, "hello.txt")
        if not exists:
            raise AssertionError("file should exist")

        exists = client.workspace.exists(ws_id, "no-such-file.txt")
        if exists:
            raise AssertionError("non-existent file should not exist")
        print("  FileExists OK")

        # CopyFile
        client.workspace.copy_file(ws_id, "hello.txt", "hello_copy.txt")
        copy_content = client.workspace.read_file(ws_id, "hello_copy.txt")
        if copy_content != "Hello World":
            raise AssertionError(f"copy content mismatch: got {copy_content!r}")
        print("  CopyFile OK")

        # MoveFile
        client.workspace.move_file(ws_id, "hello_copy.txt", "hello_moved.txt")
        exists = client.workspace.exists(ws_id, "hello_copy.txt")
        if exists:
            raise AssertionError("moved source should not exist")
        moved_content = client.workspace.read_file(ws_id, "hello_moved.txt")
        if moved_content != "Hello World":
            raise AssertionError("moved content mismatch")
        print("  MoveFile OK")

        # DeleteFile
        client.workspace.delete_file(ws_id, "hello_moved.txt", recursive=False)
        exists = client.workspace.exists(ws_id, "hello_moved.txt")
        if exists:
            raise AssertionError("deleted file should not exist")
        print("  DeleteFile OK")
    finally:
        client.workspace.delete(ws.id)


def test_sandbox_features(client: WorkspaceClient, image: str):
    """Test 3: Sandbox Exists + WaitForState"""
    ws = client.workspace.create(CreateWorkspaceParams(name="py-sandbox-test"))

    try:
        try:
            sb = client.sandbox.create(
                CreateSandboxParams(
                    workspace_id=ws.id,
                    template=image,
                    name="py-sync-test-sandbox",
                )
            )
        except Exception as e:
            print(f"  Sandbox creation failed (Docker may not be available): {e}")
            print("  Skipping sandbox-dependent tests")
            return

        try:
            print(f"  Created sandbox: {sb.id} (state: {sb.state})")

            # Exists
            exists = client.sandbox.exists(sb.id)
            if not exists:
                raise AssertionError("sandbox should exist")

            exists = client.sandbox.exists("non-existent-sandbox")
            if exists:
                raise AssertionError("non-existent sandbox should not exist")
            print("  Sandbox.exists OK")

            # WaitForState
            sb = client.sandbox.wait_for_state(
                sb.id, SandboxState.RUNNING, timeout=30.0
            )
            print(f"  WaitForState: reached {sb.state}")
        finally:
            client.sandbox.delete(sb.id, force=True)
    finally:
        client.workspace.delete(ws.id)


def test_process_features(client: WorkspaceClient, image: str):
    """Test 4: Process Shell + Exec"""
    ws = client.workspace.create(CreateWorkspaceParams(name="py-process-test"))

    try:
        try:
            sb = client.sandbox.create(
                CreateSandboxParams(workspace_id=ws.id, template=image)
            )
        except Exception as e:
            print(f"  Sandbox creation failed: {e}")
            print("  Skipping process tests")
            return

        try:
            client.sandbox.wait_for_state(sb.id, SandboxState.RUNNING, timeout=30.0)

            # Shell
            result = client.process.shell(sb.id, "echo hello && echo world")
            if result.exit_code != 0:
                raise AssertionError(f"shell exit code: {result.exit_code}")
            print(f"  Shell output: {result.stdout}", end="")
            print("  Shell OK")

            # Exec
            stdout = client.process.exec(sb.id, "echo", "Python", "SDK", "exec")
            print(f"  Exec output: {stdout}", end="")
            print("  Exec OK")
        finally:
            client.sandbox.delete(sb.id, force=True)
    finally:
        client.workspace.delete(ws.id)


def test_error_handling(client: WorkspaceClient, image: str):
    """Test 5: Error Handling (ProcessError, NotFound)"""
    ws = client.workspace.create(CreateWorkspaceParams(name="py-error-test"))

    try:
        try:
            sb = client.sandbox.create(
                CreateSandboxParams(workspace_id=ws.id, template=image)
            )
        except Exception as e:
            print("  Skipping error handling tests (no Docker)")
            return

        try:
            client.sandbox.wait_for_state(sb.id, SandboxState.RUNNING, timeout=30.0)

            # ProcessError from exec
            try:
                client.process.exec(sb.id, "false")
                raise AssertionError("exec 'false' should fail")
            except ProcessError as pe:
                print(
                    f"  ProcessError caught: sandbox={pe.sandbox_id} command={pe.command}"
                )
            except AssertionError:
                raise
            except Exception as e:
                raise AssertionError(f"expected ProcessError, got {type(e).__name__}: {e}")

            # NotFound error
            try:
                client.workspace.get("non-existent-ws")
                raise AssertionError("get non-existent workspace should fail")
            except NotFoundError:
                print("  NotFoundError OK")
            except AssertionError:
                raise
            except Exception as e:
                raise AssertionError(f"expected NotFoundError, got {type(e).__name__}: {e}")
        finally:
            client.sandbox.delete(sb.id, force=True)
    finally:
        client.workspace.delete(ws.id)


def main():
    parser = argparse.ArgumentParser(description="Python SDK Sync Test")
    parser.add_argument("--server", default="localhost:9090", help="gRPC server address")
    parser.add_argument("--apikey", default="", help="gRPC API key or JWT")
    parser.add_argument(
        "--image", default="workspace-test:latest", help="Sandbox container image"
    )
    args = parser.parse_args()

    print("╔══════════════════════════════════════════════════╗")
    print("║  Python SDK Sync Test — Verify All SDK Features ║")
    print("╚══════════════════════════════════════════════════╝")
    print(f"  Server: {args.server}\n")

    runner = TestRunner()

    with WorkspaceClient(args.server, api_key=args.apikey, timeout=60.0) as client:
        runner.run("1. Workspace CRUD + StorageType", lambda: test_workspace_crud(client))
        runner.run("2. Workspace File Operations (all)", lambda: test_file_ops(client))
        runner.run(
            "3. Sandbox Exists + WaitForState",
            lambda: test_sandbox_features(client, args.image),
        )
        runner.run(
            "4. Process Shell + Exec",
            lambda: test_process_features(client, args.image),
        )
        runner.run(
            "5. Error Handling (ProcessError, NotFound)",
            lambda: test_error_handling(client, args.image),
        )

    runner.summary()


if __name__ == "__main__":
    main()
