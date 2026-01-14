#!/usr/bin/env python3
"""
Example: Streaming Command Output

This example demonstrates streaming command output using SSE.

Run: python streaming.py
"""

import sys
from workspace_sdk import (
    WorkspaceClient,
    CreateSandboxParams,
    RunCommandOptions,
)


def main():
    print("=== Streaming Output Example ===\n")

    with WorkspaceClient("http://localhost:8080", timeout=120.0) as client:
        # Create sandbox
        print("1. Creating sandbox...")
        sandbox = client.sandbox.create(CreateSandboxParams(
            template="workspace-test:latest",
        ))
        print(f"   Created: {sandbox.id}\n")

        try:
            # 2. Stream a simple loop
            print("2. Streaming loop output (sync)...")
            print("   Output: ", end="", flush=True)

            for event in client.process.run_stream(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", """
                    for i in 1 2 3 4 5; do
                        echo "Line $i"
                        sleep 0.5
                    done
                """])
            ):
                if event.type == "stdout":
                    # Print each line as it arrives
                    print(event.data, end="", flush=True)
                elif event.type == "stderr":
                    print(event.data, end="", file=sys.stderr, flush=True)
                elif event.type == "exit":
                    print(f"\n   Exited with code: {event.code}\n")
                    break
                elif event.type == "error":
                    print(f"\n   Error: {event.message}\n")
                    break

            # 3. Stream with interleaved stdout/stderr
            print("3. Streaming with stdout and stderr...")
            print("   Output:")

            for event in client.process.run_stream(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", """
                    echo "This goes to stdout"
                    echo "This goes to stderr" >&2
                    echo "Back to stdout"
                    echo "More stderr" >&2
                """])
            ):
                if event.type == "stdout":
                    print(f"   [stdout] {event.data.strip()}")
                elif event.type == "stderr":
                    print(f"   [stderr] {event.data.strip()}")
                elif event.type == "exit":
                    print(f"   Exit code: {event.code}\n")
                    break

            # 4. Stream a longer running command
            print("4. Streaming progress updates...")
            print("   ", end="", flush=True)

            for event in client.process.run_stream(
                sandbox.id,
                "bash",
                RunCommandOptions(args=["-c", """
                    for i in $(seq 1 10); do
                        echo -n "."
                        sleep 0.2
                    done
                    echo " Done!"
                """])
            ):
                if event.type == "stdout":
                    print(event.data, end="", flush=True)
                elif event.type == "exit":
                    print(f"   (exit: {event.code})")
                    break

        finally:
            # Cleanup
            print("\n5. Cleaning up...")
            client.sandbox.delete(sandbox.id, force=True)
            print("   Done!")


if __name__ == "__main__":
    main()
