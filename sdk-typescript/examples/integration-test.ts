/**
 * Comprehensive Integration Test for Elevo Workspace TypeScript SDK.
 *
 * Tests all major SDK functionality:
 *   1. Workspace CRUD (create, get, list, exists, delete)
 *   2. Managed workspace file operations (writeFile, readFile, mkdir, listFiles, etc.)
 *   3. Sandbox lifecycle (create, get, list, waitForState, exists, delete)
 *   4. Process execution (run, exec, shell, runStream)
 *   5. PTY interactive terminal sessions
 *   6. FUSE mount on managed workspace (bidirectional API ↔ FUSE)
 *   7. StorageProvider on remote workspace (A shares local dir → B reads/writes via API)
 *   8. Error handling (not found, etc.)
 *   9. Multi-sandbox on same workspace (shared /workspace volume)
 *
 * Usage:
 *   npx tsx examples/integration-test.ts [--server <addr>]
 *
 * Example:
 *   npx tsx examples/integration-test.ts --server 172.30.0.188:3201
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { WorkspaceClient } from '../src/client';
import { FuseService, FuseMount } from '../src/services/fuse';
import { PtySession } from '../src/services/pty';
import type {
  Workspace,
  Sandbox,
  FileInfo,
  CommandResult,
  ProcessEvent,
} from '../src/types';

// ─────────────────────────── Helpers ───────────────────────────

const COLORS = {
  reset: '\x1b[0m',
  green: '\x1b[32m',
  red: '\x1b[31m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
  dim: '\x1b[2m',
  bold: '\x1b[1m',
};

let passed = 0;
let failed = 0;
const failures: string[] = [];

function log(msg: string) {
  console.log(msg);
}

function section(title: string) {
  log(`\n${COLORS.cyan}${COLORS.bold}=== ${title} ===${COLORS.reset}\n`);
}

function step(msg: string) {
  process.stdout.write(`  ${COLORS.dim}>${COLORS.reset} ${msg} ... `);
}

function ok(detail?: string) {
  passed++;
  const extra = detail ? ` ${COLORS.dim}(${detail})${COLORS.reset}` : '';
  console.log(`${COLORS.green}OK${COLORS.reset}${extra}`);
}

function fail(testName: string, error: unknown) {
  failed++;
  const msg = error instanceof Error ? error.message : String(error);
  console.log(`${COLORS.red}FAIL${COLORS.reset} - ${msg}`);
  failures.push(`${testName}: ${msg}`);
}

function assert(condition: boolean, msg: string) {
  if (!condition) throw new Error(`Assertion failed: ${msg}`);
}

function parseArgs(): { server: string; httpServer: string } {
  const args = process.argv.slice(2);
  let server = '172.30.0.188:3201';
  let httpServer = '';
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--server' && args[i + 1]) {
      server = args[++i];
    } else if (args[i] === '--http-server' && args[i + 1]) {
      httpServer = args[++i];
    }
  }
  // Auto-derive HTTP server from gRPC server if not specified
  // Convention: HTTP port = gRPC port - 1 (e.g., 3201 → 3200)
  if (!httpServer) {
    const [host, portStr] = server.split(':');
    const grpcPort = parseInt(portStr || '3201', 10);
    httpServer = `http://${host}:${grpcPort - 1}`;
  }
  return { server, httpServer };
}

/** Wait for a specified number of milliseconds. */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ─────────────────────────── Test: Workspace CRUD ───────────────────────────

async function testWorkspaceCrud(client: WorkspaceClient): Promise<string> {
  section('1. Workspace CRUD');

  // Create
  step('Create workspace');
  let workspace: Workspace;
  try {
    workspace = await client.workspace.create({
      name: 'integration-test-managed',
      metadata: { purpose: 'integration-test' },
    });
    assert(!!workspace.id, 'workspace should have an id');
    assert(workspace.name === 'integration-test-managed', 'workspace name mismatch');
    assert(workspace.storageType === 'managed', 'default storage type should be managed');
    ok(`id=${workspace.id}`);
  } catch (e) {
    fail('Create workspace', e);
    throw e;
  }

  // Get
  step('Get workspace');
  try {
    const fetched = await client.workspace.get(workspace.id);
    assert(fetched.id === workspace.id, 'fetched workspace id mismatch');
    assert(fetched.name === 'integration-test-managed', 'fetched workspace name mismatch');
    ok();
  } catch (e) {
    fail('Get workspace', e);
  }

  // List
  step('List workspaces');
  try {
    const list = await client.workspace.list();
    assert(list.length > 0, 'workspace list should not be empty');
    const found = list.some((w) => w.id === workspace.id);
    assert(found, 'created workspace should appear in list');
    ok(`count=${list.length}`);
  } catch (e) {
    fail('List workspaces', e);
  }

  // Exists
  step('Workspace exists');
  try {
    const exists = await client.workspace.exists(workspace.id);
    assert(exists === true, 'workspace should exist');
    ok();
  } catch (e) {
    fail('Workspace exists', e);
  }

  // Exists (non-existent)
  step('Workspace not exists');
  try {
    const exists = await client.workspace.exists('non-existent-id-12345');
    assert(exists === false, 'non-existent workspace should not exist');
    ok();
  } catch (e) {
    fail('Workspace not exists', e);
  }

  return workspace.id;
}

// ─────────────────────── Test: Workspace File Operations ───────────────────────

