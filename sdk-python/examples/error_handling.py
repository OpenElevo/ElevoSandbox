#!/usr/bin/env python3
"""
Example: Error Handling

This example demonstrates proper error handling with the SDK.

Run: python error_handling.py
"""

from workspace_sdk import (
    WorkspaceClient,
    CreateSandboxParams,
    RunCommandOptions,
    WorkspaceError,
    SandboxNotFoundError,
    TemplateNotFoundError,
    FileNotFoundError,
)


def main():
    print("=== Error Handling Example ===\n")

    with WorkspaceClient("http://localhost:8080", timeout=30.0) as client:
        # 1. Handle not found error
        print("1. Handling non-existent sandbox error...")
        try:
            client.sandbox.get("non-existent-sandbox-id")
        except SandboxNotFoundError as e:
            print(f"   SandboxNotFoundError: {e.sandbox_id}")
            print(f"   Error code: {e.code}")
        except WorkspaceError as e:
            print(f"   WorkspaceError [{e.code}]: {e.message}")
        print()

        # 2. Create a sandbox for more tests
        print("2. Creating sandbox for error tests...")
        sandbox = client.sandbox.create(CreateSandboxParams(
            template="workspace-test:latest",
        ))
        print(f"   Created: {sandbox.id}\n")

        try:
            # 3. Handle command failure
            print("3. Handling command that returns non-zero exit code...")
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "exit 42"])
            )
            print(f"   Exit code: {result.exit_code} (expected: 42)")
            if result.exit_code == 42:
                print("   Non-zero exit code correctly captured")
            print()

            # 4. Handle command with stderr
            print("4. Handling command that writes to stderr...")
            result = client.process.run(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", "echo 'error message' >&2; exit 1"])
            )
            print(f"   Exit code: {result.exit_code}")
            print(f"   Stderr: {result.stderr.strip()}")
            print()

            # 5. Handle missing file
            print("5. Handling missing file...")
            result = client.process.run(
                sandbox.id,
                "cat",
                RunCommandOptions(args=["/nonexistent/file.txt"])
            )
            if result.exit_code != 0:
                print(f"   Command failed with exit code: {result.exit_code}")
                print(f"   Stderr: {result.stderr.strip()}")
            print()

            # 6. Handle invalid command
            print("6. Handling invalid command...")
            result = client.process.run(
                sandbox.id,
                "nonexistent_command_xyz",
                RunCommandOptions(args=[])
            )
            if result.exit_code != 0:
                print(f"   Command failed with exit code: {result.exit_code}")
            print()

            # 7. Using exception handling pattern
            print("7. Best practice - using exception handling pattern...")

            def safe_run(client, sandbox_id: str, cmd: str, opts: RunCommandOptions):
                """Run command with proper error handling"""
                try:
                    result = client.process.run(sandbox_id, cmd, opts)
                    if result.exit_code != 0:
                        print(f"   Warning: Command '{cmd}' exited with code {result.exit_code}")
                        if result.stderr:
                            print(f"   Stderr: {result.stderr.strip()}")
                        return None
                    return result
                except WorkspaceError as e:
                    print(f"   API Error: {e}")
                    return None

            # Test the helper
            result = safe_run(client, sandbox.id, "bash",
                RunCommandOptions(args=["-c", "exit 1"]))
            print(f"   Result: {result}")
            print()

            # 8. Checking sandbox exists before operations
            print("8. Checking sandbox exists before operations...")
            exists = client.sandbox.exists(sandbox.id)
            print(f"   Sandbox exists: {exists}")

            exists = client.sandbox.exists("fake-sandbox-id")
            print(f"   Fake sandbox exists: {exists}")

        finally:
            # Cleanup
            print("\n9. Cleaning up...")
            client.sandbox.delete(sandbox.id, force=True)
            print("   Done!")


if __name__ == "__main__":
    main()
