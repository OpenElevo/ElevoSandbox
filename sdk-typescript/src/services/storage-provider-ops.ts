import * as fs from 'fs/promises';
import * as fsSync from 'fs';
import * as path from 'path';
import { PathGuard } from './storage-provider-path';
import { FileLockMap } from './storage-provider-lock';

// ============================================================
// Types
// ============================================================

export interface FileStatData {
  name: string;
  path: string;
  /** 0=File, 1=Directory, 2=Symlink */
  fileType: number;
  size: number;
  mode: number;
  uid: number;
  gid: number;
  modifiedAt?: Date;
  accessedAt?: Date;
  createdAt?: Date;
}

export interface StatFsData {
  blocks: number;
  bfree: number;
  bavail: number;
  files: number;
  ffree: number;
  bsize: number;
  namelen: number;
  frsize: number;
}

export interface OperationSuccess {
  stat?: FileStatData;
  listDir?: { entries: FileStatData[] };
  exists?: { exists: boolean };
  readData?: { data: Buffer };
  writeData?: { bytesWritten: number };
  readLink?: { target: string };
  statFs?: StatFsData;
  empty?: true;
  isLast: boolean;
}

export interface OperationError {
  code: string;
  message: string;
}

export interface OperationResponse {
  correlationId: string;
  success?: OperationSuccess;
  error?: OperationError;
}

// ============================================================
// Constants
// ============================================================

const LIST_DIR_PAGE_SIZE = 200;

// ============================================================
// Error mapping
// ============================================================

const ERRNO_MAP: Record<string, string> = {
  ENOENT: 'STORAGE_ERROR_CODE_NOT_FOUND',
  EEXIST: 'STORAGE_ERROR_CODE_ALREADY_EXISTS',
  EISDIR: 'STORAGE_ERROR_CODE_IS_A_DIRECTORY',
  ENOTDIR: 'STORAGE_ERROR_CODE_NOT_A_DIRECTORY',
  ENOTEMPTY: 'STORAGE_ERROR_CODE_DIRECTORY_NOT_EMPTY',
  EACCES: 'STORAGE_ERROR_CODE_PERMISSION_DENIED',
  EPERM: 'STORAGE_ERROR_CODE_PERMISSION_DENIED',
  EINVAL: 'STORAGE_ERROR_CODE_IO_ERROR',
};

function mapOsError(err: unknown): OperationError {
  if (err instanceof Error) {
    const nodeErr = err as NodeJS.ErrnoException;
    if (nodeErr.code && ERRNO_MAP[nodeErr.code]) {
      return { code: ERRNO_MAP[nodeErr.code], message: err.message };
    }
    if (err.message.includes('path traversal')) {
      return { code: 'STORAGE_ERROR_CODE_PATH_TRAVERSAL_DENIED', message: err.message };
    }
    if (err.message.includes('absolute paths not allowed')) {
      return { code: 'STORAGE_ERROR_CODE_PATH_TRAVERSAL_DENIED', message: err.message };
    }
  }
  const msg = err instanceof Error ? err.message : String(err);
  return { code: 'STORAGE_ERROR_CODE_IO_ERROR', message: msg };
}

function successResponse(correlationId: string, data: Partial<OperationSuccess>): OperationResponse {
  return { correlationId, success: { isLast: true, ...data } };
}

function errorResponse(correlationId: string, err: unknown): OperationResponse {
  return { correlationId, error: mapOsError(err) };
}

// ============================================================
// Stat helper
// ============================================================

function lstatToFileStatData(relPath: string, stat: fsSync.Stats): FileStatData {
  let fileType = 0; // File
  if (stat.isDirectory()) fileType = 1;
  else if (stat.isSymbolicLink()) fileType = 2;

  return {
    name: path.basename(relPath) || '.',
    path: relPath,
    fileType,
    size: Number(stat.size),
    mode: stat.mode & 0o7777,
    uid: stat.uid,
    gid: stat.gid,
    modifiedAt: stat.mtime,
    accessedAt: stat.atime,
    createdAt: stat.birthtime,
  };
}

// ============================================================
// StorageOps
// ============================================================