async function testWorkspaceFileOps(client: WorkspaceClient, workspaceId: string) {
  section('2. Workspace File Operations (Direct API)');

  // mkdir
  step('Create directory');
  try {
    await client.workspace.mkdir(workspaceId, 'test-dir');
    ok();
  } catch (e) {
    fail('Create directory', e);
  }

  // writeFile
  step('Write file');
  try {
    await client.workspace.writeFile(
      workspaceId,
      'test-dir/hello.txt',
      'Hello from TypeScript SDK integration test!',
    );
    ok();
  } catch (e) {
    fail('Write file', e);
  }

  // readFile
  step('Read file');
  try {
    const content = await client.workspace.readFile(workspaceId, 'test-dir/hello.txt');
    assert(
      content === 'Hello from TypeScript SDK integration test!',
      `content mismatch: "${content}"`,
    );
    ok();
  } catch (e) {
    fail('Read file', e);
  }

  // writeFile (binary)
  step('Write binary file');
  try {
    const binaryData = new Uint8Array([0x00, 0x01, 0x02, 0xff, 0xfe, 0xfd]);
    await client.workspace.writeFile(workspaceId, 'test-dir/binary.bin', binaryData);
    ok();
  } catch (e) {
    fail('Write binary file', e);
  }

  // readFileBytes
  step('Read binary file');
  try {
    const bytes = await client.workspace.readFileBytes(workspaceId, 'test-dir/binary.bin');
    assert(bytes.length === 6, `binary length mismatch: ${bytes.length}`);
    assert(bytes[0] === 0x00, 'first byte mismatch');
    assert(bytes[5] === 0xfd, 'last byte mismatch');
    ok(`${bytes.length} bytes`);
  } catch (e) {
    fail('Read binary file', e);
  }

  // getFileInfo
  step('Get file info');
  try {
    const info = await client.workspace.getFileInfo(workspaceId, 'test-dir/hello.txt');
    assert(info.name === 'hello.txt', `file name mismatch: "${info.name}"`);
    assert(info.type === 'file', `file type mismatch: "${info.type}"`);
    assert(info.size > 0, `file size should be > 0, got ${info.size}`);
    ok(`size=${info.size}`);
  } catch (e) {
    fail('Get file info', e);
  }

  // fileExists
  step('File exists');
  try {
    const exists = await client.workspace.fileExists(workspaceId, 'test-dir/hello.txt');
    assert(exists === true, 'file should exist');
    ok();
  } catch (e) {
    fail('File exists', e);
  }

  step('File not exists');
  try {
    const exists = await client.workspace.fileExists(workspaceId, 'nonexistent.txt');
    assert(exists === false, 'file should not exist');
    ok();
  } catch (e) {
    fail('File not exists', e);
  }

  // listFiles
  step('List files');
  try {
    const files = await client.workspace.listFiles(workspaceId, 'test-dir');
    assert(files.length >= 2, `expected >= 2 files, got ${files.length}`);
    const names = files.map((f) => f.name);
    assert(names.includes('hello.txt'), 'hello.txt should be in listing');
    assert(names.includes('binary.bin'), 'binary.bin should be in listing');
    ok(`count=${files.length}: ${names.join(', ')}`);
  } catch (e) {
    fail('List files', e);
  }

  // copyFile
  step('Copy file');
  try {
    await client.workspace.copyFile(workspaceId, 'test-dir/hello.txt', 'test-dir/hello-copy.txt');
    const content = await client.workspace.readFile(workspaceId, 'test-dir/hello-copy.txt');
    assert(
      content === 'Hello from TypeScript SDK integration test!',
      'copied file content mismatch',
    );
    ok();
  } catch (e) {
    fail('Copy file', e);
  }

  // moveFile
  step('Move file');
  try {
    await client.workspace.moveFile(
      workspaceId,
      'test-dir/hello-copy.txt',
      'test-dir/hello-moved.txt',
    );
    const exists = await client.workspace.fileExists(workspaceId, 'test-dir/hello-copy.txt');
    assert(exists === false, 'source file should be gone after move');
    const content = await client.workspace.readFile(workspaceId, 'test-dir/hello-moved.txt');
    assert(
      content === 'Hello from TypeScript SDK integration test!',
      'moved file content mismatch',
    );
    ok();
  } catch (e) {
    fail('Move file', e);
  }

  // deleteFile
  step('Delete file');
  try {
    await client.workspace.deleteFile(workspaceId, 'test-dir/hello-moved.txt');
    const exists = await client.workspace.fileExists(workspaceId, 'test-dir/hello-moved.txt');
    assert(exists === false, 'deleted file should not exist');
    ok();
  } catch (e) {
    fail('Delete file', e);
  }

  // delete directory (recursive)
  step('Delete directory (recursive)');
  try {
    await client.workspace.deleteFile(workspaceId, 'test-dir', true);
    const exists = await client.workspace.fileExists(workspaceId, 'test-dir');
    assert(exists === false, 'deleted directory should not exist');
    ok();
  } catch (e) {
    fail('Delete directory (recursive)', e);
  }
}

// ──────────────────── Test: Sandbox Lifecycle ────────────────────

