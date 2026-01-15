#!/usr/bin/env python3
"""
Example: Basic Usage

This example demonstrates basic sandbox and process operations
using the synchronous client.

Run: python basic.py
"""

from workspace_sdk import (
    WorkspaceClient,
    CreateSandboxParams,
    RunCommandOptions,
)


def main():
    print("=== Workspace SDK Basic Example ===\n")

    with WorkspaceClient("http://localhost:8080", timeout=60.0) as client:
        # 1. Create a sandbox
        print("1. Creating sandbox...")
        sandbox = client.sandbox.create(CreateSandboxParams(
            template="workspace-test:latest",
            name="example-sandbox",
            metadata={"purpose": "demo"},
        ))
        print(f"   Created: {sandbox.id} (state: {sandbox.state})\n")

        try:
            # 2. Run a simple command
            print("2. Running echo command...")
            result = client.process.run(
                sandbox.id,
                "echo",
                RunCommandOptions(args=["Hello", "from", "Python", "SDK!"])
            )
            print(f"   Output: {result.stdout}")

            # 3. Run command with environment variables
            print("3. Running command with environment variables...")
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(
                    args=["-c", "echo \"User: $USER, App: $APP_NAME\""],
                    env={"USER": "developer", "APP_NAME": "MyApp"},
                )
            )
            print(f"   Output: {result.stdout}")

            # 4. Write and read a file
            print("4. Writing and reading a file...")
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", """
                    echo '{"name": "test", "version": "1.0.0"}' > /workspace/config.json
                    cat /workspace/config.json
                """])
            )
            print(f"   File content: {result.stdout}")

            # 5. List workspace directory
            print("5. Listing workspace directory...")
            result = client.process.run(
                sandbox.id,
                "ls",
                RunCommandOptions(args=["-la", "/workspace"])
            )
            print(f"   Directory listing:\n{result.stdout}")

        finally:
            # 6. Cleanup
            print("\n6. Cleaning up...")
            client.sandbox.delete(sandbox.id, force=True)
            print("   Done!")


if __name__ == "__main__":
    main()
