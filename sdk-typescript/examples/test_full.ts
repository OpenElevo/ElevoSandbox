/**
 * Full SDK test for Elevo Workspace TypeScript SDK.
 *
 * This script tests all major SDK functionality including:
 * - Workspace CRUD
 * - Sandbox management
 * - Command execution
 * - File operations
 * - FUSE mounting
 *
 * Usage:
 *   npx ts-node examples/test_full.ts [options]
 *
 * Options:
 *   --server <addr>  gRPC server address (default: localhost:9090)
 *   --token <token>  FUSE API token (default: test-token)
 *
 * Example:
 *   npx ts-node examples/test_full.ts --server localhost:9090
 */

import * as fs from 'fs';
import * as path from 'path';
import { WorkspaceClient } from '../src/client';
import { FuseService } from '../src/services/fuse';

// Parse command line arguments
function parseArgs(): { server: string; token: string } {
  const args = process.argv.slice(2);
  let server = 'localhost:9090';
  let token = '';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--server' && args[i + 1]) {
      server = args[++i];
    } else if (args[i] === '--token' && args[i + 1]) {
      token = args[++i];
    }
  }

  return { server, token };
}

async function testWorkspace(client: WorkspaceClient): Promise<string> {
  console.log('1. Creating workspace...');
  const workspace = await client.workspace.create({ name: 'ts-sdk-test' });
  console.log(`   Created workspace: ${workspace.id}\n`);
  return workspace.id;
}

async function testSandbox(client: WorkspaceClient, workspaceId: string): Promise<string> {
  console.log('2. Creating sandbox...');
  const sandbox = await client.sandbox.create({
    workspaceId: workspaceId,
    name: 'ts-sdk-test-sandbox',
  });
  console.log(`   Created sandbox: ${sandbox.id} (state: ${sandbox.state})\n`);
  return sandbox.id;
}

async function testCommand(client: WorkspaceClient, sandboxId: string): Promise<void> {
  console.log('3. Running command...');
  const result = await client.process.run(sandboxId, 'echo', {
    args: ['Hello', 'from', 'TypeScript', 'SDK!'],
  });
  console.log(`   Output: ${result.stdout}`);
  console.log('   OK\n');
}

async function testFileOperations(client: WorkspaceClient, sandboxId: string): Promise<void> {
  console.log('4. File operations via run...');
  await client.process.run(sandboxId, 'sh', {
    args: ['-c', 'echo "Hello from TypeScript SDK" > /workspace/test.txt'],
  });
  const catResult = await client.process.run(sandboxId, 'cat', {
    args: ['/workspace/test.txt'],
  });
  console.log(`   File content: ${catResult.stdout}`);
  console.log('   OK\n');
}

async function testDirectoryListing(client: WorkspaceClient, sandboxId: string): Promise<void> {
  console.log('5. Listing workspace directory...');
  const lsResult = await client.process.run(sandboxId, 'ls', {
    args: ['-la', '/workspace'],
  });
  console.log(`   Directory listing:\n${lsResult.stdout}`);
  console.log('   OK\n');
}

async function testStreaming(client: WorkspaceClient, sandboxId: string): Promise<void> {
  console.log('6. Testing streaming output...');
  process.stdout.write('   Output: ');

  for await (const event of client.process.runStream(sandboxId, 'bash', {
    args: ['-c', 'for i in 1 2 3; do echo -n "$i "; sleep 0.2; done; echo "done"'],
  })) {
    switch (event.type) {
      case 'stdout':
        process.stdout.write(event.data);
        break;
      case 'exit':
        console.log(`\n   Exit code: ${event.code}`);
        break;
    }
  }
  console.log('   OK\n');
}

async function testFuse(serverAddr: string, token: string, workspaceId: string): Promise<void> {
  console.log('7. Testing FUSE mount...');

  if (!FuseService.isAvailable()) {
    console.log('   FUSE not available on this system, skipping...');
    return;
  }

  console.log('   Creating FUSE service...');
  const fuseService = new FuseService(serverAddr, token || undefined);

  console.log('   Mounting workspace...');
  const mountOptions: { token?: string } = {};
  if (token) {
    mountOptions.token = token;
  }
  const mount = await fuseService.mount(workspaceId, mountOptions);
  const mountPoint = await mount.mount();
  console.log(`   Mounted at: ${mountPoint}`);

  try {
    // Write file via FUSE
    console.log('   Writing file via FUSE...');
    const testContent = 'Hello from TypeScript SDK via FUSE!';
    const testFile = path.join(mountPoint, 'fuse_test.txt');
    fs.writeFileSync(testFile, testContent);
    console.log('   Write OK');

    // Read file via FUSE
    console.log('   Reading file via FUSE...');
    const content = fs.readFileSync(testFile, 'utf-8');
    console.log(`   Content: ${content}`);

    // Verify content
    if (content !== testContent) {
      throw new Error(`Content mismatch: expected "${testContent}", got "${content}"`);
    }
    console.log('   Content verified OK');

    // List directory via FUSE
    console.log('   Listing directory via FUSE...');
    const entries = fs.readdirSync(mountPoint);
    console.log(`   Files: ${entries.join(', ')}`);

    console.log('   FUSE test OK');
  } finally {
    console.log('   Unmounting...');
    mount.unmount();
    console.log('   Unmounted OK\n');
  }
}

async function cleanup(client: WorkspaceClient, sandboxId: string | null, workspaceId: string | null): Promise<void> {
  console.log('\n--- Cleanup ---');

  if (sandboxId) {
    console.log('Deleting sandbox...');
    try {
      await client.sandbox.delete(sandboxId);
      console.log('   OK');
    } catch (e: any) {
      console.log(`   Warning: ${e.message}`);
    }
  }

  if (workspaceId) {
    console.log('Deleting workspace...');
    try {
      await client.workspace.delete(workspaceId);
      console.log('   OK');
    } catch (e: any) {
      console.log(`   Warning: ${e.message}`);
    }
  }
}

async function main() {
  const { server, token } = parseArgs();

  console.log('=== TypeScript SDK Test ===');
  console.log(`Server: ${server}\n`);

  const client = new WorkspaceClient(server);

  let workspaceId: string | null = null;
  let sandboxId: string | null = null;

  try {
    workspaceId = await testWorkspace(client);
    sandboxId = await testSandbox(client, workspaceId);
    await testCommand(client, sandboxId);
    await testFileOperations(client, sandboxId);
    await testDirectoryListing(client, sandboxId);
    await testStreaming(client, sandboxId);
    await testFuse(server, token, workspaceId);

    console.log('=== All tests passed! ===');
  } finally {
    await cleanup(client, sandboxId, workspaceId);
    client.close();
  }
}

main().catch((e) => {
  console.error('Error:', e);
  process.exit(1);
});
