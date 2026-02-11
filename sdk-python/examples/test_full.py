#!/usr/bin/env python3
"""
Full SDK test for Elevo Workspace Python SDK.

This script tests all major SDK functionality including:
- Workspace CRUD
- Sandbox management
- Command execution
- File operations
- FUSE mounting

Prerequisites:
    pip install -e .  # Install SDK in development mode

Usage:
    python examples/test_full.py [--server ADDR] [--token TOKEN]

Example:
    python examples/test_full.py --server localhost:9090
"""

import argparse
import os
import sys

# Add parent directory to path for development
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'src'))

from workspace_sdk import WorkspaceClient
from workspace_sdk.services.fuse import FuseService
from workspace_sdk.types import CreateWorkspaceParams, CreateSandboxParams, RunCommandOptions


def test_workspace(client: WorkspaceClient) -> str:
    """Test workspace creation."""
    print("1. Creating workspace...")
    workspace = client.workspace.create(CreateWorkspaceParams(name="python-sdk-test"))
    print(f"   Created workspace: {workspace.id}\n")
    return workspace.id


def test_sandbox(client: WorkspaceClient, workspace_id: str) -> str:
    """Test sandbox creation."""
    print("2. Creating sandbox...")
    sandbox = client.sandbox.create(CreateSandboxParams(
        workspace_id=workspace_id,
        name="python-sdk-test-sandbox",
        template="workspace-test:latest"
    ))
    print(f"   Created sandbox: {sandbox.id} (state: {sandbox.state})\n")
    return sandbox.id


def test_command(client: WorkspaceClient, sandbox_id: str) -> None:
    """Test command execution."""
    print("3. Running command...")
    result = client.process.run(
        sandbox_id,
        "echo",
        RunCommandOptions(args=["Hello", "from", "Python", "SDK!"])
    )
    print(f"   Output: {result.stdout}")
    print("   OK\n")


def test_shell(client: WorkspaceClient, sandbox_id: str) -> None:
    """Test shell operations."""
    print("4. File operations via shell...")
    result = client.process.shell(
        sandbox_id,
        'echo "Hello from Python SDK" > /workspace/test.txt && cat /workspace/test.txt'
    )
    print(f"   File content: {result.stdout}")
    print("   OK\n")


def test_directory_listing(client: WorkspaceClient, sandbox_id: str) -> None:
    """Test directory listing."""
    print("5. Listing workspace directory...")
    result = client.process.run(sandbox_id, "ls", RunCommandOptions(args=["-la", "/workspace"]))
    print(f"   Directory listing:\n{result.stdout}")
    print("   OK\n")


def test_fuse(grpc_url: str, token: str, workspace_id: str) -> None:
    """Test FUSE mounting."""
    print("6. Testing FUSE mount...")

    if not FuseService.is_available():
        print("   FUSE not available on this system, skipping...")
        return

    if not token:
        print("   No token provided, skipping FUSE test (use --token to enable)...")
        return

    print("   Creating FUSE service...")
    fuse_service = FuseService(
        server=grpc_url,
        default_token=token if token else None,
    )

    print("   Mounting workspace...")
    mount_kwargs = {}
    if token:
        mount_kwargs["token"] = token
    with fuse_service.mount(workspace_id, **mount_kwargs) as mount:
        mount_point = mount.mount()
        print(f"   Mounted at: {mount_point}")

        # Write file via FUSE
        print("   Writing file via FUSE...")
        test_content = "Hello from Python SDK via FUSE!"
        test_file = os.path.join(mount_point, "fuse_test.txt")
        with open(test_file, "w") as f:
            f.write(test_content)
        print("   Write OK")

        # Read file via FUSE
        print("   Reading file via FUSE...")
        with open(test_file, "r") as f:
            content = f.read()
        print(f"   Content: {content}")

        # Verify content
        assert content == test_content, f"Content mismatch: expected {test_content!r}, got {content!r}"
        print("   Content verified OK")

        # List directory via FUSE
        print("   Listing directory via FUSE...")
        entries = os.listdir(mount_point)
        print(f"   Files: {', '.join(entries)}")

        print("   Unmounting...")

    print("   FUSE test OK\n")


def cleanup(client: WorkspaceClient, sandbox_id: str, workspace_id: str) -> None:
    """Clean up resources."""
    print("\n--- Cleanup ---")

    if sandbox_id:
        print("Deleting sandbox...")
        try:
            client.sandbox.delete(sandbox_id, force=True)
            print("   OK")
        except Exception as e:
            print(f"   Warning: {e}")

    if workspace_id:
        print("Deleting workspace...")
        try:
            client.workspace.delete(workspace_id)
            print("   OK")
        except Exception as e:
            print(f"   Warning: {e}")


def main():
    parser = argparse.ArgumentParser(description="Test Elevo Workspace Python SDK")
    parser.add_argument("--server", default="localhost:9090", help="gRPC server address")
    parser.add_argument("--token", default="", help="FUSE API token (optional)")
    args = parser.parse_args()

    print("=== Python SDK Test ===")
    print(f"Server: {args.server}\n")

    workspace_id = None
    sandbox_id = None

    with WorkspaceClient(args.server) as client:
        try:
            workspace_id = test_workspace(client)
            sandbox_id = test_sandbox(client, workspace_id)
            test_command(client, sandbox_id)
            test_shell(client, sandbox_id)
            test_directory_listing(client, sandbox_id)
            test_fuse(args.server, args.token, workspace_id)

            print("=== All tests passed! ===")
        finally:
            cleanup(client, sandbox_id, workspace_id)


if __name__ == "__main__":
    main()
