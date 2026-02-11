/**
 * Example: Concurrent Operations
 *
 * This example demonstrates running multiple sandboxes and
 * commands concurrently using Promise.all.
 *
 * Run: npx ts-node concurrent.ts
 */

import { WorkspaceClient, Sandbox } from '../src';

async function main() {
  console.log('=== Concurrent Operations Example ===\n');

  const client = new WorkspaceClient('localhost:9090');

  try {
    // 1. Create workspace first
    console.log('1. Creating workspace...');
    const workspace = await client.workspace.create({ name: 'concurrent-example' });
    console.log(`   Created workspace: ${workspace.id}\n`);

    // 2. Create multiple sandboxes concurrently
    const numSandboxes = 3;
    console.log(`2. Creating ${numSandboxes} sandboxes concurrently...`);
    const start1 = Date.now();

    const sandboxes = await Promise.all(
      Array.from({ length: numSandboxes }, async (_, i) => {
        const sandbox = await client.sandbox.create({
          workspaceId: workspace.id,
          template: 'workspace-test:latest',
          name: `concurrent-sandbox-${i}`,
        });
        console.log(`   Sandbox ${i} created: ${sandbox.id}`);
        return sandbox;
      })
    );

    const elapsed1 = (Date.now() - start1) / 1000;
    console.log(`   All sandboxes created in ${elapsed1.toFixed(2)}s\n`);

    try {
      // 3. Run commands in all sandboxes concurrently
      console.log('3. Running commands in all sandboxes concurrently...');
      const start2 = Date.now();

      const results = await Promise.all(
        sandboxes.map(async (sandbox, i) => {
          const result = await client.process.run(sandbox.id, 'bash', {
            args: ['-c', `echo 'Hello from sandbox ${i}' && sleep 1`],
          });
          return { index: i, result };
        })
      );

      const elapsed2 = (Date.now() - start2) / 1000;
      console.log(`   All commands completed in ${elapsed2.toFixed(2)}s\n`);

      // 4. Print results
      console.log('4. Results:');
      for (const { index, result } of results) {
        console.log(`   Sandbox ${index}: ${result.stdout.trim()}`);
      }
      console.log();

      // 5. Run multiple commands in single sandbox
      console.log('5. Running 10 commands concurrently in single sandbox...');
      const start3 = Date.now();

      const cmdResults = await Promise.all(
        Array.from({ length: 10 }, async (_, i) => {
          const result = await client.process.run(sandboxes[0].id, 'bash', {
            args: ['-c', `echo 'Command ${i}' && sleep 0.5`],
          });
          return { index: i, output: result.stdout.trim() };
        })
      );

      const elapsed3 = (Date.now() - start3) / 1000;
      console.log(`   10 commands completed in ${elapsed3.toFixed(2)}s (concurrent)\n`);

      // Verify results
      const outputs = cmdResults
        .sort((a, b) => a.index - b.index)
        .map((r) => r.output);
      console.log(`   Results: ${JSON.stringify(outputs)}`);
    } finally {
      // 6. Cleanup sandboxes concurrently
      console.log('\n6. Cleaning up sandboxes...');
      const start4 = Date.now();

      await Promise.all(
        sandboxes.map(async (sandbox, i) => {
          await client.sandbox.delete(sandbox.id, true);
          console.log(`   Deleted sandbox ${i}`);
        })
      );

      const elapsed4 = (Date.now() - start4) / 1000;
      console.log(`   All sandboxes deleted in ${elapsed4.toFixed(2)}s`);

      // Cleanup workspace
      console.log('7. Cleaning up workspace...');
      await client.workspace.delete(workspace.id);
      console.log('   Done!');
    }
  } finally {
    client.close();
  }
}

main().catch(console.error);
