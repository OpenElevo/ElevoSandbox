# TypeScript SDK StorageProvider Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement `StorageProvider` in the TypeScript SDK to share a local directory to a remote workspace via gRPC reverse stream, matching Go SDK functionality.

**Architecture:** A `StorageProvider` class connects to the server via `ClientStorageService.Connect()` bidirectional stream. The server sends file operation requests; the provider executes them on the local filesystem and returns results. A `FileWatcher` monitors the local directory and pushes change notifications. Large file transfers use separate `ReadFileStream`/`WriteFileStream` RPCs.

**Tech Stack:** TypeScript (ES2022, ESM), `@grpc/grpc-js` + `@grpc/proto-loader`, `chokidar` (file watching), `vitest` (testing), Node.js >= 18

---

### Task 1: Set Up Test Infrastructure

**Files:**
- Create: `sdk-typescript/vitest.config.ts`
- Modify: `sdk-typescript/package.json`
- Create: `sdk-typescript/tests/storage-provider-path.test.ts` (placeholder)

**Step 1: Install dev dependencies**

Run: `cd sdk-typescript && npm install --save-dev vitest`
Expected: vitest added to devDependencies

**Step 2: Create vitest config**

Create `sdk-typescript/vitest.config.ts`:

```typescript
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['tests/**/*.test.ts'],
    testTimeout: 10000,
  },
});
```

**Step 3: Update package.json test script**

In `sdk-typescript/package.json`, change the `"test"` script:

```json
"test": "vitest run",
"test:watch": "vitest"
```

**Step 4: Create placeholder test to verify setup**

Create `sdk-typescript/tests/storage-provider-path.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';

describe('test setup', () => {
  it('works', () => {
    expect(1 + 1).toBe(2);
  });
});
```

**Step 5: Run test to verify setup**

Run: `cd sdk-typescript && npx vitest run`
Expected: 1 test passed

**Step 6: Commit**

```bash
git add sdk-typescript/vitest.config.ts sdk-typescript/package.json sdk-typescript/package-lock.json sdk-typescript/tests/
git commit -m "feat(sdk-ts): add vitest test infrastructure"
```

---

### Task 2: Add client_storage.proto and gRPC Client

**Files:**
- Create: `sdk-typescript/proto/workspace/v1/client_storage.proto` (copy from `proto/workspace/v1/client_storage.proto`)
- Modify: `sdk-typescript/src/grpc.ts`

**Step 1: Copy proto file**

Copy `proto/workspace/v1/client_storage.proto` to `sdk-typescript/proto/workspace/v1/client_storage.proto`.

**Step 2: Add ClientStorageServiceClient interface to grpc.ts**

Add to `sdk-typescript/src/grpc.ts` after `PtyServiceClient`:

```typescript
export interface ClientStorageServiceClient extends grpc.Client {
  connect(metadata: grpc.Metadata): grpc.ClientDuplexStream<any, any>;
  readFileStream(metadata: grpc.Metadata): grpc.ClientWritableStream<any>;
  writeFileStream(
    request: any,
    metadata: grpc.Metadata
  ): grpc.ClientReadableStream<any>;
}
```

**Step 3: Add client_storage.proto to loadProtos**

In `loadProtos()`, add `client_storage.proto` to `protoFiles`:

```typescript
const protoFiles = [
  path.join(PROTO_DIR, 'workspace.proto'),
  path.join(PROTO_DIR, 'sandbox.proto'),
  path.join(PROTO_DIR, 'process.proto'),
  path.join(PROTO_DIR, 'pty.proto'),
  path.join(PROTO_DIR, 'client_storage.proto'),
];
```

**Step 4: Add clientStorage to createClients return type and implementation**

Update `createClients` return type to include `clientStorage: ClientStorageServiceClient` and add:

```typescript
clientStorage: new workspaceV1.ClientStorageService(
  serverAddr,
  credentials
) as ClientStorageServiceClient,
```

**Step 5: Verify build compiles**

Run: `cd sdk-typescript && npx tsc --noEmit`
Expected: No errors

**Step 6: Commit**

```bash
git add sdk-typescript/proto/workspace/v1/client_storage.proto sdk-typescript/src/grpc.ts
git commit -m "feat(sdk-ts): add client_storage.proto and gRPC client"
```

---

### Task 3: Implement PathGuard

**Files:**
- Create: `sdk-typescript/src/services/storage-provider-path.ts`
- Modify: `sdk-typescript/tests/storage-provider-path.test.ts`

**Step 1: Write the failing tests**

Replace `sdk-typescript/tests/storage-provider-path.test.ts`:

