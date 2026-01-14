#!/usr/bin/env python3
"""
Example: Async Client Usage

This example demonstrates using the asynchronous client for
sandbox and process operations.

Run: python async_example.py
"""

import asyncio
from workspace_sdk import (
    AsyncWorkspaceClient,
    CreateSandboxParams,
    RunCommandOptions,
)


async def main():
    print("=== Async Workspace SDK Example ===\n")

    async with AsyncWorkspaceClient("http://localhost:8080", timeout=60.0) as client:
        # 1. Create a sandbox
        print("1. Creating sandbox...")
        sandbox = await client.sandbox.create(CreateSandboxParams(
            template="workspace-test:latest",
            name="async-example-sandbox",
        ))
        print(f"   Created: {sandbox.id} (state: {sandbox.state})\n")

        try:
            # 2. Run multiple commands concurrently
            print("2. Running multiple commands concurrently...")

            async def run_command(idx: int):
                result = await client.process.run(
                    sandbox.id,
                    "bash",
                    RunCommandOptions(args=["-c", f"echo 'Task {idx}' && sleep 1"])
                )
                return idx, result.stdout.strip()

            # Run 5 tasks concurrently
            tasks = [run_command(i) for i in range(5)]
            results = await asyncio.gather(*tasks)

            for idx, output in results:
                print(f"   Task {idx}: {output}")
            print()

            # 3. Run command with timeout using asyncio
            print("3. Running command with asyncio timeout...")
            try:
                async with asyncio.timeout(2.0):
                    result = await client.process.run(
                        sandbox.id,
                        "bash",
                        RunCommandOptions(args=["-c", "sleep 5 && echo done"])
                    )
                    print(f"   Output: {result.stdout}")
            except asyncio.TimeoutError:
                print("   Command timed out (as expected)\n")

            # 4. Check sandbox state
            print("4. Checking sandbox state...")
            sandbox_info = await client.sandbox.get(sandbox.id)
            print(f"   State: {sandbox_info.state}")
            print(f"   Template: {sandbox_info.template}\n")

            # 5. List all sandboxes
            print("5. Listing all sandboxes...")
            sandboxes = await client.sandbox.list()
            print(f"   Found {len(sandboxes)} sandbox(es):")
            for s in sandboxes:
                print(f"   - {s.id} ({s.state})")

        finally:
            # 6. Cleanup
            print("\n6. Cleaning up...")
            await client.sandbox.delete(sandbox.id, force=True)
            print("   Done!")


if __name__ == "__main__":
    asyncio.run(main())