/**
 * Implements all 16 file operations for the StorageProvider.
 * Each method takes a correlation ID and request params, returns structured response(s).
 * Errors are caught and mapped to StorageErrorCode.
 */
export class StorageOps {
  constructor(
    private readonly pathGuard: PathGuard,
    private readonly fileLocks: FileLockMap,
  ) {}

  // ==================== Stat ====================

  async opStat(
    correlationId: string,
    req: { path: string },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      const stat = await fs.lstat(resolved.fullPath);
      const data = lstatToFileStatData(req.path, stat);
      return successResponse(correlationId, { stat: data });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== Exists ====================

  async opExists(
    correlationId: string,
    req: { path: string },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      // Use lstat (not access) so dangling symlinks report as existing.
      try {
        await fs.lstat(resolved.fullPath);
        return successResponse(correlationId, { exists: { exists: true } });
      } catch {
        return successResponse(correlationId, { exists: { exists: false } });
      }
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== ListDir ====================

  /**
   * List directory contents with pagination at 200 entries per page.
   * Returns an array of OperationResponse (one per page).
   */
  async opListDir(
    correlationId: string,
    req: { path: string },
  ): Promise<OperationResponse[]> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      const dirents = await fs.readdir(resolved.fullPath, { withFileTypes: true });
      const entries: FileStatData[] = [];

      for (const dirent of dirents) {
        const entryPath = path.join(resolved.fullPath, dirent.name);
        const relPath = req.path === '' || req.path === '.'
          ? dirent.name
          : path.join(req.path, dirent.name);
        try {
          const stat = await fs.lstat(entryPath);
          entries.push(lstatToFileStatData(relPath, stat));
        } catch {
          // Entry may have been deleted between readdir and lstat — skip it.
        }
      }

      // Small directory: single response.
      if (entries.length <= LIST_DIR_PAGE_SIZE) {
        return [successResponse(correlationId, { listDir: { entries } })];
      }

      // Large directory: paginated responses.
      const pages: OperationResponse[] = [];
      for (let i = 0; i < entries.length; i += LIST_DIR_PAGE_SIZE) {
        const end = Math.min(i + LIST_DIR_PAGE_SIZE, entries.length);
        const isLast = end >= entries.length;
        pages.push({
          correlationId,
          success: {
            listDir: { entries: entries.slice(i, end) },
            isLast,
          },
        });
      }
      return pages;
    } catch (err) {
      return [errorResponse(correlationId, err)];
    }
  }

  // ==================== ReadFileRange ====================

