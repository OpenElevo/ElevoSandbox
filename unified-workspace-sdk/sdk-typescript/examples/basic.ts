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

  // 1. Create a sandbox
  console.log('1. Creating sandbox...');
  const sandbox = await client.sandbox.create({
    template: 'workspace-test:latest',
    name: 'example-sandbox',
    metadata: { purpose: 'demo' },
  });
  console.log(`   Created: ${sandbox.id} (state: ${sandbox.state})\n`);

  try {
    // 2. Run a simple command
    console.log('2. Running echo command...');
    let result = await client.process.run(sandbox.id, 'echo', {
      args: ['Hello', 'from', 'TypeScript', 'SDK!'],
    });
    console.log(`   Output: ${result.stdout}`);

    // 3. Run command with environment variables
    console.log('3. Running command with environment variables...');
    result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'echo "User: $USER, App: $APP_NAME"'],
      env: { USER: 'developer', APP_NAME: 'MyApp' },
    });
    console.log(`   Output: ${result.stdout}`);

    // 4. Write and read a file
    console.log('4. Writing and reading a file...');
    result = await client.process.run(sandbox.id, 'bash', {
      args: [
        '-c',
        `echo '{"name": "test", "version": "1.0.0"}' > /workspace/config.json && cat /workspace/config.json`,
      ],
    });
    console.log(`   File content: ${result.stdout}`);

    // 5. List workspace directory
    console.log('5. Listing workspace directory...');
    result = await client.process.run(sandbox.id, 'ls', {
      args: ['-la', '/workspace'],
    });
    console.log(`   Directory listing:\n${result.stdout}`);
  } finally {
    // 6. Cleanup
    console.log('\n6. Cleaning up...');
    await client.sandbox.delete(sandbox.id, true);
    console.log('   Done!');
  }
}

main().catch(console.error);
