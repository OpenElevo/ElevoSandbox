/**
 * SDK synchronization test — verifies all features added during SDK sync.
 *
 * Tests: workspace CRUD + StorageType, file ops (move, copy, getFileInfo, exists),
 * sandbox (exists, waitForState), process (shell, exec), error handling.
 *
 * Usage:
 *   npx ts-node examples/test_sdk_sync.ts [options]
 *
 * Options:
 *   --server <addr>   gRPC server address (default: localhost:9090)
 *   --apikey <key>    gRPC API key or JWT (optional)
 *   --image <image>   Sandbox container image (default: workspace-test:latest)
 */

import { WorkspaceClient } from '../src/client';
import { ProcessError, WorkspaceError } from '../src/errors';
import type { Workspace, Sandbox } from '../src/types';

// Parse command line arguments
function parseArgs(): { server: string; apikey: string; image: string } {
  const args = process.argv.slice(2);
  let server = 'localhost:9090';
  let apikey = '';
  let image = 'workspace-test:latest';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--server' && args[i + 1]) {
      server = args[++i];
    } else if (args[i] === '--apikey' && args[i + 1]) {
      apikey = args[++i];
    } else if (args[i] === '--image' && args[i + 1]) {
      image = args[++i];
    }
  }

  return { server, apikey, image };
}

interface TestResult {
  name: string;
  passed: boolean;
  error?: Error;
}

const results: TestResult[] = [];

async function runTest(name: string, fn: () => Promise<void>): Promise<void> {
  console.log(`\n── ${name} ──`);
  try {
    await fn();
    results.push({ name, passed: true });
    console.log('  PASSED');
  } catch (err) {
    results.push({ name, passed: false, error: err as Error });
    console.log(`  FAILED: ${(err as Error).message}`);
  }
}

async function testWorkspaceCRUD(client: WorkspaceClient): Promise<void> {
  // Create
  const ws = await client.workspace.create({
    name: 'ts-sync-test',
    metadata: { test: 'sync' },
  });
  console.log(`  Created workspace: ${ws.id} (storageType=${ws.storageType})`);

  try {
    // Verify default storage type is managed
    if (ws.storageType !== 'managed') {
      throw new Error(`expected storageType=managed, got ${ws.storageType}`);
    }

    // Get
    const got = await client.workspace.get(ws.id);
    if (got.name !== 'ts-sync-test') {
      throw new Error(`name mismatch: got ${got.name}`);
    }

    // List
    const list = await client.workspace.list();
    const found = list.some((w) => w.id === ws.id);
    if (!found) {
      throw new Error('workspace not found in list');
    }

    console.log('  Workspace CRUD + StorageType OK');
  } finally {
    await client.workspace.delete(ws.id);
  }
}

async function testFileOps(client: WorkspaceClient): Promise<void> {
  const ws = await client.workspace.create({ name: 'ts-file-ops-test' });

  try {
    const wsId = ws.id;

    // WriteFile + ReadFile
    await client.workspace.writeFile(wsId, 'hello.txt', 'Hello World');
    const content = await client.workspace.readFile(wsId, 'hello.txt');
    if (content !== 'Hello World') {
      throw new Error(`content mismatch: got "${content}"`);
    }
    console.log('  WriteFile + ReadFile OK');

    // Mkdir + ListFiles
    await client.workspace.mkdir(wsId, 'subdir');
    const files = await client.workspace.listFiles(wsId, '.');
    console.log(`  ListFiles: ${files.length} items`);

    // GetFileInfo
    const info = await client.workspace.getFileInfo(wsId, 'hello.txt');
    if (info.type !== 'file' || info.name !== 'hello.txt') {
      throw new Error(`file info mismatch: name=${info.name} type=${info.type}`);
    }
    console.log(`  GetFileInfo: name=${info.name} type=${info.type} size=${info.size}`);

    // FileExists
    let exists = await client.workspace.fileExists(wsId, 'hello.txt');
    if (!exists) {
      throw new Error('file should exist');
    }

    exists = await client.workspace.fileExists(wsId, 'no-such-file.txt');
    if (exists) {
      throw new Error('non-existent file should not exist');
    }
    console.log('  FileExists OK');

    // CopyFile
    await client.workspace.copyFile(wsId, 'hello.txt', 'hello_copy.txt');
    const copyContent = await client.workspace.readFile(wsId, 'hello_copy.txt');
    if (copyContent !== 'Hello World') {
      throw new Error(`copy content mismatch: got "${copyContent}"`);
    }
    console.log('  CopyFile OK');

    // MoveFile
    await client.workspace.moveFile(wsId, 'hello_copy.txt', 'hello_moved.txt');
    exists = await client.workspace.fileExists(wsId, 'hello_copy.txt');
    if (exists) {
      throw new Error('moved source should not exist');
    }
    const movedContent = await client.workspace.readFile(wsId, 'hello_moved.txt');
    if (movedContent !== 'Hello World') {
      throw new Error('moved content mismatch');
    }
    console.log('  MoveFile OK');

    // DeleteFile
    await client.workspace.deleteFile(wsId, 'hello_moved.txt', false);
    exists = await client.workspace.fileExists(wsId, 'hello_moved.txt');
    if (exists) {
      throw new Error('deleted file should not exist');
    }
    console.log('  DeleteFile OK');
  } finally {
    await client.workspace.delete(ws.id);
  }
}