async function testSandboxLifecycle(
  client: WorkspaceClient,
  workspaceId: string,
): Promise<string> {
  section('3. Sandbox Lifecycle');

  // Create
  step('Create sandbox');
  let sandbox: Sandbox;
  try {
    sandbox = await client.sandbox.create({
      workspaceId,
      name: 'integration-test-sandbox',
    });
    assert(!!sandbox.id, 'sandbox should have an id');
    assert(sandbox.workspaceId === workspaceId, 'sandbox workspaceId mismatch');
    ok(`id=${sandbox.id}, state=${sandbox.state}`);
  } catch (e) {
    fail('Create sandbox', e);
    throw e;
  }

  // Wait for running
  step('Wait for sandbox running');
  try {
    const ac = new AbortController();
    const timeout = setTimeout(() => ac.abort(), 60000);
    const running = await client.sandbox.waitForState(sandbox.id, 'running', ac.signal);
    clearTimeout(timeout);
    assert(running.state === 'running', `sandbox should be running, got: ${running.state}`);
    ok();
  } catch (e) {
    fail('Wait for sandbox running', e);
    throw e;
  }

  // Get
  step('Get sandbox');
  try {
    const fetched = await client.sandbox.get(sandbox.id);
    assert(fetched.id === sandbox.id, 'sandbox id mismatch');
    assert(fetched.state === 'running', `state should be running, got: ${fetched.state}`);
    ok();
  } catch (e) {
    fail('Get sandbox', e);
  }

  // List
  step('List sandboxes');
  try {
    const list = await client.sandbox.list();
    assert(list.length > 0, 'sandbox list should not be empty');
    const found = list.some((s) => s.id === sandbox.id);
    assert(found, 'created sandbox should appear in list');
    ok(`count=${list.length}`);
  } catch (e) {
    fail('List sandboxes', e);
  }

  // List with filter
  step('List sandboxes (filtered: running)');
  try {
    const running = await client.sandbox.listWithFilter('running');
    const found = running.some((s) => s.id === sandbox.id);
    assert(found, 'running sandbox should appear in filtered list');
    ok(`count=${running.length}`);
  } catch (e) {
    fail('List sandboxes (filtered)', e);
  }

  // Exists
  step('Sandbox exists');
  try {
    const exists = await client.sandbox.exists(sandbox.id);
    assert(exists === true, 'sandbox should exist');
    ok();
  } catch (e) {
    fail('Sandbox exists', e);
  }

  step('Sandbox not exists');
  try {
    const exists = await client.sandbox.exists('non-existent-sandbox-12345');
    assert(exists === false, 'non-existent sandbox should not exist');
    ok();
  } catch (e) {
    fail('Sandbox not exists', e);
  }

  return sandbox.id;
}

// ────────────────────── Test: Process Execution ──────────────────────

async function testProcessExecution(client: WorkspaceClient, sandboxId: string) {
  section('4. Process Execution');

  // Basic run
  step('Run command (echo)');
  try {
    const result = await client.process.run(sandboxId, 'echo', {
      args: ['Hello', 'from', 'TypeScript', 'SDK!'],
    });
    assert(result.exitCode === 0, `exit code should be 0, got ${result.exitCode}`);
    assert(
      result.stdout.trim() === 'Hello from TypeScript SDK!',
      `stdout mismatch: "${result.stdout.trim()}"`,
    );
    ok(`stdout="${result.stdout.trim()}"`);
  } catch (e) {
    fail('Run command (echo)', e);
  }

  // Run with env vars
  step('Run with environment variables');
  try {
    const result = await client.process.run(sandboxId, 'sh', {
      args: ['-c', 'echo "VAR=$MY_VAR"'],
      env: { MY_VAR: 'test-value-123' },
    });
    assert(result.exitCode === 0, `exit code should be 0`);
    assert(
      result.stdout.trim() === 'VAR=test-value-123',
      `env var mismatch: "${result.stdout.trim()}"`,
    );
    ok();
  } catch (e) {
    fail('Run with environment variables', e);
  }

  // Run with working directory
  step('Run with working directory');
  try {
    const result = await client.process.run(sandboxId, 'pwd', { cwd: '/tmp' });
    assert(result.exitCode === 0, `exit code should be 0`);
    assert(result.stdout.trim() === '/tmp', `cwd mismatch: "${result.stdout.trim()}"`);
    ok();
  } catch (e) {
    fail('Run with working directory', e);
  }

  // exec convenience method
  step('Exec convenience method');
  try {
    const stdout = await client.process.exec(sandboxId, 'uname', '-s');
    assert(stdout.trim().length > 0, 'uname output should not be empty');
    ok(`stdout="${stdout.trim()}"`);
  } catch (e) {
    fail('Exec convenience method', e);
  }

  // shell convenience method
  step('Shell convenience method');
  try {
    const result = await client.process.shell(
      sandboxId,
      'echo "line1" && echo "line2" && echo "done"',
    );
    assert(result.exitCode === 0, `exit code should be 0`);
    const lines = result.stdout.trim().split('\n');
    assert(lines.length === 3, `expected 3 lines, got ${lines.length}`);
    assert(lines[2] === 'done', `last line should be "done"`);
    ok();
  } catch (e) {
    fail('Shell convenience method', e);
  }

  // Non-zero exit code
  step('Run command (non-zero exit)');
  try {
    const result = await client.process.run(sandboxId, 'sh', {
      args: ['-c', 'exit 42'],
    });
    assert(result.exitCode === 42, `exit code should be 42, got ${result.exitCode}`);
    ok(`exitCode=${result.exitCode}`);
  } catch (e) {
    fail('Run command (non-zero exit)', e);
  }

  // Stderr capture
  step('Stderr capture');
  try {
    const result = await client.process.run(sandboxId, 'sh', {
      args: ['-c', 'echo "err-msg" >&2'],
    });
    assert(result.stderr.includes('err-msg'), `stderr should contain "err-msg": "${result.stderr}"`);
    ok();
  } catch (e) {
    fail('Stderr capture', e);
  }

  // Streaming output
  step('Streaming output');
  try {
    const events: ProcessEvent[] = [];
    for await (const event of client.process.runStream(sandboxId, 'bash', {
      args: ['-c', 'for i in 1 2 3; do echo "$i"; done'],
    })) {
      events.push(event);
    }
    const stdoutEvents = events.filter((e) => e.type === 'stdout');
    const exitEvents = events.filter((e) => e.type === 'exit');
    assert(stdoutEvents.length > 0, 'should have stdout events');
    assert(exitEvents.length === 1, 'should have exactly 1 exit event');
    assert(
      exitEvents[0].type === 'exit' && exitEvents[0].code === 0,
      'exit code should be 0',
    );
    ok(`events=${events.length}`);
  } catch (e) {
    fail('Streaming output', e);
  }

  // Streaming stderr
  step('Streaming stderr');
  try {
    const events: ProcessEvent[] = [];
    for await (const event of client.process.runStream(sandboxId, 'bash', {
      args: ['-c', 'echo "stdout-msg" && echo "stderr-msg" >&2'],
    })) {
      events.push(event);
    }
    const hasStdout = events.some((e) => e.type === 'stdout' && e.data.includes('stdout-msg'));
    const hasStderr = events.some((e) => e.type === 'stderr' && e.data.includes('stderr-msg'));
    assert(hasStdout, 'should have stdout event with message');
    assert(hasStderr, 'should have stderr event with message');
    ok();
  } catch (e) {
    fail('Streaming stderr', e);
  }

  // File operations via process (write + read)
  step('File operations via process');
  try {
    await client.process.shell(sandboxId, 'echo "process-written" > /workspace/proc-test.txt');
    const result = await client.process.exec(sandboxId, 'cat', '/workspace/proc-test.txt');
    assert(result.trim() === 'process-written', `content mismatch: "${result.trim()}"`);
    ok();
  } catch (e) {
    fail('File operations via process', e);
  }
}

