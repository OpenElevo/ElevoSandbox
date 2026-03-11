/**
 * End-to-end test: StorageProvider (Client A) + FUSE mount (Client B).
 *
 * 1. Creates a remote workspace
 * 2. Shares a local directory via StorageProvider
 * 3. Mounts the workspace via FUSE
 * 4. Reads/lists files through the FUSE mount
 * 5. Verifies content matches
 *
 * IMPORTANT: All FUSE filesystem operations use ASYNC (fs.promises) APIs
 * to avoid deadlocking the event loop when the StorageProvider runs in
 * the same Node.js process.
 */
import * as fs from 'fs';
import * as fsp from 'fs/promises';
import * as os from 'os';
import * as path from 'path';
import { WorkspaceClient } from '../src/client';
import { FuseService } from '../src/services/fuse';

const SERVER = process.argv[2] || '127.0.0.1:3201';
const [host, port] = SERVER.split(':');
const HTTP_SERVER = `http://${host}:${parseInt(port) - 1}`;

function sleep(ms: number) {
  return new Promise<void>((r) => setTimeout(r, ms));
}

async function main() {
  console.log(`Server: ${SERVER} (HTTP: ${HTTP_SERVER})`);

  const client = new WorkspaceClient(SERVER);
  const fuseService = new FuseService(
    `http://${SERVER}`, undefined, 'latest', undefined, HTTP_SERVER
  );

  // Setup local dir with test files
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'elevo-test-'));
  fs.writeFileSync(path.join(tmpDir, 'hello.txt'), 'Hello from StorageProvider!');
  fs.mkdirSync(path.join(tmpDir, 'subdir'));
  fs.writeFileSync(path.join(tmpDir, 'subdir', 'nested.txt'), 'Nested file content');
  console.log(`Local dir: ${tmpDir}`);

  // Create remote workspace
  const ws = await client.workspace.create({ name: 'test-remote-fuse', storageType: 'remote' });
  console.log(`Remote workspace: ${ws.id}`);

  // Start StorageProvider
  const ac = new AbortController();
  const provider = client.newStorageProvider({ localDir: tmpDir, workspaceId: ws.id, token: 'test' });
  const sharePromise = provider.share(ac.signal).catch(() => {});
  for (let i = 0; i < 50; i++) {
    await sleep(200);
    if (provider.isConnected()) break;
  }
  if (!provider.isConnected()) {
    throw new Error('StorageProvider failed to connect');
  }
  console.log('StorageProvider connected');

  // Verify via API first
  console.log('\n--- API verification ---');
  const apiContent = await client.workspace.readFile(ws.id, 'hello.txt');
  console.log(`API readFile: "${apiContent}"`);
  const apiFiles = await client.workspace.listFiles(ws.id, '');
  console.log(`API listFiles: [${apiFiles.map(f => f.name).join(', ')}]`);

  // Mount FUSE
  console.log('\n--- Mounting FUSE ---');
  const mount = await fuseService.mount(ws.id, { debug: true });
  const mountPoint = await mount.mount(30000);
  console.log(`FUSE mounted at: ${mountPoint}`);
  await sleep(1000);

  // Test FUSE operations — ALL use async fs.promises to avoid deadlocking
  // the event loop (StorageProvider needs the event loop to process gRPC messages)
  let passed = 0;
  let failed = 0;

  // Test 1: stat root
  console.log('\n--- Test 1: stat root ---');
  const t1 = Date.now();
  try {
    const stat = await fsp.stat(mountPoint);
    const elapsed = Date.now() - t1;
    console.log(`PASS: stat root ${elapsed}ms isDir=${stat.isDirectory()}`);
    passed++;
  } catch (e: any) {
    console.log(`FAIL: stat root after ${Date.now() - t1}ms: ${e.message}`);
    failed++;
  }

  // Test 2: readdir root
  console.log('\n--- Test 2: readdir root ---');
  const t2 = Date.now();
  try {
    const entries = await fsp.readdir(mountPoint);
    const elapsed = Date.now() - t2;
    console.log(`PASS: readdir ${elapsed}ms entries=[${entries.join(', ')}]`);
    passed++;
  } catch (e: any) {
    console.log(`FAIL: readdir after ${Date.now() - t2}ms: ${e.message}`);
    failed++;
  }

  // Test 3: read hello.txt
  console.log('\n--- Test 3: read hello.txt ---');
  const t3 = Date.now();
  try {
    const content = await fsp.readFile(path.join(mountPoint, 'hello.txt'), 'utf-8');
    const elapsed = Date.now() - t3;
    if (content === 'Hello from StorageProvider!') {
      console.log(`PASS: read hello.txt ${elapsed}ms content="${content}"`);
      passed++;
    } else {
      console.log(`FAIL: content mismatch. Expected "Hello from StorageProvider!" got "${content}"`);
      failed++;
    }
  } catch (e: any) {
    console.log(`FAIL: read hello.txt after ${Date.now() - t3}ms: ${e.message}`);
    failed++;
  }

  // Test 4: read nested file
  console.log('\n--- Test 4: read subdir/nested.txt ---');
  const t4 = Date.now();
  try {
    const content = await fsp.readFile(path.join(mountPoint, 'subdir', 'nested.txt'), 'utf-8');
    const elapsed = Date.now() - t4;
    if (content === 'Nested file content') {
      console.log(`PASS: read nested ${elapsed}ms content="${content}"`);
      passed++;
    } else {
      console.log(`FAIL: content mismatch. Expected "Nested file content" got "${content}"`);
      failed++;
    }
  } catch (e: any) {
    console.log(`FAIL: read nested after ${Date.now() - t4}ms: ${e.message}`);
    failed++;
  }

  // Test 5: readdir subdir
  console.log('\n--- Test 5: readdir subdir ---');
  const t5 = Date.now();
  try {
    const entries = await fsp.readdir(path.join(mountPoint, 'subdir'));
    const elapsed = Date.now() - t5;
    console.log(`PASS: readdir subdir ${elapsed}ms entries=[${entries.join(', ')}]`);
    passed++;
  } catch (e: any) {
    console.log(`FAIL: readdir subdir after ${Date.now() - t5}ms: ${e.message}`);
    failed++;
  }

  // Summary
  console.log(`\n========================================`);
  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log(`========================================`);

  // Cleanup
  mount.unmount();
  ac.abort();
  await sharePromise;
  await client.workspace.delete(ws.id);
  fs.rmSync(tmpDir, { recursive: true, force: true });
  client.close();

  if (failed > 0) {
    process.exit(1);
  }
}

main().catch(e => { console.error('Fatal:', e); process.exit(1); });
