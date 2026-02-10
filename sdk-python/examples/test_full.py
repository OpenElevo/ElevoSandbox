#!/usr/bin/env python3
"""
Full SDK test for Elevo Workspace Python SDK.

This script tests all major SDK functionality including:
- Health check
- Workspace CRUD
- Sandbox management
- Command execution
- File operations
- FUSE mounting

Prerequisites:
    pip install -e .  # Install SDK in development mode

Usage:
    python examples/test_full.py [--server URL] [--grpc URL] [--token TOKEN]

Example:
    python examples/test_full.py --server http://localhost:8080
"""

import argparse
import os
import sys
import tempfile

# Add parent directory to path for development
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'src'))

from workspace_sdk import WorkspaceClient
from workspace_sdk.services.fuse import FuseService


def test_health(client: WorkspaceClient) -> None:
    """Test health check."""
    print("1. Health check...")
    health = client.health()
    print(f"   Status: {health['status']}, Version: {health.get('version', 'N/A')}")
    print("   OK\n")


def test_workspace(client: WorkspaceClient) -> str:
    """Test workspace creation."""
    print("2. Creating workspace...")
    workspace = client.workspaces.create(name="python-sdk-test")
    print(f"   Created workspace: {workspace.id}\n")
    return workspace.id


def test_sandbox(client: WorkspaceClient, workspace_id: str) -> str:
    """Test sandbox creation."""
    print("3. Creating sandbox...")
    sandbox = client.sandboxes.create(
        workspace_id=workspace_id,
        name="python-sdk-test-sandbox"
    )
    print(f"   Created sandbox: {sandbox.id} (state: {sandbox.state})\n")
    return sandbox.id


def test_command(client: WorkspaceClient, sandbox_id: str) -> None:
    """Test command execution."""
    print("4. Running command...")
    result = client.process.run(
        sandbox_id,
        "echo",
        args=["Hello", "from", "Python", "SDK!"]
    )
    print(f"   Output: {result.stdout}")
    print("   OK\n")


def test_shell(client: WorkspaceClient, sandbox_id: str) -> None:
    """Test shell operations."""
    print("5. File operations via shell...")
    result = client.process.shell(
        sandbox_id,
        '''
        echo "Hello from Python SDK" > /workspace/test.txt
        cat /workspace/test.txt
        '''
    )
    print(f"   File content: {result.stdout}")
    print("   OK\n")


def test_directory_listing(client: WorkspaceClient, sandbox_id: str) -> None:
    """Test directory listing."""
    print("6. Listing workspace directory...")
    result = client.process.run(sandbox_id, "ls", args=["-la", "/workspace"])
    print(f"   Directory listing:\n{result.stdout}")
    print("   OK\n")


def test_fuse(grpc_url: str, http_url: str, token: str, workspace_id: str) -> None:
    """Test FUSE mounting."""
    print("7. Testing FUSE mount...")

    if not FuseService.is_available():
        print("   FUSE not available on this system, skipping...")
        return

    print("   Creating FUSE service...")
    fuse_service = FuseService(
        server=grpc_url,
        default_token=token,
        http_server=http_url
    )

    print("   Mounting workspace...")
    with fuse_service.mount(workspace_id, token=token) as mount:
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
            client.sandboxes.delete(sandbox_id, force=True)
            print("   OK")
        except Exception as e:
            print(f"   Warning: {e}")

    if workspace_id:
        print("Deleting workspace...")
        try:
            client.workspaces.delete(workspace_id)
            print("   OK")
        except Exception as e:
            print(f"   Warning: {e}")


def main():
    parser = argparse.ArgumentParser(description="Test Elevo Workspace Python SDK")
    parser.add_argument("--server", default="http://localhost:8080", help="HTTP server URL")
    parser.add_argument("--grpc", default=None, help="gRPC server URL (default: derived from server)")
    parser.add_argument("--token", default="test-token", help="FUSE API token")
    args = parser.parse_args()

    # Derive gRPC URL from HTTP URL if not specified
    grpc_url = args.grpc
    if not grpc_url:
        grpc_url = args.server.replace(":8080", ":9090").replace(":8081", ":9090")

    print("=== Python SDK Test ===")
    print(f"Server: {args.server}")
    print(f"gRPC: {grpc_url}\n")

    client = WorkspaceClient(args.server)

    workspace_id = None
    sandbox_id = None

    try:
        test_health(client)
        workspace_id = test_workspace(client)
        sandbox_id = test_sandbox(client, workspace_id)
        test_command(client, sandbox_id)
        test_shell(client, sandbox_id)
        test_directory_listing(client, sandbox_id)
        test_fuse(grpc_url, args.server, args.token, workspace_id)

        print("=== All tests passed! ===")
    finally:
        cleanup(client, sandbox_id, workspace_id)


if __name__ == "__main__":
    main()