// ──────────────────────── Test: PTY Session ────────────────────────

async function testPtySession(client: WorkspaceClient, sandboxId: string) {
  section('5. PTY Interactive Terminal');

  // Connect
  step('PTY connect');
  let session: PtySession;
  try {
    session = await client.pty.connect(sandboxId, {
      cols: 80,
      rows: 24,
      shell: '/bin/bash',
    });
    assert(!!session.id, 'PTY session should have an id');
    assert(session.cols === 80, `cols should be 80, got ${session.cols}`);
    assert(session.rows === 24, `rows should be 24, got ${session.rows}`);
    ok(`id=${session.id}`);
  } catch (e) {
    fail('PTY connect', e);
    return;
  }

  // Write and read
  step('PTY write and read');
  try {
    const output = await new Promise<string>((resolve, reject) => {
      let buf = '';
      const timeout = setTimeout(() => {
        resolve(buf);
      }, 3000);

      session.onData((data: Uint8Array) => {
        const text = new TextDecoder().decode(data);
        buf += text;
        // Wait until we see the echo output
        if (buf.includes('PTY_TEST_MARKER')) {
          clearTimeout(timeout);
          resolve(buf);
        }
      });

      // Send a command that produces recognizable output
      session.write('echo PTY_TEST_MARKER\n').catch(reject);
    });
    assert(output.includes('PTY_TEST_MARKER'), 'PTY output should contain marker');
    ok();
  } catch (e) {
    fail('PTY write and read', e);
  }

  // Resize
  step('PTY resize');
  try {
    await session.resize(120, 40);
    assert(session.cols === 120, `cols should be 120 after resize, got ${session.cols}`);
    assert(session.rows === 40, `rows should be 40 after resize, got ${session.rows}`);
    ok(`${session.cols}x${session.rows}`);
  } catch (e) {
    fail('PTY resize', e);
  }

  // Kill
  step('PTY kill');
  try {
    await session.kill();
    ok();
  } catch (e) {
    fail('PTY kill', e);
  }
}

// ─────────────── Test: FUSE Mount (Managed Workspace) ───────────────

