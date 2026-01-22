/**
 * Example: Basic Usage
 *
 * This example demonstrates basic sandbox and process operations.
 *
 * Run: npx ts-node basic.ts
 */

import { WorkspaceClient } from '../src';

async function main() {
  console.log('=== Workspace SDK Basic Example ===\n');

  const client = new WorkspaceClient({
    apiUrl: 'http://localhost:8080',
    timeout: 60000,
  });

  // 1. Create a workspace
  console.log('1. Creating workspace...');
  const workspace = await client.workspace.create();
  console.log(`   Created: ${workspace.id}\n`);

  // 2. Create a sandbox
  console.log('2. Creating sandbox...');
  const sandbox = await client.sandbox.create({
    workspaceId: workspace.id,
    template: 'workspace-base:latest',
    name: 'example-sandbox',
    metadata: { purpose: 'demo' },
  });
  console.log(`   Created: ${sandbox.id} (state: ${sandbox.state})\n`);

  try {
    // 3. Run a simple command
    console.log('3. Running echo command...');
    let result = await client.process.run(sandbox.id, 'echo', {
      args: ['Hello', 'from', 'TypeScript', 'SDK!'],
    });
    console.log(`   Output: ${result.stdout}`);

    // 4. Run command with environment variables
    console.log('4. Running command with environment variables...');
    result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'echo "User: $USER, App: $APP_NAME"'],
      env: { USER: 'developer', APP_NAME: 'MyApp' },
    });
    console.log(`   Output: ${result.stdout}`);

    // 5. Write and read a file
    console.log('5. Writing and reading a file...');
    result = await client.process.run(sandbox.id, 'bash', {
      args: [
        '-c',
        `echo '{"name": "test", "version": "1.0.0"}' > /workspace/config.json && cat /workspace/config.json`,
      ],
    });
    console.log(`   File content: ${result.stdout}`);

    // 6. List workspace directory
    console.log('6. Listing workspace directory...');
    result = await client.process.run(sandbox.id, 'ls', {
      args: ['-la', '/workspace'],
    });
    console.log(`   Directory listing:\n${result.stdout}`);
  } finally {
    // 7. Cleanup
    console.log('\n7. Cleaning up...');
    await client.sandbox.delete(sandbox.id);
    await client.workspace.delete(workspace.id);
    console.log('   Done!');
  }
}

main().catch(console.error);
