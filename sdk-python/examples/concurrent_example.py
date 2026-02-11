#!/usr/bin/env python3
"""
Example: Concurrent Operations

This example demonstrates running multiple sandboxes and
commands concurrently using async client.

Run: python concurrent.py
"""

import asyncio
import time
from workspace_sdk import (
    AsyncWorkspaceClient,
    CreateSandboxParams,
    RunCommandOptions,
)


async def main():
    print("=== Concurrent Operations Example ===\n")

    async with AsyncWorkspaceClient("http://localhost:8080", timeout=120.0) as client:
        # 1. Create multiple sandboxes concurrently
        num_sandboxes = 3
        print(f"1. Creating {num_sandboxes} sandboxes concurrently...")
        start = time.time()

        async def create_sandbox(idx: int):
            sandbox = await client.sandbox.create(CreateSandboxParams(
                template="workspace-test:latest",
                name=f"concurrent-sandbox-{idx}",
            ))
            print(f"   Sandbox {idx} created: {sandbox.id}")
            return sandbox

        sandboxes = await asyncio.gather(*[
            create_sandbox(i) for i in range(num_sandboxes)
        ])

        elapsed = time.time() - start
        print(f"   All sandboxes created in {elapsed:.2f}s\n")

        try:
            # 2. Run commands in all sandboxes concurrently
            print("2. Running commands in all sandboxes concurrently...")
            start = time.time()

            async def run_in_sandbox(idx: int, sandbox):
                result = await client.process.run(
                    sandbox.id,
                    "bash",
                    RunCommandOptions(args=["-c", f"echo 'Hello from sandbox {idx}' && sleep 1"])
                )
                return idx, result

            results = await asyncio.gather(*[
                run_in_sandbox(i, s) for i, s in enumerate(sandboxes)
            ])

            elapsed = time.time() - start
            print(f"   All commands completed in {elapsed:.2f}s\n")

            # 3. Print results
            print("3. Results:")
            for idx, result in results:
                print(f"   Sandbox {idx}: {result.stdout.strip()}")
            print()

            # 4. Run multiple commands in single sandbox
            print("4. Running 10 commands concurrently in single sandbox...")
            start = time.time()

            async def run_command(sandbox_id: str, cmd_idx: int):
                result = await client.process.run(
                    sandbox_id,
                    "bash",
                    RunCommandOptions(args=["-c", f"echo 'Command {cmd_idx}' && sleep 0.5"])
                )
                return cmd_idx, result.stdout.strip()

            cmd_results = await asyncio.gather(*[
                run_command(sandboxes[0].id, i) for i in range(10)
            ])

            elapsed = time.time() - start
            print(f"   10 commands completed in {elapsed:.2f}s (concurrent)\n")

            # Verify results
            print("   Results:", [r[1] for r in sorted(cmd_results)])

        finally:
            # 5. Cleanup sandboxes concurrently
            print("\n5. Cleaning up sandboxes...")
            start = time.time()

            async def delete_sandbox(idx: int, sandbox):
                await client.sandbox.delete(sandbox.id, force=True)
                print(f"   Deleted sandbox {idx}")

            await asyncio.gather(*[
                delete_sandbox(i, s) for i, s in enumerate(sandboxes)
            ])

            elapsed = time.time() - start
            print(f"   All sandboxes deleted in {elapsed:.2f}s")
            print("   Done!")


if __name__ == "__main__":
    asyncio.run(main())