async function testFuseMount(
  client: WorkspaceClient,
  workspaceId: string,
  serverAddr: string,
  httpServerAddr: string,
) {
  section('6. FUSE Mount on Managed Workspace');

  step('Check FUSE availability');
  if (!FuseService.isAvailable()) {
    log(`\n  ${COLORS.yellow}FUSE not available on this system, skipping all FUSE tests${COLORS.reset}`);
    ok('skipped');
    return;
  }
  ok('fusermount + /dev/fuse present');

  // Write some files via Workspace API before mounting
  step('Prepare workspace files via API');
  try {
    await client.workspace.writeFile(workspaceId, 'api-created.txt', 'Created via API before FUSE mount');
    await client.workspace.mkdir(workspaceId, 'api-dir');
    await client.workspace.writeFile(workspaceId, 'api-dir/nested.txt', 'Nested API content');
    ok();
  } catch (e) {
    fail('Prepare workspace files via API', e);
    return;
  }

  // Mount via FUSE
  const fuseGrpcAddr = serverAddr.includes('://') ? serverAddr : `http://${serverAddr}`;
  const fuseService = new FuseService(fuseGrpcAddr, undefined, 'latest', undefined, httpServerAddr);

  let fuseMount: FuseMount;
  let mountPoint: string;

  step('FUSE mount workspace');
  try {
    fuseMount = await fuseService.mount(workspaceId);
    mountPoint = await fuseMount.mount(30000);
    assert(fuseMount.isMounted, 'FUSE should be mounted');
    ok(`mounted at ${mountPoint}`);
  } catch (e) {
    fail('FUSE mount workspace', e);
    return;
  }

  try {
    // Read API-created files via FUSE
    step('Read API-created file via FUSE');
    try {
      const content = fs.readFileSync(path.join(mountPoint, 'api-created.txt'), 'utf-8');
      assert(content === 'Created via API before FUSE mount', `content mismatch: "${content}"`);
      ok();
    } catch (e) {
      fail('Read API-created file via FUSE', e);
    }

    step('Read nested API file via FUSE');
    try {
      const content = fs.readFileSync(path.join(mountPoint, 'api-dir', 'nested.txt'), 'utf-8');
      assert(content === 'Nested API content', `content mismatch: "${content}"`);
      ok();
    } catch (e) {
      fail('Read nested API file via FUSE', e);
    }

    // List directory via FUSE
    step('List root dir via FUSE');
    try {
      const entries = fs.readdirSync(mountPoint);
      assert(entries.includes('api-created.txt'), 'api-created.txt should be listed');
      assert(entries.includes('api-dir'), 'api-dir should be listed');
      ok(`entries: [${entries.join(', ')}]`);
    } catch (e) {
      fail('List root dir via FUSE', e);
    }

    // Write file via FUSE, read back via API
    step('Write file via FUSE');
    try {
      fs.writeFileSync(path.join(mountPoint, 'fuse-created.txt'), 'Written via FUSE mount!');
      ok();
    } catch (e) {
      fail('Write file via FUSE', e);
    }

    step('Read FUSE-created file via API');
    try {
      const content = await client.workspace.readFile(workspaceId, 'fuse-created.txt');
      assert(content === 'Written via FUSE mount!', `content mismatch: "${content}"`);
      ok();
    } catch (e) {
      fail('Read FUSE-created file via API', e);
    }

    // Create directory via FUSE, verify via API
    step('Create nested dir + file via FUSE');
    try {
      fs.mkdirSync(path.join(mountPoint, 'fuse-dir', 'sub'), { recursive: true });
      fs.writeFileSync(path.join(mountPoint, 'fuse-dir', 'sub', 'nested.txt'), 'Nested from FUSE!');
      ok();
    } catch (e) {
      fail('Create nested dir + file via FUSE', e);
    }

    step('Read FUSE-created nested file via API');
    try {
      const content = await client.workspace.readFile(workspaceId, 'fuse-dir/sub/nested.txt');
      assert(content === 'Nested from FUSE!', `content mismatch: "${content}"`);
      ok();
    } catch (e) {
      fail('Read FUSE-created nested file via API', e);
    }

    // Binary data round-trip
    step('Binary data round-trip via FUSE');
    try {
      const binaryData = Buffer.from([0x00, 0x01, 0x02, 0xff, 0xfe, 0xfd]);
      fs.writeFileSync(path.join(mountPoint, 'binary.bin'), binaryData);
      const readBack = await client.workspace.readFileBytes(workspaceId, 'binary.bin');
      assert(readBack.length === 6, `binary length mismatch: ${readBack.length}`);
      assert(readBack[0] === 0x00, 'first byte mismatch');
      assert(readBack[5] === 0xfd, 'last byte mismatch');
      ok(`${readBack.length} bytes`);
    } catch (e) {
      fail('Binary data round-trip via FUSE', e);
    }

    // List directory after writes
    step('List dir via FUSE after writes');
    try {
      const entries = fs.readdirSync(mountPoint);
      assert(entries.includes('fuse-created.txt'), 'fuse-created.txt should be listed');
      assert(entries.includes('fuse-dir'), 'fuse-dir should be listed');
      assert(entries.includes('binary.bin'), 'binary.bin should be listed');
      ok(`${entries.length} entries`);
    } catch (e) {
      fail('List dir via FUSE after writes', e);
    }
  } finally {
    step('Unmount FUSE');
    try {
      fuseMount!.unmount();
      ok();
    } catch (e) {
      fail('Unmount FUSE', e);
    }
  }

  // Cleanup FUSE test files from workspace
  try {
    await client.workspace.deleteFile(workspaceId, 'api-created.txt');
    await client.workspace.deleteFile(workspaceId, 'api-dir', true);
    await client.workspace.deleteFile(workspaceId, 'fuse-created.txt');
    await client.workspace.deleteFile(workspaceId, 'fuse-dir', true);
    await client.workspace.deleteFile(workspaceId, 'binary.bin');
  } catch {
    // ignore cleanup errors
  }
}

// ──────────────── Test: StorageProvider (Remote Workspace) ────────────────