  async opReadFileRange(
    correlationId: string,
    req: { path: string; offset: number | string; length: number | string },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      // proto-loader deserializes uint64 as string; convert to number.
      const offset = Number(req.offset);
      const length = Number(req.length);
      const fh = await fs.open(resolved.fullPath, 'r');
      try {
        if (length === 0) {
          // Read from offset to end of file.
          const stat = await fh.stat();
          const remaining = Number(stat.size) - offset;
          if (remaining <= 0) {
            return successResponse(correlationId, { readData: { data: Buffer.alloc(0) } });
          }
          const buf = Buffer.alloc(remaining);
          const { bytesRead } = await fh.read(buf, 0, remaining, offset);
          return successResponse(correlationId, { readData: { data: buf.subarray(0, bytesRead) } });
        }

        const buf = Buffer.alloc(length);
        const { bytesRead } = await fh.read(buf, 0, length, offset);
        return successResponse(correlationId, { readData: { data: buf.subarray(0, bytesRead) } });
      } finally {
        await fh.close();
      }
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== WriteFileAt ====================

  async opWriteFileAt(
    correlationId: string,
    req: { path: string; offset: number | string; data: Buffer | Uint8Array },
  ): Promise<OperationResponse> {
    let release: (() => void) | undefined;
    try {
      release = await this.fileLocks.acquire(req.path);
      const resolved = this.pathGuard.resolve(req.path);
      // proto-loader deserializes uint64 as string; convert to number.
      const offset = Number(req.offset);

      // Match Go SDK semantics: offset=0 means full-file write (create + truncate).
      let flags: number;
      if (offset === 0) {
        flags = fsSync.constants.O_WRONLY | fsSync.constants.O_CREAT | fsSync.constants.O_TRUNC;
      } else {
        // Positional write into existing file.
        flags = fsSync.constants.O_WRONLY;
      }

      const fh = await fs.open(resolved.fullPath, flags, 0o644);
      try {
        const buf = Buffer.from(req.data);
        const { bytesWritten } = await fh.write(buf, 0, buf.length, offset);
        return successResponse(correlationId, { writeData: { bytesWritten } });
      } finally {
        await fh.close();
      }
    } catch (err) {
      return errorResponse(correlationId, err);
    } finally {
      release?.();
    }
  }

  // ==================== CreateFile ====================

  async opCreateFile(
    correlationId: string,
    req: { path: string; exclusive: boolean },
  ): Promise<OperationResponse> {
    let release: (() => void) | undefined;
    try {
      release = await this.fileLocks.acquire(req.path);
      const resolved = this.pathGuard.resolve(req.path);

      // Match Go SDK: non-exclusive includes O_TRUNC to clear existing content.
      let flags = fsSync.constants.O_CREAT | fsSync.constants.O_WRONLY;
      if (req.exclusive) {
        flags |= fsSync.constants.O_EXCL;
      } else {
        flags |= fsSync.constants.O_TRUNC;
      }

      const fh = await fs.open(resolved.fullPath, flags, 0o644);
      await fh.close();
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    } finally {
      release?.();
    }
  }

  // ==================== Mkdir ====================

  async opMkdir(
    correlationId: string,
    req: { path: string; recursive: boolean },
  ): Promise<OperationResponse> {
    try {
      // For recursive mkdir, intermediate dirs may not exist yet.
      // Use validatePath (string-level check) instead of full resolve.
      this.pathGuard.validatePath(req.path);
      const fullPath = path.join(this.pathGuard.rootDir, path.normalize(req.path));
      const resolved = path.resolve(fullPath);
      if (resolved !== this.pathGuard.rootDir && !resolved.startsWith(this.pathGuard.rootDir + path.sep)) {
        throw new Error(`path traversal denied: ${req.path}`);
      }
      await fs.mkdir(resolved, { recursive: req.recursive });
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== RemoveFile ====================

  async opRemoveFile(
    correlationId: string,
    req: { path: string },
  ): Promise<OperationResponse> {
    let release: (() => void) | undefined;
    try {
      release = await this.fileLocks.acquire(req.path);
      const resolved = this.pathGuard.resolve(req.path);
      await fs.unlink(resolved.fullPath);
      // Clean up the file lock entry.
      this.fileLocks.delete(req.path);
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    } finally {
      release?.();
    }
  }

  // ==================== RemoveDir ====================

  async opRemoveDir(
    correlationId: string,
    req: { path: string; recursive: boolean },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      if (req.recursive) {
        // Don't use force:true — it masks real errors like permission denied.
        await fs.rm(resolved.fullPath, { recursive: true });
      } else {
        await fs.rmdir(resolved.fullPath);
      }
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== Rename ====================

  async opRename(
    correlationId: string,
    req: { src: string; dst: string; flags: number },
  ): Promise<OperationResponse> {
    // Acquire locks in lexicographic order to prevent deadlocks (matches Go SDK).
    let path1 = req.src;
    let path2 = req.dst;
    if (path1 > path2) {
      [path1, path2] = [path2, path1];
    }

    let release1: (() => void) | undefined;
    let release2: (() => void) | undefined;
    try {
      release1 = await this.fileLocks.acquire(path1);
      release2 = await this.fileLocks.acquire(path2);

      const srcResolved = this.pathGuard.resolve(req.src);
      const dstResolved = this.pathGuard.resolve(req.dst);

      // flags: 0=normal, 1=NOREPLACE, 2=EXCHANGE
      if (req.flags === 2) {
        return {
          correlationId,
          error: {
            code: 'STORAGE_ERROR_CODE_NOT_SUPPORTED',
            message: 'EXCHANGE rename not supported on this platform',
          },
        };
      }

      if (req.flags === 1) {
        // NOREPLACE: fail if destination exists.
        try {
          await fs.lstat(dstResolved.fullPath);
          return {
            correlationId,
            error: {
              code: 'STORAGE_ERROR_CODE_ALREADY_EXISTS',
              message: `destination already exists: ${req.dst}`,
            },
          };
        } catch {
          // Destination doesn't exist — proceed.
        }
      }

      await fs.rename(srcResolved.fullPath, dstResolved.fullPath);
      // Clean up old path's lock entry.
      this.fileLocks.delete(req.src);
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    } finally {
      release2?.();
      release1?.();
    }
  }

  // ==================== Copy ====================

  async opCopy(
    correlationId: string,
    req: { src: string; dst: string },
  ): Promise<OperationResponse> {
    try {
      const srcResolved = this.pathGuard.resolve(req.src);
      const dstResolved = this.pathGuard.resolve(req.dst);

      const stat = await fs.lstat(srcResolved.fullPath);
      if (stat.isDirectory()) {
        // dereference:false ensures symlinks are not followed during recursive copy.
        await fs.cp(srcResolved.fullPath, dstResolved.fullPath, {
          recursive: true,
          dereference: false,
        });
      } else {
        await fs.copyFile(srcResolved.fullPath, dstResolved.fullPath);
      }

      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== SetFileSize ====================

  async opSetFileSize(
    correlationId: string,
    req: { path: string; size: number | string },
  ): Promise<OperationResponse> {
    let release: (() => void) | undefined;
    try {
      release = await this.fileLocks.acquire(req.path);
      const resolved = this.pathGuard.resolve(req.path);
      // proto-loader deserializes uint64 as string; convert to number.
      await fs.truncate(resolved.fullPath, Number(req.size));
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    } finally {
      release?.();
    }
  }

  // ==================== SetPermissions ====================

  async opSetPermissions(
    correlationId: string,
    req: { path: string; mode: number },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      // Note: Node.js fs.chmod follows symlinks. There is no lchmod equivalent
      // in Node.js fs/promises. On Linux, lchmod is generally not supported
      // (chmod on symlinks returns ENOTSUP), so this matches practical behavior.
      await fs.chmod(resolved.fullPath, req.mode);
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== SetTimes ====================

  async opSetTimes(
    correlationId: string,
    req: { path: string; atime?: Date; mtime?: Date },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);

      // If either time is not provided, use the current value.
      const currentStat = await fs.lstat(resolved.fullPath);
      const atime = req.atime ?? currentStat.atime;
      const mtime = req.mtime ?? currentStat.mtime;

      // Use lutimes to operate on symlinks directly (not their targets),
      // matching Go SDK's AT_SYMLINK_NOFOLLOW behavior.
      await fs.lutimes(resolved.fullPath, atime, mtime);
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== Symlink ====================

  async opSymlink(
    correlationId: string,
    req: { linkPath: string; target: string },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.linkPath);
      await fs.symlink(req.target, resolved.fullPath);
      return successResponse(correlationId, { empty: true });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== ReadLink ====================

  async opReadLink(
    correlationId: string,
    req: { path: string },
  ): Promise<OperationResponse> {
    try {
      const resolved = this.pathGuard.resolve(req.path);
      const target = await fs.readlink(resolved.fullPath);
      return successResponse(correlationId, { readLink: { target } });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }

  // ==================== StatFs ====================

  async opStatFs(correlationId: string): Promise<OperationResponse> {
    try {
      const stat = await fs.statfs(this.pathGuard.rootDir);
      return successResponse(correlationId, {
        statFs: {
          blocks: Number(stat.blocks),
          bfree: Number(stat.bfree),
          bavail: Number(stat.bavail),
          files: Number(stat.files),
          ffree: Number(stat.ffree),
          bsize: Number(stat.bsize),
          // namelen and frsize are not exposed by Node.js fs.statfs().
          // 255 is correct for most Linux filesystems (ext4, xfs, btrfs).
          namelen: 255,
          frsize: Number(stat.bsize),
        },
      });
    } catch (err) {
      return errorResponse(correlationId, err);
    }
  }
}