```typescript
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { PathGuard } from '../src/services/storage-provider-path';

describe('PathGuard', () => {
  let tmpDir: string;
  let guard: PathGuard;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'pathguard-test-'));
    guard = new PathGuard(tmpDir);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  describe('validatePath', () => {
    it('rejects double dot', () => {
      expect(() => guard.validatePath('..')).toThrow('path traversal denied');
    });

    it('rejects leading double dot', () => {
      expect(() => guard.validatePath('../etc/passwd')).toThrow('path traversal denied');
    });

    it('rejects mid double dot', () => {
      expect(() => guard.validatePath('foo/../../etc/passwd')).toThrow('path traversal denied');
    });

    it('rejects absolute path', () => {
      expect(() => guard.validatePath('/etc/passwd')).toThrow('absolute paths not allowed');
    });

    it('allows simple file', () => {
      expect(() => guard.validatePath('file.txt')).not.toThrow();
    });

    it('allows nested file', () => {
      expect(() => guard.validatePath('src/main.rs')).not.toThrow();
    });

    it('allows empty path (root)', () => {
      expect(() => guard.validatePath('')).not.toThrow();
    });

    it('allows current dir', () => {
      expect(() => guard.validatePath('.')).not.toThrow();
    });

    it('allows dot in name', () => {
      expect(() => guard.validatePath('foo.bar/baz.txt')).not.toThrow();
    });
  });

  describe('resolve', () => {
    it('resolves file in root dir', () => {
      fs.writeFileSync(path.join(tmpDir, 'root.txt'), 'hello');
      const result = guard.resolve('root.txt');
      expect(result.dirPath).toBe(tmpDir);
      expect(result.fileName).toBe('root.txt');
    });

    it('resolves nested file', () => {
      const sub = path.join(tmpDir, 'sub');
      fs.mkdirSync(sub);
      fs.writeFileSync(path.join(sub, 'file.txt'), 'hello');
      const result = guard.resolve('sub/file.txt');
      expect(result.dirPath).toBe(sub);
      expect(result.fileName).toBe('file.txt');
    });

    it('blocks symlink traversal in path component', () => {
      fs.mkdirSync(path.join(tmpDir, 'real'));
      fs.symlinkSync('/tmp', path.join(tmpDir, 'link'));
      expect(() => guard.resolve('link/somefile')).toThrow();
    });

    it('detects symlink at leaf', () => {
      fs.writeFileSync(path.join(tmpDir, 'target.txt'), 'data');
      fs.symlinkSync('target.txt', path.join(tmpDir, 'link.txt'));
      const result = guard.resolve('link.txt');
      expect(result.isSymlink).toBe(true);
    });
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-path.test.ts`
Expected: FAIL — module not found

**Step 3: Implement PathGuard**

Create `sdk-typescript/src/services/storage-provider-path.ts`:

```typescript
import * as fs from 'fs';
import * as path from 'path';

export interface ResolveResult {
  /** Absolute path to the parent directory */
  dirPath: string;
  /** Leaf file/directory name */
  fileName: string;
  /** Full resolved absolute path */
  fullPath: string;
  /** Whether the leaf is a symlink */
  isSymlink: boolean;
}

/**
 * PathGuard ensures all file operations stay within the shared root directory.
 *
 * Two-layer security:
 * 1. String validation: fast rejection of ".." components and absolute paths.
 * 2. Resolve + lstat: path.resolve then verify startsWith(rootDir), lstat to detect symlinks.
 */
export class PathGuard {
  readonly rootDir: string;

  constructor(rootDir: string) {
    const resolved = path.resolve(rootDir);
    const stat = fs.statSync(resolved);
    if (!stat.isDirectory()) {
      throw new Error(`root path is not a directory: ${resolved}`);
    }
    this.rootDir = resolved;
  }

  /**
   * Layer 1: Fast string-level check to reject obvious path traversal.
   */
  validatePath(relPath: string): void {
    if (relPath === '' || relPath === '.') return;

    const cleaned = path.normalize(relPath);

    if (path.isAbsolute(cleaned)) {
      throw new Error(`absolute paths not allowed: ${relPath}`);
    }

    // Check for ".." components after normalization.
    if (cleaned === '..' || cleaned.startsWith('..' + path.sep) ||
        cleaned.includes(path.sep + '..' + path.sep) ||
        cleaned.endsWith(path.sep + '..')) {
      throw new Error(`path traversal denied: ${relPath}`);
    }
  }

  /**
   * Layer 2: Resolve the path and verify it stays within rootDir.
   * Also checks each directory component for symlinks.
   */
  resolve(relPath: string): ResolveResult {
    this.validatePath(relPath);

    if (relPath === '' || relPath === '.') {
      return {
        dirPath: this.rootDir,
        fileName: '.',
        fullPath: this.rootDir,
        isSymlink: false,
      };
    }

    const cleaned = path.normalize(relPath);
    const fullPath = path.join(this.rootDir, cleaned);

    // Verify the resolved path is within rootDir.
    // Use path.resolve to handle any remaining edge cases.
    const resolved = path.resolve(fullPath);
    if (!resolved.startsWith(this.rootDir + path.sep) && resolved !== this.rootDir) {
      throw new Error(`path traversal denied: ${relPath}`);
    }

    // Check each directory component for symlinks (path component traversal guard).
    const parts = cleaned.split(path.sep);
    let currentPath = this.rootDir;
    for (let i = 0; i < parts.length - 1; i++) {
      currentPath = path.join(currentPath, parts[i]);
      try {
        const stat = fs.lstatSync(currentPath);
        if (stat.isSymbolicLink()) {
          throw new Error(`path traversal denied (symlink in path): ${relPath}`);
        }
        if (!stat.isDirectory()) {
          throw new Error(`not a directory: ${parts[i]}`);
        }
      } catch (err: any) {
        if (err.code === 'ENOENT') throw err;
        if (err.message.includes('path traversal') || err.message.includes('not a directory')) throw err;
        throw err;
      }
    }

    // Check if the leaf itself is a symlink.
    let isSymlink = false;
    try {
      const leafStat = fs.lstatSync(resolved);
      isSymlink = leafStat.isSymbolicLink();
    } catch {
      // File may not exist yet (e.g., for create operations) — that's fine.
    }

    return {
      dirPath: path.dirname(resolved),
      fileName: path.basename(resolved),
      fullPath: resolved,
      isSymlink,
    };
  }
}
```