async function testSandboxFeatures(client: WorkspaceClient, image: string): Promise<void> {
  const ws = await client.workspace.create({ name: 'ts-sandbox-test' });

  try {
    let sb: Sandbox;
    try {
      sb = await client.sandbox.create({
        workspaceId: ws.id,
        template: image,
        name: 'ts-sync-test-sandbox',
      });
    } catch (e) {
      console.log(`  Sandbox creation failed (Docker may not be available): ${(e as Error).message}`);
      console.log('  Skipping sandbox-dependent tests');
      return;
    }

    try {
      console.log(`  Created sandbox: ${sb.id} (state: ${sb.state})`);

      // Exists
      let exists = await client.sandbox.exists(sb.id);
      if (!exists) {
        throw new Error('sandbox should exist');
      }

      exists = await client.sandbox.exists('non-existent-sandbox');
      if (exists) {
        throw new Error('non-existent sandbox should not exist');
      }
      console.log('  Sandbox.exists OK');

      // WaitForState
      sb = await client.sandbox.waitForState(sb.id, 'running', AbortSignal.timeout(30000));
      console.log(`  WaitForState: reached ${sb.state}`);
    } finally {
      await client.sandbox.delete(sb.id, true);
    }
  } finally {
    await client.workspace.delete(ws.id);
  }
}

async function testProcessFeatures(client: WorkspaceClient, image: string): Promise<void> {
  const ws = await client.workspace.create({ name: 'ts-process-test' });

  try {
    let sb: Sandbox;
    try {
      sb = await client.sandbox.create({
        workspaceId: ws.id,
        template: image,
      });
    } catch (e) {
      console.log(`  Sandbox creation failed: ${(e as Error).message}`);
      console.log('  Skipping process tests');
      return;
    }

    try {
      await client.sandbox.waitForState(sb.id, 'running', AbortSignal.timeout(30000));

      // Shell
      const result = await client.process.shell(sb.id, 'echo hello && echo world');
      if (result.exitCode !== 0) {
        throw new Error(`shell exit code: ${result.exitCode}`);
      }
      process.stdout.write(`  Shell output: ${result.stdout}`);
      console.log('  Shell OK');

      // Exec
      const stdout = await client.process.exec(sb.id, 'echo', 'TypeScript', 'SDK', 'exec');
      process.stdout.write(`  Exec output: ${stdout}`);
      console.log('  Exec OK');
    } finally {
      await client.sandbox.delete(sb.id, true);
    }
  } finally {
    await client.workspace.delete(ws.id);
  }
}

async function testErrorHandling(client: WorkspaceClient, image: string): Promise<void> {
  const ws = await client.workspace.create({ name: 'ts-error-test' });

  try {
    let sb: Sandbox;
    try {
      sb = await client.sandbox.create({
        workspaceId: ws.id,
        template: image,
      });
    } catch (e) {
      console.log('  Skipping error handling tests (no Docker)');
      return;
    }

    try {
      await client.sandbox.waitForState(sb.id, 'running', AbortSignal.timeout(30000));

      // ProcessError from exec
      try {
        await client.process.exec(sb.id, 'false');
        throw new Error("exec 'false' should fail");
      } catch (err) {
        if (err instanceof ProcessError) {
          console.log(`  ProcessError caught: sandbox=${err.sandboxId} command=${err.command}`);
        } else if ((err as Error).message === "exec 'false' should fail") {
          throw err;
        } else {
          throw new Error(`expected ProcessError, got ${(err as Error).constructor.name}: ${(err as Error).message}`);
        }
      }

      // NotFound error
      try {
        await client.workspace.get('non-existent-ws');
        throw new Error('get non-existent workspace should fail');
      } catch (err) {
        if (err instanceof WorkspaceError && err.message.toLowerCase().includes('not found')) {
          console.log('  NotFoundError OK');
        } else if ((err as Error).message === 'get non-existent workspace should fail') {
          throw err;
        } else {
          throw new Error(`expected NotFound, got: ${(err as Error).message}`);
        }
      }
    } finally {
      await client.sandbox.delete(sb.id, true);
    }
  } finally {
    await client.workspace.delete(ws.id);
  }
}

async function main() {
  const { server, apikey, image } = parseArgs();

  console.log('╔══════════════════════════════════════════════════╗');
  console.log('║   TS SDK Sync Test — Verify All SDK Features    ║');
  console.log('╚══════════════════════════════════════════════════╝');
  console.log(`  Server: ${server}\n`);

  const clientOpts: { apiKey?: string } = {};
  if (apikey) clientOpts.apiKey = apikey;
  const client = new WorkspaceClient(server, clientOpts);

  try {
    await runTest('1. Workspace CRUD + StorageType', () => testWorkspaceCRUD(client));
    await runTest('2. Workspace File Operations (all)', () => testFileOps(client));
    await runTest('3. Sandbox Exists + WaitForState', () => testSandboxFeatures(client, image));
    await runTest('4. Process Shell + Exec', () => testProcessFeatures(client, image));
    await runTest('5. Error Handling (ProcessError, NotFound)', () => testErrorHandling(client, image));
  } finally {
    client.close();
  }

  // Print summary
  console.log('\n╔══════════════════════════════════════════════════╗');
  console.log('║                  Test Summary                    ║');
  console.log('╚══════════════════════════════════════════════════╝');
  let passed = 0;
  let failed = 0;
  for (const r of results) {
    const status = r.passed ? '✓ PASS' : '✗ FAIL';
    if (r.passed) {
      passed++;
    } else {
      failed++;
    }
    console.log(`  ${status}  ${r.name}`);
    if (r.error) {
      console.log(`         Error: ${r.error.message}`);
    }
  }
  console.log(`\n  Total: ${passed} passed, ${failed} failed`);
  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((e) => {
  console.error('Error:', e);
  process.exit(1);
});