async function testStorageProvider(
  client: WorkspaceClient,
): Promise<{
  workspaceId: string;
}> {
  section('7. StorageProvider: Remote Workspace Sharing');

  // ─── Setup: Client A's local directory ───
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'elevo-sp-test-'));
  log(`  ${COLORS.dim}Client A local dir: ${tmpDir}${COLORS.reset}`);

  fs.writeFileSync(path.join(tmpDir, 'shared-file.txt'), 'Hello from Client A!');
  fs.mkdirSync(path.join(tmpDir, 'shared-dir'));
  fs.writeFileSync(path.join(tmpDir, 'shared-dir', 'nested.txt'), 'Nested content from A');
  fs.writeFileSync(
    path.join(tmpDir, 'data.json'),
    JSON.stringify({ source: 'client-a', timestamp: Date.now() }),
  );

  // ─── Create remote workspace ───
  step('Create remote workspace');
  let workspaceId: string;
  try {
    const workspace = await client.workspace.create({
      name: 'integration-test-storage-provider',
      storageType: 'remote',
      metadata: { purpose: 'storage-provider-test' },
    });
    workspaceId = workspace.id;
    assert(workspace.storageType === 'remote', 'storage type should be remote');
    ok(`id=${workspaceId}`);
  } catch (e) {
    fail('Create remote workspace', e);
    throw e;
  }

  // ─── Client A: Start StorageProvider ───
  step('Client A: Start StorageProvider');
  const ac = new AbortController();
  const provider = client.newStorageProvider({
    localDir: tmpDir,
    workspaceId,
    token: 'test-token',
  });

  const sharePromise = provider.share(ac.signal).catch((err) => {
    if (!ac.signal.aborted) {
      console.error(`\n  ${COLORS.red}StorageProvider error: ${err.message}${COLORS.reset}`);
    }
  });

  let connected = false;
  for (let i = 0; i < 50; i++) {
    await sleep(200);
    if (provider.isConnected()) {
      connected = true;
      break;
    }
  }

  if (!connected) {
    fail('Client A: Start StorageProvider', new Error('Failed to connect within 10s'));
    ac.abort();
    await sharePromise;
    fs.rmSync(tmpDir, { recursive: true, force: true });
    throw new Error('StorageProvider connection failed');
  }
  ok('connected');

  // ─── Client B: Read via Workspace API ───

  step('B reads shared-file.txt via Workspace API');
  try {
    const content = await client.workspace.readFile(workspaceId, 'shared-file.txt');
    assert(content === 'Hello from Client A!', `content mismatch: "${content}"`);
    ok();
  } catch (e) {
    fail('B reads shared-file.txt via Workspace API', e);
  }

  step('B reads nested file via Workspace API');
  try {
    const content = await client.workspace.readFile(workspaceId, 'shared-dir/nested.txt');
    assert(content === 'Nested content from A', `content mismatch: "${content}"`);
    ok();
  } catch (e) {
    fail('B reads nested file via Workspace API', e);
  }

  step('B reads data.json via Workspace API');
  try {
    const content = await client.workspace.readFile(workspaceId, 'data.json');
    const data = JSON.parse(content);
    assert(data.source === 'client-a', `source mismatch: "${data.source}"`);
    ok();
  } catch (e) {
    fail('B reads data.json via Workspace API', e);
  }

  step('B lists root directory via Workspace API');
  try {
    const files = await client.workspace.listFiles(workspaceId, '');
    const names = files.map((f) => f.name);
    assert(names.includes('shared-file.txt'), 'shared-file.txt should be listed');
    assert(names.includes('shared-dir'), 'shared-dir should be listed');
    assert(names.includes('data.json'), 'data.json should be listed');
    ok(`count=${files.length}: ${names.join(', ')}`);
  } catch (e) {
    fail('B lists root directory via Workspace API', e);
  }

  // ─── Client B writes via Workspace API → Client A sees locally ───

  step('B writes file via Workspace API');
  try {
    await client.workspace.writeFile(workspaceId, 'from-b.txt', 'Hello from Client B via API!');
    ok();
  } catch (e) {
    fail('B writes file via Workspace API', e);
  }

  step('A sees file written by B locally');
  try {
    let content = '';
    const localPath = path.join(tmpDir, 'from-b.txt');
    for (let i = 0; i < 20; i++) {
      await sleep(300);
      if (fs.existsSync(localPath)) {
        content = fs.readFileSync(localPath, 'utf-8');
        if (content.length > 0) break;
      }
    }
    assert(content === 'Hello from Client B via API!', `content mismatch: "${content}"`);
    ok();
  } catch (e) {
    fail('A sees file written by B locally', e);
  }

  // ─── Client A writes locally → Client B sees via Workspace API ───

  step('A writes new file locally');
  try {
    fs.writeFileSync(path.join(tmpDir, 'from-a-live.txt'), 'Live update from Client A!');
    ok();
  } catch (e) {
    fail('A writes new file locally', e);
  }

  step('B sees new file via Workspace API');
  try {
    let content = '';
    for (let i = 0; i < 20; i++) {
      await sleep(300);
      try {
        content = await client.workspace.readFile(workspaceId, 'from-a-live.txt');
        if (content === 'Live update from Client A!') break;
      } catch {
        // file may not be visible yet
      }
    }
    assert(content === 'Live update from Client A!', `content mismatch: "${content}"`);
    ok();
  } catch (e) {
    fail('B sees new file via Workspace API', e);
  }

  // ─── Binary data round-trip ───

  step('Binary data round-trip via StorageProvider');
  try {
    const binaryData = new Uint8Array([0x00, 0x01, 0x02, 0xff, 0xfe, 0xfd]);
    await client.workspace.writeFile(workspaceId, 'binary.bin', binaryData);
    let localBytes = Buffer.alloc(0);
    const localPath = path.join(tmpDir, 'binary.bin');
    for (let i = 0; i < 20; i++) {
      await sleep(300);
      if (fs.existsSync(localPath)) {
        localBytes = fs.readFileSync(localPath);
        if (localBytes.length === 6) break;
      }
    }
    assert(localBytes.length === 6, `binary length mismatch: ${localBytes.length}`);
    assert(localBytes[0] === 0x00, 'first byte mismatch');
    assert(localBytes[5] === 0xfd, 'last byte mismatch');
    ok(`${localBytes.length} bytes`);
  } catch (e) {
    fail('Binary data round-trip via StorageProvider', e);
  }

  // ─── Cleanup ───

  step('Stop StorageProvider');
  try {
    ac.abort();
    await sharePromise;
    assert(provider.isConnected() === false, 'should be disconnected');
    ok();
  } catch (e) {
    fail('Stop StorageProvider', e);
  }

  try {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  } catch {
    // ignore
  }

  return { workspaceId };
}