**Step 4: Run tests to verify they pass**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-path.test.ts`
Expected: All tests pass

**Step 5: Commit**

```bash
git add sdk-typescript/src/services/storage-provider-path.ts sdk-typescript/tests/storage-provider-path.test.ts
git commit -m "feat(sdk-ts): implement PathGuard for path traversal protection"
```

---

### Task 4: Implement File Lock and Async Semaphore Utilities

**Files:**
- Create: `sdk-typescript/src/services/storage-provider-lock.ts`
- Create: `sdk-typescript/tests/storage-provider-lock.test.ts`

**Step 1: Write the failing tests**

Create `sdk-typescript/tests/storage-provider-lock.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';
import { FileLockMap, Semaphore } from '../src/services/storage-provider-lock';

describe('FileLockMap', () => {
  it('serializes concurrent operations on same file', async () => {
    const locks = new FileLockMap(5000);
    const order: number[] = [];

    const op = async (id: number) => {
      const release = await locks.acquire('file.txt');
      order.push(id);
      await new Promise(r => setTimeout(r, 10));
      release();
    };

    await Promise.all([op(1), op(2), op(3)]);
    // All three should have run, in some order, serially.
    expect(order).toHaveLength(3);
  });

  it('allows concurrent operations on different files', async () => {
    const locks = new FileLockMap(5000);
    let maxConcurrent = 0;
    let current = 0;

    const op = async (file: string) => {
      const release = await locks.acquire(file);
      current++;
      maxConcurrent = Math.max(maxConcurrent, current);
      await new Promise(r => setTimeout(r, 20));
      current--;
      release();
    };

    await Promise.all([op('a.txt'), op('b.txt'), op('c.txt')]);
    // Different files should run concurrently.
    expect(maxConcurrent).toBeGreaterThan(1);
  });

  it('times out when lock held too long', async () => {
    const locks = new FileLockMap(50); // 50ms timeout
    const release = await locks.acquire('busy.txt');

    await expect(locks.acquire('busy.txt')).rejects.toThrow('file lock timeout');
    release();
  });
});

