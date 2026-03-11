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

    it('allows deep path', () => {
      expect(() => guard.validatePath('a/b/c/d/e.txt')).not.toThrow();
    });
  });

  describe('resolve', () => {
    it('resolves file in root dir', () => {
      fs.writeFileSync(path.join(tmpDir, 'root.txt'), 'hello');
      const result = guard.resolve('root.txt');
      expect(result.dirPath).toBe(tmpDir);
      expect(result.fileName).toBe('root.txt');
      expect(result.fullPath).toBe(path.join(tmpDir, 'root.txt'));
    });

    it('resolves nested file', () => {
      const sub = path.join(tmpDir, 'sub');
      fs.mkdirSync(sub);
      fs.writeFileSync(path.join(sub, 'file.txt'), 'hello');
      const result = guard.resolve('sub/file.txt');
      expect(result.dirPath).toBe(sub);
      expect(result.fileName).toBe('file.txt');
    });

    it('resolves root path (empty string)', () => {
      const result = guard.resolve('');
      expect(result.fullPath).toBe(tmpDir);
      expect(result.fileName).toBe('.');
    });

    it('resolves root path (dot)', () => {
      const result = guard.resolve('.');
      expect(result.fullPath).toBe(tmpDir);
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

    it('resolves non-existent file without error', () => {
      // For create operations, the file doesn't exist yet.
      const result = guard.resolve('nonexistent.txt');
      expect(result.fullPath).toBe(path.join(tmpDir, 'nonexistent.txt'));
      expect(result.isSymlink).toBe(false);
    });
  });
});