// ──────────────────── Test: Error Handling ────────────────────

async function testErrorHandling(client: WorkspaceClient) {
  section('8. Error Handling');

  // Get non-existent workspace
  step('Get non-existent workspace throws');
  try {
    await client.workspace.get('non-existent-workspace-id-99999');
    fail('Get non-existent workspace', new Error('Should have thrown'));
  } catch (e: any) {
    if (e.message === 'Should have thrown') {
      fail('Get non-existent workspace', e);
    } else {
      assert(e.statusCode === 404 || e.message?.includes('not found') || e.message?.includes('Not found'),
        `should be not-found error, got: ${e.message} (status: ${e.statusCode})`);
      ok(`error: ${e.constructor.name}`);
    }
  }

  // Get non-existent sandbox
  step('Get non-existent sandbox throws');
  try {
    await client.sandbox.get('non-existent-sandbox-id-99999');
    fail('Get non-existent sandbox', new Error('Should have thrown'));
  } catch (e: any) {
    if (e.message === 'Should have thrown') {
      fail('Get non-existent sandbox', e);
    } else {
      assert(e.statusCode === 404 || e.message?.includes('not found') || e.message?.includes('Not found'),
        `should be not-found error, got: ${e.message} (status: ${e.statusCode})`);
      ok(`error: ${e.constructor.name}`);
    }
  }

  // Read non-existent file
  step('Read non-existent file throws');
  try {
    // Create a temp workspace for this test
    const ws = await client.workspace.create({ name: 'error-test-ws' });
    try {
      await client.workspace.readFile(ws.id, 'nonexistent-file.txt');
      fail('Read non-existent file', new Error('Should have thrown'));
    } catch (e: any) {
      if (e.message === 'Should have thrown') {
        fail('Read non-existent file', e);
      } else {
        ok(`error: ${e.constructor.name}`);
      }
    } finally {
      await client.workspace.delete(ws.id);
    }
  } catch (e) {
    fail('Read non-existent file', e);
  }

  // exec with non-zero exit throws ProcessError
  step('Exec with non-zero exit throws ProcessError');
  try {
    // We need a running sandbox for this — create a minimal one
    const ws = await client.workspace.create({ name: 'error-exec-test' });
    const sb = await client.sandbox.create({ workspaceId: ws.id });
    const timeout = new AbortController();
    const timer = setTimeout(() => timeout.abort(), 60000);
    await client.sandbox.waitForState(sb.id, 'running', timeout.signal);
    clearTimeout(timer);

    try {
      await client.process.exec(sb.id, 'sh', '-c', 'exit 1');
      fail('Exec non-zero exit', new Error('Should have thrown ProcessError'));
    } catch (e: any) {
      if (e.message === 'Should have thrown ProcessError') {
        fail('Exec non-zero exit', e);
      } else {
        assert(e.constructor.name === 'ProcessError' || e.message?.includes('exit code'),
          `should be ProcessError, got: ${e.constructor.name}`);
        ok(`error: ${e.constructor.name}`);
      }
    } finally {
      await client.sandbox.delete(sb.id, true).catch(() => {});
      await client.workspace.delete(ws.id).catch(() => {});
    }
  } catch (e) {
    fail('Exec non-zero exit throws', e);
  }
}

// ──────────────────── Test: Multi-Sandbox ────────────────────