describe('Semaphore', () => {
  it('limits concurrency', async () => {
    const sem = new Semaphore(2);
    let maxConcurrent = 0;
    let current = 0;

    const op = async () => {
      const release = await sem.acquire();
      current++;
      maxConcurrent = Math.max(maxConcurrent, current);
      await new Promise(r => setTimeout(r, 20));
      current--;
      release();
    };

    await Promise.all([op(), op(), op(), op()]);
    expect(maxConcurrent).toBeLessThanOrEqual(2);
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-lock.test.ts`
Expected: FAIL — module not found

**Step 3: Implement FileLockMap and Semaphore**

Create `sdk-typescript/src/services/storage-provider-lock.ts`:

```typescript
/**
 * Per-file lock using Promise chains.
 * Ensures only one write operation runs on a given file at a time.
 */
export class FileLockMap {
  private locks = new Map<string, Promise<void>>();
  private timeoutMs: number;

  constructor(timeoutMs: number = 10000) {
    this.timeoutMs = timeoutMs;
  }

  /**
   * Acquire a lock for the given file path.
   * Returns a release function. Throws if timeout is exceeded.
   */
  async acquire(filePath: string): Promise<() => void> {
    let release!: () => void;
    const newLock = new Promise<void>(resolve => { release = resolve; });

    const prev = this.locks.get(filePath) ?? Promise.resolve();
    this.locks.set(filePath, newLock);

    // Wait for previous operation with timeout.
    const timeout = new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error('file lock timeout')), this.timeoutMs)
    );

    await Promise.race([prev, timeout]);
    return release;
  }

  /** Remove a lock entry (for cleanup after file deletion). */
  delete(filePath: string): void {
    this.locks.delete(filePath);
  }
}

/**
 * Counting semaphore for limiting concurrency.
 */
export class Semaphore {
  private current = 0;
  private queue: Array<() => void> = [];

  constructor(private readonly max: number) {}

  async acquire(): Promise<() => void> {
    if (this.current < this.max) {
      this.current++;
      return () => this.release();
    }

    return new Promise<() => void>(resolve => {
      this.queue.push(() => {
        this.current++;
        resolve(() => this.release());
      });
    });
  }

  private release(): void {
    this.current--;
    const next = this.queue.shift();
    if (next) next();
  }
}
```

**Step 4: Run tests to verify they pass**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-lock.test.ts`
Expected: All tests pass

**Step 5: Commit**

```bash
git add sdk-typescript/src/services/storage-provider-lock.ts sdk-typescript/tests/storage-provider-lock.test.ts
git commit -m "feat(sdk-ts): implement FileLockMap and Semaphore concurrency primitives"
```

---

### Task 5: Implement Storage Operations — Error Mapping and Response Helpers

**Files:**
- Create: `sdk-typescript/src/services/storage-provider-ops.ts`
- Create: `sdk-typescript/tests/storage-provider-ops.test.ts`

This task implements the response builder helpers and errno→StorageErrorCode mapping, plus the first batch of operations (Stat, Exists, ListDir). Subsequent tasks add the remaining operations.

**Step 1: Write the failing tests**

Create `sdk-typescript/tests/storage-provider-ops.test.ts`:

```typescript
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { StorageOps } from '../src/services/storage-provider-ops';
import { PathGuard } from '../src/services/storage-provider-path';
import { FileLockMap } from '../src/services/storage-provider-lock';

describe('StorageOps', () => {
  let tmpDir: string;
  let ops: StorageOps;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'storageops-test-'));
    const guard = new PathGuard(tmpDir);
    const locks = new FileLockMap(5000);
    ops = new StorageOps(guard, locks);
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  describe('opStat', () => {
    it('returns stat for a regular file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'test.txt'), 'hello');
      const resp = await ops.opStat('corr-1', { path: 'test.txt' });
      expect(resp.success?.stat?.name).toBe('test.txt');
      expect(resp.success?.stat?.size).toBe(5);
      expect(resp.success?.stat?.fileType).toBe(0); // File
    });

    it('returns stat for a directory', async () => {
      fs.mkdirSync(path.join(tmpDir, 'subdir'));
      const resp = await ops.opStat('corr-1', { path: 'subdir' });
      expect(resp.success?.stat?.fileType).toBe(1); // Directory
    });

    it('returns error for non-existent path', async () => {
      const resp = await ops.opStat('corr-1', { path: 'nope.txt' });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_NOT_FOUND');
    });
  });

  describe('opExists', () => {
    it('returns true for existing file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'exists.txt'), 'x');
      const resp = await ops.opExists('corr-1', { path: 'exists.txt' });
      expect(resp.success?.exists?.exists).toBe(true);
    });

    it('returns false for non-existing file', async () => {
      const resp = await ops.opExists('corr-1', { path: 'nope.txt' });
      expect(resp.success?.exists?.exists).toBe(false);
    });
  });

  describe('opListDir', () => {
    it('lists root directory entries', async () => {
      fs.writeFileSync(path.join(tmpDir, 'a.txt'), 'a');
      fs.writeFileSync(path.join(tmpDir, 'b.txt'), 'b');
      fs.mkdirSync(path.join(tmpDir, 'sub'));
      const resp = await ops.opListDir('corr-1', { path: '' });
      expect(resp.success?.listDir?.entries).toHaveLength(3);
      const names = resp.success!.listDir!.entries.map(e => e.name);
      expect(names).toContain('a.txt');
      expect(names).toContain('b.txt');
      expect(names).toContain('sub');
    });
  });

  describe('opReadFileRange', () => {
    it('reads range from file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'data.txt'), 'Hello, World!');
      const resp = await ops.opReadFileRange('corr-1', { path: 'data.txt', offset: 7, length: 5 });
      expect(Buffer.from(resp.success!.readData!.data).toString()).toBe('World');
    });

    it('reads entire file when length=0', async () => {
      fs.writeFileSync(path.join(tmpDir, 'full.txt'), 'Read entire file');
      const resp = await ops.opReadFileRange('corr-1', { path: 'full.txt', offset: 0, length: 0 });
      expect(Buffer.from(resp.success!.readData!.data).toString()).toBe('Read entire file');
    });

    it('reads from offset to end when length=0', async () => {
      fs.writeFileSync(path.join(tmpDir, 'offset.txt'), '0123456789ABCDEF');
      const resp = await ops.opReadFileRange('corr-1', { path: 'offset.txt', offset: 10, length: 0 });
      expect(Buffer.from(resp.success!.readData!.data).toString()).toBe('ABCDEF');
    });
  });

  describe('opWriteFileAt', () => {
    it('writes at offset', async () => {
      fs.writeFileSync(path.join(tmpDir, 'out.txt'), 'AAAAAAAAAA');
      const resp = await ops.opWriteFileAt('corr-1', {
        path: 'out.txt', offset: 5, data: Buffer.from('BBBBB'),
      });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'out.txt'), 'utf-8')).toBe('AAAAABBBBB');
    });

    it('creates file and writes from offset 0', async () => {
      const resp = await ops.opWriteFileAt('corr-1', {
        path: 'new.txt', offset: 0, data: Buffer.from('hello'),
      });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'new.txt'), 'utf-8')).toBe('hello');
    });
  });

  describe('opCreateFile', () => {
    it('creates a new file', async () => {
      const resp = await ops.opCreateFile('corr-1', { path: 'new.txt', exclusive: true });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'new.txt'))).toBe(true);
    });

    it('fails exclusive create on existing file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'exist.txt'), 'x');
      const resp = await ops.opCreateFile('corr-1', { path: 'exist.txt', exclusive: true });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_ALREADY_EXISTS');
    });
  });

  describe('opMkdir', () => {
    it('creates a directory', async () => {
      const resp = await ops.opMkdir('corr-1', { path: 'subdir', recursive: false });
      expect(resp.error).toBeUndefined();
      expect(fs.statSync(path.join(tmpDir, 'subdir')).isDirectory()).toBe(true);
    });

    it('creates recursive directories', async () => {
      const resp = await ops.opMkdir('corr-1', { path: 'a/b/c', recursive: true });
      expect(resp.error).toBeUndefined();
      expect(fs.statSync(path.join(tmpDir, 'a', 'b', 'c')).isDirectory()).toBe(true);
    });
  });

  describe('opRemoveFile', () => {
    it('removes a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'del.txt'), 'x');
      const resp = await ops.opRemoveFile('corr-1', { path: 'del.txt' });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'del.txt'))).toBe(false);
    });
  });

  describe('opRemoveDir', () => {
    it('removes directory recursively', async () => {
      fs.mkdirSync(path.join(tmpDir, 'sub', 'nested'), { recursive: true });
      fs.writeFileSync(path.join(tmpDir, 'sub', 'nested', 'file.txt'), 'x');
      const resp = await ops.opRemoveDir('corr-1', { path: 'sub', recursive: true });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'sub'))).toBe(false);
    });
  });

  describe('opRename', () => {
    it('renames a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'old.txt'), 'data');
      const resp = await ops.opRename('corr-1', { src: 'old.txt', dst: 'new.txt', flags: 0 });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'old.txt'))).toBe(false);
      expect(fs.readFileSync(path.join(tmpDir, 'new.txt'), 'utf-8')).toBe('data');
    });

    it('NOREPLACE fails when dst exists', async () => {
      fs.writeFileSync(path.join(tmpDir, 'src.txt'), 'src');
      fs.writeFileSync(path.join(tmpDir, 'dst.txt'), 'dst');
      const resp = await ops.opRename('corr-1', { src: 'src.txt', dst: 'dst.txt', flags: 1 });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_ALREADY_EXISTS');
    });
  });

  describe('opCopy', () => {
    it('copies a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'orig.txt'), 'copy me');
      const resp = await ops.opCopy('corr-1', { src: 'orig.txt', dst: 'copied.txt' });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'copied.txt'), 'utf-8')).toBe('copy me');
      expect(fs.existsSync(path.join(tmpDir, 'orig.txt'))).toBe(true);
    });

    it('copies a directory recursively', async () => {
      fs.mkdirSync(path.join(tmpDir, 'srcdir', 'sub'), { recursive: true });
      fs.writeFileSync(path.join(tmpDir, 'srcdir', 'a.txt'), 'aaa');
      fs.writeFileSync(path.join(tmpDir, 'srcdir', 'sub', 'b.txt'), 'bbb');
      const resp = await ops.opCopy('corr-1', { src: 'srcdir', dst: 'dstdir' });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'dstdir', 'a.txt'), 'utf-8')).toBe('aaa');
      expect(fs.readFileSync(path.join(tmpDir, 'dstdir', 'sub', 'b.txt'), 'utf-8')).toBe('bbb');
    });
  });

  describe('opSetFileSize', () => {
    it('truncates a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'trunc.txt'), 'hello world');
      const resp = await ops.opSetFileSize('corr-1', { path: 'trunc.txt', size: 5 });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'trunc.txt'), 'utf-8')).toBe('hello');
    });
  });

  describe('opSetPermissions', () => {
    it('sets file permissions', async () => {
      fs.writeFileSync(path.join(tmpDir, 'perm.txt'), 'x');
      const resp = await ops.opSetPermissions('corr-1', { path: 'perm.txt', mode: 0o755 });
      expect(resp.error).toBeUndefined();
      const stat = fs.statSync(path.join(tmpDir, 'perm.txt'));
      expect(stat.mode & 0o777).toBe(0o755);
    });
  });

  describe('opSetTimes', () => {
    it('sets modification time', async () => {
      fs.writeFileSync(path.join(tmpDir, 'times.txt'), 'data');
      const mtime = new Date('2025-07-20T08:30:00Z');
      const atime = new Date('2025-06-15T12:00:00Z');
      const resp = await ops.opSetTimes('corr-1', { path: 'times.txt', atime, mtime });
      expect(resp.error).toBeUndefined();
      const stat = fs.statSync(path.join(tmpDir, 'times.txt'));
      expect(stat.mtime.getTime()).toBe(mtime.getTime());
    });
  });

  describe('opSymlink and opReadLink', () => {
    it('creates and reads symlink', async () => {
      fs.writeFileSync(path.join(tmpDir, 'target.txt'), 'data');
      const resp1 = await ops.opSymlink('corr-1', { linkPath: 'link.txt', target: 'target.txt' });
      expect(resp1.error).toBeUndefined();

      const resp2 = await ops.opReadLink('corr-2', { path: 'link.txt' });
      expect(resp2.success?.readLink?.target).toBe('target.txt');
    });
  });

  describe('opStatFs', () => {
    it('returns filesystem stats', async () => {
      const resp = await ops.opStatFs('corr-1');
      expect(resp.success?.statFs?.bsize).toBeGreaterThan(0);
      expect(resp.success?.statFs?.blocks).toBeGreaterThan(0);
    });
  });
});
```

**Step 2: Run tests to verify they fail**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-ops.test.ts`
Expected: FAIL — module not found

**Step 3: Implement StorageOps**

Create `sdk-typescript/src/services/storage-provider-ops.ts`. This file is large (~500 lines). It implements all 16 operations as async methods, a `FileStatData` interface, and error mapping. Full implementation code:

The `StorageOps` class:
- Takes `PathGuard` and `FileLockMap` in its constructor
- Each operation method (`opStat`, `opExists`, etc.) is async and returns a structured response object
- Error mapping: `ENOENT` → `NOT_FOUND`, `EEXIST` → `ALREADY_EXISTS`, `EACCES/EPERM` → `PERMISSION_DENIED`, `EISDIR` → `IS_A_DIRECTORY`, `ENOTDIR` → `NOT_A_DIRECTORY`, `ENOTEMPTY` → `DIRECTORY_NOT_EMPTY`
- Response type: `{ correlationId, success?, error? }` — flat object, not proto messages (proto serialization is done in the connection layer)
- Rename NOREPLACE: check existence before rename, if dst exists return ALREADY_EXISTS
- Rename EXCHANGE: not supported in Node.js, return NOT_SUPPORTED error
- `opListDir` returns all entries in one call (pagination is handled by the connection layer when sending to the stream)
- `opStatFs` uses `fs.statfs` (available in Node.js 18.15+)

Key internal types to define:

```typescript
export interface FileStatData {
  name: string;
  path: string;
  fileType: number; // 0=File, 1=Directory, 2=Symlink
  size: number;
  mode: number;
  uid: number;
  gid: number;
  modifiedAt?: Date;
  accessedAt?: Date;
  createdAt?: Date;
}

export interface OperationResponse {
  correlationId: string;
  success?: OperationSuccess;
  error?: OperationError;
}

export interface OperationSuccess {
  stat?: FileStatData;
  listDir?: { entries: FileStatData[] };
  exists?: { exists: boolean };
  readData?: { data: Buffer };
  writeData?: { bytesWritten: number };
  readLink?: { target: string };
  statFs?: { blocks: number; bfree: number; bavail: number; files: number; ffree: number; bsize: number; namelen: number; frsize: number };
  empty?: true;
  isLast: boolean;
}

export interface OperationError {
  code: string; // StorageErrorCode enum string
  message: string;
}
```

Each operation follows this pattern:
1. Resolve path via PathGuard
2. Execute fs operation
3. Return structured response
4. Catch errors, map to OperationError

See reference implementation: `sdk-go/storage_provider_ops.go`

**Step 4: Run tests to verify they pass**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-ops.test.ts`
Expected: All tests pass

**Step 5: Commit**

```bash
git add sdk-typescript/src/services/storage-provider-ops.ts sdk-typescript/tests/storage-provider-ops.test.ts
git commit -m "feat(sdk-ts): implement 16 storage operation handlers"
```

---

### Task 6: Implement FileWatcher

**Files:**
- Modify: `sdk-typescript/package.json` (add chokidar dependency)
- Create: `sdk-typescript/src/services/storage-provider-watch.ts`
- Create: `sdk-typescript/tests/storage-provider-watch.test.ts`

**Step 1: Install chokidar**

Run: `cd sdk-typescript && npm install chokidar@^4`

**Step 2: Write the failing tests**

Create `sdk-typescript/tests/storage-provider-watch.test.ts`:

```typescript
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { FileWatcher, FileChangeEvent } from '../src/services/storage-provider-watch';

describe('FileWatcher', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'filewatcher-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('detects file creation', async () => {
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'new.txt'), 'hello');

    // Wait for coalescing window to flush.
    await new Promise(r => setTimeout(r, 300));
    await watcher.close();

    expect(events.length).toBeGreaterThan(0);
    const allEvents = events.flat();
    const created = allEvents.find(e => e.path === 'new.txt');
    expect(created).toBeDefined();
  });

  it('coalesces rapid events on same file', async () => {
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    // Rapid writes to the same file.
    for (let i = 0; i < 5; i++) {
      fs.writeFileSync(path.join(tmpDir, 'rapid.txt'), `v${i}`);
    }

    await new Promise(r => setTimeout(r, 300));
    await watcher.close();

    // Should be coalesced: the file appears at most once per batch.
    const allEvents = events.flat();
    const rapidEvents = allEvents.filter(e => e.path === 'rapid.txt');
    // Exact count depends on timing, but should be fewer than 5.
    expect(rapidEvents.length).toBeLessThan(5);
  });

  it('respects default ignore dirs', async () => {
    fs.mkdirSync(path.join(tmpDir, 'node_modules'), { recursive: true });
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'node_modules', 'pkg.json'), '{}');
    await new Promise(r => setTimeout(r, 300));
    await watcher.close();

    const allEvents = events.flat();
    const nodeModuleEvents = allEvents.filter(e => e.path.startsWith('node_modules'));
    expect(nodeModuleEvents).toHaveLength(0);
  });

  it('respects .elevoignore rules', async () => {
    fs.writeFileSync(path.join(tmpDir, '.elevoignore'), '*.log\n');
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'app.log'), 'log data');
    fs.writeFileSync(path.join(tmpDir, 'app.ts'), 'code');
    await new Promise(r => setTimeout(r, 300));
    await watcher.close();

    const allEvents = events.flat();
    const logEvents = allEvents.filter(e => e.path.endsWith('.log'));
    expect(logEvents).toHaveLength(0);
    const tsEvents = allEvents.filter(e => e.path === 'app.ts');
    expect(tsEvents.length).toBeGreaterThan(0);
  });
});
```

**Step 3: Run tests to verify they fail**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-watch.test.ts`
Expected: FAIL — module not found

**Step 4: Implement FileWatcher**

Create `sdk-typescript/src/services/storage-provider-watch.ts`.

Key design:
- Uses `chokidar.watch()` with `ignored` option for default dirs
- Loads `.elevoignore` from rootDir (glob patterns, one per line, # comments)
- Event coalescing: 50ms timer on first event, 200ms max-latency cap, per-path dedup (last event wins)
- `onFlush` callback receives `FileChangeEvent[]`
- `start()` returns Promise (resolves when watcher is ready)
- `close()` returns Promise

```typescript
export interface FileChangeEvent {
  path: string;       // workspace-relative
  eventType: string;  // CREATED, MODIFIED, DELETED, RENAMED, ATTR_CHANGED
}
```

Mapping from chokidar events:
- `add` / `addDir` → `CREATED`
- `change` → `MODIFIED`
- `unlink` / `unlinkDir` → `DELETED`

Reference: `sdk-go/storage_provider_watch.go`

**Step 5: Run tests to verify they pass**

Run: `cd sdk-typescript && npx vitest run tests/storage-provider-watch.test.ts`
Expected: All tests pass

**Step 6: Commit**

```bash
git add sdk-typescript/package.json sdk-typescript/package-lock.json sdk-typescript/src/services/storage-provider-watch.ts sdk-typescript/tests/storage-provider-watch.test.ts
git commit -m "feat(sdk-ts): implement FileWatcher with event coalescing"
```

---

### Task 7: Implement StorageProvider Connection Management

**Files:**
- Create: `sdk-typescript/src/services/storage-provider.ts`
- Modify: `sdk-typescript/src/types/index.ts`

**Step 1: Add types to types/index.ts**

Add to `sdk-typescript/src/types/index.ts`:

```typescript
/**
 * Configuration for StorageProvider
 */
export interface StorageProviderConfig {
  /** Local directory path to share */
  localDir: string;
  /** Workspace ID this provider serves */
  workspaceId: string;
  /** Authentication token */
  token: string;
  /** Number of concurrent operation workers (default: 64) */
  workerPoolSize?: number;
  /** Response buffer size (default: 256) */
  responseBufferSize?: number;
  /** Max concurrent data stream RPCs (default: 8) */
  maxConcurrentDataStreams?: number;
  /** Timeout for individual operations in ms (default: 10000) */
  operationTimeoutMs?: number;
}
```

**Step 2: Implement StorageProvider**

Create `sdk-typescript/src/services/storage-provider.ts`.

This is the main orchestration class. Key responsibilities:
- `share(signal?: AbortSignal): Promise<void>` — main entry point, blocks until cancelled
- `connectAndServe()` — one connection cycle: handshake → main loop → cleanup
- Response queue: array + Promise-based drain, single writer to stream
- Worker pool: `Semaphore(workerPoolSize)` gates concurrent `executeOperation` calls
- Message dispatch: `operationRequest` → worker pool, `ping` → pong, `startDataTransfer` → data transfer handler
- Reconnection: exponential backoff 1s → 30s
- `stop()` — aborts the internal AbortController
- `isConnected()` — returns boolean

Proto message construction: Since the TS SDK uses dynamic proto loading (not generated types), messages are plain JS objects matching proto field names (camelCase via proto-loader `keepCase: false`).

Example handshake send:
```typescript
stream.write({
  handshake: { workspaceId: config.workspaceId, token: config.token }
});
```

Example operation response send:
```typescript
stream.write({
  operationResponse: {
    correlationId: resp.correlationId,
    success: resp.success ? this.toProtoSuccess(resp.success) : undefined,
    error: resp.error ? { code: resp.error.code, message: resp.error.message } : undefined,
  }
});
```

Data transfer handling:
- `handleReadFileTransfer` — reads local file, opens `ReadFileStream` client-streaming RPC, sends header + 64KB chunks
- `handleWriteFileTransfer` — opens `WriteFileStream` server-streaming RPC, writes received chunks to local file, cleans up on failure

File change notification:
- FileWatcher's `onFlush` callback builds a `fileChanged` message and writes to stream

Reference: `sdk-go/storage_provider.go`

**Step 3: Verify build compiles**

Run: `cd sdk-typescript && npx tsc --noEmit`
Expected: No errors

**Step 4: Commit**

```bash
git add sdk-typescript/src/services/storage-provider.ts sdk-typescript/src/types/index.ts
git commit -m "feat(sdk-ts): implement StorageProvider connection management"
```

---

### Task 8: Integrate into Client and Exports

**Files:**
- Modify: `sdk-typescript/src/client.ts`
- Modify: `sdk-typescript/src/services/index.ts`
- Modify: `sdk-typescript/src/index.ts`

**Step 1: Update client.ts to expose newStorageProvider**

Add import and method to `WorkspaceClient`:

```typescript
import { StorageProvider } from './services/storage-provider';
import { StorageProviderConfig } from './types';

// In WorkspaceClient class:
/**
 * Create a new StorageProvider that shares a local directory
 * with a remote workspace.
 */
newStorageProvider(config: StorageProviderConfig): StorageProvider {
  return new StorageProvider(this.clients.clientStorage, config);
}
```

**Step 2: Update services/index.ts**

Add export:
```typescript
export { StorageProvider } from './storage-provider';
export { PathGuard } from './storage-provider-path';
export { FileWatcher, type FileChangeEvent } from './storage-provider-watch';
```

**Step 3: Update client.ts close() to handle clientStorage client**

In the `close()` method, add:
```typescript
this.clients.clientStorage.close();
```

**Step 4: Verify build compiles**

Run: `cd sdk-typescript && npx tsc --noEmit`
Expected: No errors

**Step 5: Commit**

```bash
git add sdk-typescript/src/client.ts sdk-typescript/src/services/index.ts sdk-typescript/src/index.ts
git commit -m "feat(sdk-ts): integrate StorageProvider into WorkspaceClient"
```

---

### Task 9: Add Integration Example

**Files:**
- Create: `sdk-typescript/examples/storage-provider.ts`

**Step 1: Create example**

Create `sdk-typescript/examples/storage-provider.ts`:

```typescript
/**
 * Example: Share a local directory to a remote workspace
 */
import { WorkspaceClient } from '../src/index';

async function main() {
  const client = new WorkspaceClient('localhost:9090', { apiKey: 'test-key' });

  try {
    // Create a remote workspace.
    const workspace = await client.workspace.create({
      name: 'my-remote-workspace',
      // Note: storage_type must be set to "remote" on the server side.
    });
    console.log(`Created workspace: ${workspace.id}`);

    // Share local directory to the workspace.
    const provider = client.newStorageProvider({
      localDir: '/path/to/local/project',
      workspaceId: workspace.id,
      token: 'test-key',
    });

    // Start sharing (blocks until stopped).
    const controller = new AbortController();

    // Stop after 60 seconds for demo purposes.
    setTimeout(() => {
      console.log('Stopping storage provider...');
      provider.stop();
    }, 60000);

    console.log('Sharing local directory...');
    await provider.share(controller.signal);
    console.log('Storage provider stopped.');
  } finally {
    client.close();
  }
}

main().catch(console.error);
```

**Step 2: Commit**

```bash
git add sdk-typescript/examples/storage-provider.ts
git commit -m "feat(sdk-ts): add StorageProvider usage example"
```

---

### Task 10: Run All Tests and Final Verification

**Step 1: Run full test suite**

Run: `cd sdk-typescript && npx vitest run`
Expected: All tests pass

**Step 2: Run TypeScript build**

Run: `cd sdk-typescript && npx tsc`
Expected: Build succeeds, dist/ generated

**Step 3: Verify exports**

Run: `cd sdk-typescript && node -e "import('./dist/index.js').then(m => console.log(Object.keys(m)))"`
Expected: Should include `StorageProvider`, `PathGuard`, `FileWatcher`, `WorkspaceClient`, etc.

**Step 4: Final commit if any cleanup needed**

```bash
git add -A sdk-typescript/
git commit -m "feat(sdk-ts): StorageProvider complete — local directory sharing via gRPC reverse stream"
```
