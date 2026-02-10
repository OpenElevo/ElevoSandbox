/**
 * Full SDK test for Elevo Workspace TypeScript SDK.
 *
 * This script tests all major SDK functionality including:
 * - Health check
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
 *   --server <url>  HTTP server URL (default: http://localhost:8080)
 *   --grpc <url>    gRPC server URL (default: derived from server)
 *   --token <token> FUSE API token (default: test-token)
 *
 * Example:
 *   npx ts-node examples/test_full.ts --server http://localhost:8080
 */

import * as fs from 'fs';
import * as path from 'path';
import { WorkspaceClient } from '../src/client';
import { FuseService } from '../src/services/fuse';

// Parse command line arguments
function parseArgs(): { server: string; grpc: string; token: string } {
  const args = process.argv.slice(2);
  let server = 'http://localhost:8080';
  let grpc = '';
  let token = 'test-token';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--server' && args[i + 1]) {
      server = args[++i];
    } else if (args[i] === '--grpc' && args[i + 1]) {
      grpc = args[++i];
    } else if (args[i] === '--token' && args[i + 1]) {
      token = args[++i];
    }
  }

  // Derive gRPC URL from HTTP URL if not specified
  if (!grpc) {
    grpc = server.replace(':8080', ':9090').replace(':8081', ':9090');
  }

  return { server, grpc, token };
}

async function testHealth(client: WorkspaceClient): Promise<void> {
  console.log('1. Health check...');
  const health = await client.health();
  console.log(`   Status: ${health.status}, Version: ${health.version}`);
  console.log('   OK\n');
}

async function testWorkspace(client: WorkspaceClient): Promise<string> {
  console.log('2. Creating workspace...');
  const workspace = await client.workspace.create({ name: 'ts-sdk-test' });
  console.log(`   Created workspace: ${workspace.id}\n`);
  return workspace.id;
}

async function testSandbox(client: WorkspaceClient, workspaceId: string): Promise<string> {
  console.log('3. Creating sandbox...');
  const sandbox = await client.sandbox.create({
    workspaceId: workspaceId,
    name: 'ts-sdk-test-sandbox',
  });
  console.log(`   Created sandbox: ${sandbox.id} (state: ${sandbox.state})\n`);
  return sandbox.id;
}

async function testCommand(client: WorkspaceClient, sandboxId: string): Promise<void> {
  console.log('4. Running command...');
  const result = await client.process.run(sandboxId, 'echo', {
    args: ['Hello', 'from', 'TypeScript', 'SDK!'],
  });
  console.log(`   Output: ${result.stdout}`);
  console.log('   OK\n');
}

async function testFileOperations(client: WorkspaceClient, sandboxId: string): Promise<void> {
  console.log('5. File operations via run...');
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
  console.log('6. Listing workspace directory...');
  const lsResult = await client.process.run(sandboxId, 'ls', {
    args: ['-la', '/workspace'],
  });
  console.log(`   Directory listing:\n${lsResult.stdout}`);
  console.log('   OK\n');
}

async function testFuse(grpcUrl: string, httpUrl: string, token: string, workspaceId: string): Promise<void> {
  console.log('7. Testing FUSE mount...');

  if (!FuseService.isAvailable()) {
    console.log('   FUSE not available on this system, skipping...');
    return;
  }

  console.log('   Creating FUSE service...');
  const fuseService = new FuseService(grpcUrl, token, 'latest', undefined, httpUrl);

  console.log('   Mounting workspace...');
  const mount = await fuseService.mount(workspaceId);
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
  const { server, grpc, token } = parseArgs();

  console.log('=== TypeScript SDK Test ===');
  console.log(`Server: ${server}`);
  console.log(`gRPC: ${grpc}\n`);

  const client = new WorkspaceClient({
    apiUrl: server,
    timeout: 60000,
  });

  let workspaceId: string | null = null;
  let sandboxId: string | null = null;

  try {
    await testHealth(client);
    workspaceId = await testWorkspace(client);
    sandboxId = await testSandbox(client, workspaceId);
    await testCommand(client, sandboxId);
    await testFileOperations(client, sandboxId);
    await testDirectoryListing(client, sandboxId);
    await testFuse(grpc, server, token, workspaceId);

    console.log('=== All tests passed! ===');
  } finally {
    await cleanup(client, sandboxId, workspaceId);
  }
}

main().catch((e) => {
  console.error('Error:', e);
  process.exit(1);
});
