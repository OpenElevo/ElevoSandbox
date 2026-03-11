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

  // ============================================================
  // Stat
  // ============================================================

  describe('opStat', () => {
    it('returns stat for a regular file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'test.txt'), 'hello');
      const resp = await ops.opStat('corr-1', { path: 'test.txt' });
      expect(resp.error).toBeUndefined();
      expect(resp.success?.stat?.name).toBe('test.txt');
      expect(resp.success?.stat?.size).toBe(5);
      expect(resp.success?.stat?.fileType).toBe(0); // File
    });

    it('returns stat for a directory', async () => {
      fs.mkdirSync(path.join(tmpDir, 'subdir'));
      const resp = await ops.opStat('corr-1', { path: 'subdir' });
      expect(resp.success?.stat?.fileType).toBe(1); // Directory
    });

    it('returns stat for a symlink', async () => {
      fs.writeFileSync(path.join(tmpDir, 'target.txt'), 'data');
      fs.symlinkSync('target.txt', path.join(tmpDir, 'link.txt'));
      const resp = await ops.opStat('corr-1', { path: 'link.txt' });
      expect(resp.success?.stat?.fileType).toBe(2); // Symlink
    });

    it('returns error for non-existent path', async () => {
      const resp = await ops.opStat('corr-1', { path: 'nope.txt' });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_NOT_FOUND');
    });
  });

  // ============================================================
  // Exists
  // ============================================================

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

    it('returns true for dangling symlink', async () => {
      // Create a symlink pointing to a non-existent target.
      fs.symlinkSync('/nonexistent-target', path.join(tmpDir, 'dangling'));
      const resp = await ops.opExists('corr-1', { path: 'dangling' });
      // lstat reports the symlink itself as existing, even though target doesn't exist.
      expect(resp.success?.exists?.exists).toBe(true);
    });
  });

  // ============================================================
  // ListDir
  // ============================================================

  describe('opListDir', () => {
    it('lists root directory entries', async () => {
      fs.writeFileSync(path.join(tmpDir, 'a.txt'), 'a');
      fs.writeFileSync(path.join(tmpDir, 'b.txt'), 'b');
      fs.mkdirSync(path.join(tmpDir, 'sub'));
      const pages = await ops.opListDir('corr-1', { path: '' });
      expect(pages).toHaveLength(1);
      const resp = pages[0];
      expect(resp.success?.listDir?.entries).toHaveLength(3);
      expect(resp.success!.isLast).toBe(true);
      const names = resp.success!.listDir!.entries.map(e => e.name);
      expect(names).toContain('a.txt');
      expect(names).toContain('b.txt');
      expect(names).toContain('sub');
    });

    it('lists subdirectory entries', async () => {
      const sub = path.join(tmpDir, 'sub');
      fs.mkdirSync(sub);
      fs.writeFileSync(path.join(sub, 'c.txt'), 'c');
      const pages = await ops.opListDir('corr-1', { path: 'sub' });
      expect(pages).toHaveLength(1);
      expect(pages[0].success?.listDir?.entries).toHaveLength(1);
      expect(pages[0].success!.listDir!.entries[0].name).toBe('c.txt');
    });

    it('returns error for non-existent dir', async () => {
      const pages = await ops.opListDir('corr-1', { path: 'nonexistent' });
      expect(pages).toHaveLength(1);
      expect(pages[0].error).toBeDefined();
    });

    it('paginates large directories at 200 entries', async () => {
      // Create 250 files to trigger pagination.
      for (let i = 0; i < 250; i++) {
        fs.writeFileSync(path.join(tmpDir, `file-${String(i).padStart(3, '0')}.txt`), `${i}`);
      }
      const pages = await ops.opListDir('corr-1', { path: '' });
      expect(pages.length).toBe(2);
      // First page has 200 entries, isLast=false.
      expect(pages[0].success?.listDir?.entries).toHaveLength(200);
      expect(pages[0].success!.isLast).toBe(false);
      // Second page has 50 entries, isLast=true.
      expect(pages[1].success?.listDir?.entries).toHaveLength(50);
      expect(pages[1].success!.isLast).toBe(true);
    });
  });

  // ============================================================
  // ReadFileRange
  // ============================================================

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

    it('returns error for non-existent file', async () => {
      const resp = await ops.opReadFileRange('corr-1', { path: 'nope.txt', offset: 0, length: 10 });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_NOT_FOUND');
    });
  });

  // ============================================================
  // WriteFileAt
  // ============================================================

  describe('opWriteFileAt', () => {
    it('writes at offset', async () => {
      fs.writeFileSync(path.join(tmpDir, 'out.txt'), 'AAAAAAAAAA');
      const resp = await ops.opWriteFileAt('corr-1', {
        path: 'out.txt', offset: 5, data: Buffer.from('BBBBB'),
      });
      expect(resp.error).toBeUndefined();
      expect(resp.success?.writeData?.bytesWritten).toBe(5);
      expect(fs.readFileSync(path.join(tmpDir, 'out.txt'), 'utf-8')).toBe('AAAAABBBBB');
    });

    it('creates file and writes from offset 0', async () => {
      const resp = await ops.opWriteFileAt('corr-1', {
        path: 'new.txt', offset: 0, data: Buffer.from('hello'),
      });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'new.txt'), 'utf-8')).toBe('hello');
    });

    it('truncates existing file when writing at offset 0', async () => {
      // Write 100 bytes initially.
      fs.writeFileSync(path.join(tmpDir, 'trunc.txt'), 'A'.repeat(100));
      // Write only 5 bytes at offset 0 — should truncate the rest.
      const resp = await ops.opWriteFileAt('corr-1', {
        path: 'trunc.txt', offset: 0, data: Buffer.from('hello'),
      });
      expect(resp.error).toBeUndefined();
      const content = fs.readFileSync(path.join(tmpDir, 'trunc.txt'), 'utf-8');
      expect(content).toBe('hello');
      expect(content.length).toBe(5);
    });
  });

  // ============================================================
  // CreateFile
  // ============================================================

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

    it('non-exclusive create succeeds on existing file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'exist.txt'), 'x');
      const resp = await ops.opCreateFile('corr-1', { path: 'exist.txt', exclusive: false });
      expect(resp.error).toBeUndefined();
    });

    it('non-exclusive create truncates existing file content', async () => {
      fs.writeFileSync(path.join(tmpDir, 'big.txt'), 'A'.repeat(100));
      const resp = await ops.opCreateFile('corr-1', { path: 'big.txt', exclusive: false });
      expect(resp.error).toBeUndefined();
      // File should be empty after non-exclusive create (O_TRUNC).
      const content = fs.readFileSync(path.join(tmpDir, 'big.txt'), 'utf-8');
      expect(content).toBe('');
    });
  });

  // ============================================================
  // Mkdir
  // ============================================================

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

    it('fails non-recursive when parent missing', async () => {
      const resp = await ops.opMkdir('corr-1', { path: 'x/y', recursive: false });
      expect(resp.error).toBeDefined();
    });
  });

  // ============================================================
  // RemoveFile
  // ============================================================

  describe('opRemoveFile', () => {
    it('removes a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'del.txt'), 'x');
      const resp = await ops.opRemoveFile('corr-1', { path: 'del.txt' });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'del.txt'))).toBe(false);
    });

    it('returns error for non-existent file', async () => {
      const resp = await ops.opRemoveFile('corr-1', { path: 'nope.txt' });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_NOT_FOUND');
    });
  });

  // ============================================================
  // RemoveDir
  // ============================================================

  describe('opRemoveDir', () => {
    it('removes empty directory', async () => {
      fs.mkdirSync(path.join(tmpDir, 'empty'));
      const resp = await ops.opRemoveDir('corr-1', { path: 'empty', recursive: false });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'empty'))).toBe(false);
    });

    it('removes directory recursively', async () => {
      fs.mkdirSync(path.join(tmpDir, 'sub', 'nested'), { recursive: true });
      fs.writeFileSync(path.join(tmpDir, 'sub', 'nested', 'file.txt'), 'x');
      const resp = await ops.opRemoveDir('corr-1', { path: 'sub', recursive: true });
      expect(resp.error).toBeUndefined();
      expect(fs.existsSync(path.join(tmpDir, 'sub'))).toBe(false);
    });

    it('fails non-recursive on non-empty directory', async () => {
      fs.mkdirSync(path.join(tmpDir, 'notempty'));
      fs.writeFileSync(path.join(tmpDir, 'notempty', 'file.txt'), 'x');
      const resp = await ops.opRemoveDir('corr-1', { path: 'notempty', recursive: false });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_DIRECTORY_NOT_EMPTY');
    });
  });

  // ============================================================
  // Rename
  // ============================================================

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

    it('EXCHANGE returns NOT_SUPPORTED', async () => {
      fs.writeFileSync(path.join(tmpDir, 'alpha.txt'), 'AAA');
      fs.writeFileSync(path.join(tmpDir, 'beta.txt'), 'BBB');
      const resp = await ops.opRename('corr-1', { src: 'alpha.txt', dst: 'beta.txt', flags: 2 });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_NOT_SUPPORTED');
    });
  });

  // ============================================================
  // Copy
  // ============================================================

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

  // ============================================================
  // SetFileSize
  // ============================================================

  describe('opSetFileSize', () => {
    it('truncates a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'trunc.txt'), 'hello world');
      const resp = await ops.opSetFileSize('corr-1', { path: 'trunc.txt', size: 5 });
      expect(resp.error).toBeUndefined();
      expect(fs.readFileSync(path.join(tmpDir, 'trunc.txt'), 'utf-8')).toBe('hello');
    });

    it('extends a file', async () => {
      fs.writeFileSync(path.join(tmpDir, 'extend.txt'), 'hi');
      const resp = await ops.opSetFileSize('corr-1', { path: 'extend.txt', size: 10 });
      expect(resp.error).toBeUndefined();
      expect(fs.statSync(path.join(tmpDir, 'extend.txt')).size).toBe(10);
    });
  });

  // ============================================================
  // SetPermissions
  // ============================================================

  describe('opSetPermissions', () => {
    it('sets file permissions', async () => {
      fs.writeFileSync(path.join(tmpDir, 'perm.txt'), 'x');
      const resp = await ops.opSetPermissions('corr-1', { path: 'perm.txt', mode: 0o755 });
      expect(resp.error).toBeUndefined();
      const stat = fs.statSync(path.join(tmpDir, 'perm.txt'));
      expect(stat.mode & 0o777).toBe(0o755);
    });
  });

  // ============================================================
  // SetTimes
  // ============================================================

  describe('opSetTimes', () => {
    it('sets modification and access time', async () => {
      fs.writeFileSync(path.join(tmpDir, 'times.txt'), 'data');
      const mtime = new Date('2025-07-20T08:30:00Z');
      const atime = new Date('2025-06-15T12:00:00Z');
      const resp = await ops.opSetTimes('corr-1', { path: 'times.txt', atime, mtime });
      expect(resp.error).toBeUndefined();
      const stat = fs.statSync(path.join(tmpDir, 'times.txt'));
      expect(stat.mtime.getTime()).toBe(mtime.getTime());
      expect(stat.atime.getTime()).toBe(atime.getTime());
    });
  });

  // ============================================================
  // Symlink and ReadLink
  // ============================================================

  describe('opSymlink and opReadLink', () => {
    it('creates and reads symlink', async () => {
      fs.writeFileSync(path.join(tmpDir, 'target.txt'), 'data');
      const resp1 = await ops.opSymlink('corr-1', { linkPath: 'link.txt', target: 'target.txt' });
      expect(resp1.error).toBeUndefined();

      const resp2 = await ops.opReadLink('corr-2', { path: 'link.txt' });
      expect(resp2.success?.readLink?.target).toBe('target.txt');
    });

    it('readlink returns error for non-symlink', async () => {
      fs.writeFileSync(path.join(tmpDir, 'regular.txt'), 'data');
      const resp = await ops.opReadLink('corr-1', { path: 'regular.txt' });
      expect(resp.error).toBeDefined();
    });
  });

  // ============================================================
  // StatFs
  // ============================================================

  describe('opStatFs', () => {
    it('returns filesystem stats', async () => {
      const resp = await ops.opStatFs('corr-1');
      expect(resp.error).toBeUndefined();
      expect(resp.success?.statFs?.bsize).toBeGreaterThan(0);
      expect(resp.success?.statFs?.blocks).toBeGreaterThan(0);
    });
  });

  // ============================================================
  // Error mapping
  // ============================================================

  describe('error mapping', () => {
    it('path traversal returns PATH_TRAVERSAL_DENIED', async () => {
      const resp = await ops.opStat('corr-1', { path: '../etc/passwd' });
      expect(resp.error?.code).toBe('STORAGE_ERROR_CODE_PATH_TRAVERSAL_DENIED');
    });
  });
});
