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
   * Throws on invalid paths.
   */
  validatePath(relPath: string): void {
    if (relPath === '' || relPath === '.') return;

    if (path.isAbsolute(relPath)) {
      throw new Error(`absolute paths not allowed: ${relPath}`);
    }

    const cleaned = path.normalize(relPath);

    // After normalization, check for ".." — path.normalize('foo/../../bar') = '../bar'
    if (cleaned === '..' || cleaned.startsWith('..' + path.sep) ||
        cleaned.includes(path.sep + '..' + path.sep) ||
        cleaned.endsWith(path.sep + '..')) {
      throw new Error(`path traversal denied: ${relPath}`);
    }
  }

  /**
   * Layer 2: Resolve the path and verify it stays within rootDir.
   * Also checks each directory component for symlinks.
   *
   * For file creation operations, the leaf file may not exist yet — that's fine.
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
    const resolved = path.resolve(fullPath);
    if (resolved !== this.rootDir && !resolved.startsWith(this.rootDir + path.sep)) {
      throw new Error(`path traversal denied: ${relPath}`);
    }

    // Check each directory component for symlinks (path component traversal guard).
    const parts = cleaned.split(path.sep);
    let currentPath = this.rootDir;
    for (let i = 0; i < parts.length - 1; i++) {
      currentPath = path.join(currentPath, parts[i]);
      const stat = fs.lstatSync(currentPath);
      if (stat.isSymbolicLink()) {
        throw new Error(`path traversal denied (symlink in path): ${relPath}`);
      }
      if (!stat.isDirectory()) {
        throw new Error(`not a directory: ${parts[i]}`);
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