async function testMultiSandbox(client: WorkspaceClient, workspaceId: string) {
  section('9. Multi-Sandbox on Same Workspace');

  step('Create two sandboxes on same workspace');
  let sandboxA: Sandbox;
  let sandboxB: Sandbox;
  try {
    sandboxA = await client.sandbox.create({
      workspaceId,
      name: 'multi-sandbox-a',
    });
    sandboxB = await client.sandbox.create({
      workspaceId,
      name: 'multi-sandbox-b',
    });
    ok(`A=${sandboxA.id}, B=${sandboxB.id}`);
  } catch (e) {
    fail('Create two sandboxes', e);
    return;
  }

  step('Wait for both sandboxes running');
  try {
    const ac = new AbortController();
    const timer = setTimeout(() => ac.abort(), 60000);
    await Promise.all([
      client.sandbox.waitForState(sandboxA.id, 'running', ac.signal),
      client.sandbox.waitForState(sandboxB.id, 'running', ac.signal),
    ]);
    clearTimeout(timer);
    ok();
  } catch (e) {
    fail('Wait for both sandboxes', e);
    // Cleanup and return
    await client.sandbox.delete(sandboxA!.id, true).catch(() => {});
    await client.sandbox.delete(sandboxB!.id, true).catch(() => {});
    return;
  }

  // Sandbox A writes to workspace, Sandbox B reads
  step('Sandbox A writes file to /workspace');
  try {
    await client.process.shell(
      sandboxA.id,
      'echo "Written by Sandbox A" > /workspace/multi-test.txt',
    );
    ok();
  } catch (e) {
    fail('Sandbox A writes file', e);
  }

  step('Sandbox B reads file written by A');
  try {
    // Allow some time for filesystem sync
    let content = '';
    for (let i = 0; i < 20; i++) {
      await sleep(500);
      try {
        const result = await client.process.exec(sandboxB.id, 'cat', '/workspace/multi-test.txt');
        content = result.trim();
        if (content === 'Written by Sandbox A') break;
      } catch {
        // File may not be synced yet
      }
    }
    assert(content === 'Written by Sandbox A', `content mismatch: "${content}"`);
    ok();
  } catch (e) {
    fail('Sandbox B reads file from A', e);
  }

  // Sandbox B writes, Sandbox A reads
  step('Sandbox B writes file to /workspace');
  try {
    await client.process.shell(
      sandboxB.id,
      'echo "Written by Sandbox B" > /workspace/multi-test-b.txt',
    );
    ok();
  } catch (e) {
    fail('Sandbox B writes file', e);
  }

  step('Sandbox A reads file written by B');
  try {
    let content = '';
    for (let i = 0; i < 20; i++) {
      await sleep(500);
      try {
        const result = await client.process.exec(sandboxA.id, 'cat', '/workspace/multi-test-b.txt');
        content = result.trim();
        if (content === 'Written by Sandbox B') break;
      } catch {
        // File may not be synced yet
      }
    }
    assert(content === 'Written by Sandbox B', `content mismatch: "${content}"`);
    ok();
  } catch (e) {
    fail('Sandbox A reads file from B', e);
  }

  // Cleanup multi-sandboxes
  step('Cleanup multi-sandboxes');
  try {
    await client.sandbox.delete(sandboxA.id, true);
    await client.sandbox.delete(sandboxB.id, true);
    ok();
  } catch (e) {
    fail('Cleanup multi-sandboxes', e);
  }
}

// ──────────────────── Cleanup ────────────────────

async function cleanup(
  client: WorkspaceClient,
  resources: { workspaceIds: string[]; sandboxIds: string[] },
) {
  section('Cleanup');

  for (const sandboxId of resources.sandboxIds) {
    step(`Delete sandbox ${sandboxId.substring(0, 8)}...`);
    try {
      await client.sandbox.delete(sandboxId, true);
      ok();
    } catch (e: any) {
      log(`${COLORS.yellow}SKIP${COLORS.reset} - ${e.message}`);
    }
  }

  for (const workspaceId of resources.workspaceIds) {
    step(`Delete workspace ${workspaceId.substring(0, 8)}...`);
    try {
      await client.workspace.delete(workspaceId);
      ok();
    } catch (e: any) {
      log(`${COLORS.yellow}SKIP${COLORS.reset} - ${e.message}`);
    }
  }
}

// ──────────────────── Main ────────────────────

async function main() {
  const { server, httpServer } = parseArgs();

  log(`${COLORS.bold}Elevo Workspace SDK - Comprehensive Integration Test${COLORS.reset}`);
  log(`gRPC Server: ${server}`);
  log(`HTTP Server: ${httpServer}`);
  log(`Time:        ${new Date().toISOString()}`);

  const client = new WorkspaceClient(server);
  const resources = { workspaceIds: [] as string[], sandboxIds: [] as string[] };

  try {
    // 1. Workspace CRUD
    const managedWsId = await testWorkspaceCrud(client);
    resources.workspaceIds.push(managedWsId);

    // 2. Workspace File Operations
    await testWorkspaceFileOps(client, managedWsId);

    // 3. Sandbox Lifecycle
    const sandboxId = await testSandboxLifecycle(client, managedWsId);
    resources.sandboxIds.push(sandboxId);

    // 4. Process Execution
    await testProcessExecution(client, sandboxId);

    // 5. PTY Session
    await testPtySession(client, sandboxId);

    // 6. FUSE Mount on Managed Workspace
    await testFuseMount(client, managedWsId, server, httpServer);

    // 7. StorageProvider (Remote Workspace)
    const spResult = await testStorageProvider(client);
    resources.workspaceIds.push(spResult.workspaceId);

    // 8. Error Handling
    await testErrorHandling(client);

    // 9. Multi-Sandbox on same workspace
    await testMultiSandbox(client, managedWsId);
  } finally {
    await cleanup(client, resources);
    client.close();
  }

  // Summary
  log(`\n${COLORS.bold}─── Test Summary ───${COLORS.reset}`);
  log(`${COLORS.green}Passed: ${passed}${COLORS.reset}`);
  if (failed > 0) {
    log(`${COLORS.red}Failed: ${failed}${COLORS.reset}`);
    log(`\n${COLORS.red}Failures:${COLORS.reset}`);
    for (const f of failures) {
      log(`  ${COLORS.red}- ${f}${COLORS.reset}`);
    }
  }
  log('');

  if (failed > 0) {
    process.exit(1);
  }
}

main().catch((e) => {
  console.error(`\n${COLORS.red}Fatal error:${COLORS.reset}`, e);
  process.exit(1);
});
