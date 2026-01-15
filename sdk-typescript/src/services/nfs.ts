/**
 * NFS Service for mounting sandbox workspaces
 */

import { execSync, spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

/**
 * Options for NFS mount
 */
export interface NfsMountOptions {
  /** NFS server host */
  host: string;
  /** NFS server port (default: 2049) */
  port?: number;
  /** Export path on the server */
  exportPath: string;
  /** Local mount point (auto-created if not specified) */
  mountPoint?: string;
  /** Custom mount options (default: nfsvers=3,tcp,nolock,port={port},mountport={port}) */
  options?: string;
}

/**
 * Represents an active NFS mount
 */
export class NfsMount {
  private readonly host: string;
  private readonly port: number;
  private readonly exportPath: string;
  private _mountPoint: string;
  private readonly options: string;
  private _mounted: boolean = false;
  private _tempDir: string | null = null;

  constructor(options: NfsMountOptions) {
    this.host = options.host;
    this.port = options.port ?? 2049;
    this.exportPath = options.exportPath;
    this._mountPoint = options.mountPoint ?? '';
    this.options = options.options ?? 'nfsvers=3,tcp,nolock,port={port},mountport={port}';
  }

  /**
   * Get the mount point path
   */
  get mountPoint(): string {
    if (this._mountPoint) {
      return this._mountPoint;
    }
    throw new Error('Mount point not initialized');
  }

  /**
   * Check if currently mounted
   */
  get isMounted(): boolean {
    return this._mounted;
  }

  /**
   * Mount the NFS share
   * @returns The mount point path
   */
  mount(): string {
    if (this._mounted) {
      return this._mountPoint;
    }

    // Create mount point if not specified
    if (!this._mountPoint) {
      this._tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'workspace_nfs_'));
      this._mountPoint = this._tempDir;
    } else {
      fs.mkdirSync(this._mountPoint, { recursive: true });
    }

    // Build mount options
    const opts = this.options.replace(/{port}/g, String(this.port));

    // Mount command
    const cmd = `mount -t nfs -o ${opts} ${this.host}:${this.exportPath} ${this._mountPoint}`;

    try {
      execSync(cmd, { stdio: 'pipe' });
      this._mounted = true;
      return this._mountPoint;
    } catch (error: any) {
      if (this._tempDir) {
        fs.rmdirSync(this._tempDir);
        this._tempDir = null;
      }
      throw new Error(`Failed to mount NFS: ${error.stderr?.toString() || error.message}`);
    }
  }

  /**
   * Unmount the NFS share
   */
  unmount(): void {
    if (!this._mounted) {
      return;
    }

    try {
      execSync(`umount ${this._mountPoint}`, { stdio: 'pipe' });
    } catch {
      // Try lazy unmount
      try {
        execSync(`umount -l ${this._mountPoint}`, { stdio: 'pipe' });
      } catch {
        // Ignore errors
      }
    } finally {
      this._mounted = false;
      if (this._tempDir) {
        try {
          fs.rmdirSync(this._tempDir);
        } catch {
          // Ignore cleanup errors
        }
        this._tempDir = null;
      }
    }
  }
}

/**
 * Service for managing NFS mounts for sandbox workspaces
 */
export class NfsService {
  private readonly defaultHost?: string;
  private readonly defaultPort: number;

  /**
   * Create an NFS service
   * @param defaultHost Default NFS server host
   * @param defaultPort Default NFS port (default: 2049)
   */
  constructor(defaultHost?: string, defaultPort: number = 2049) {
    this.defaultHost = defaultHost;
    this.defaultPort = defaultPort;
  }

  /**
   * Create an NFS mount for a sandbox
   * @param sandboxId The sandbox ID to mount
   * @param options Mount options
   * @returns NfsMount instance
   *
   * @example
   * ```typescript
   * const mount = nfs.mount('sandbox-123');
   * try {
   *   mount.mount();
   *   // Access files at mount.mountPoint
   *   fs.writeFileSync(`${mount.mountPoint}/test.txt`, 'Hello');
   * } finally {
   *   mount.unmount();
   * }
   * ```
   */
  mount(
    sandboxId: string,
    options?: {
      mountPoint?: string;
      host?: string;
      port?: number;
    }
  ): NfsMount {
    const host = options?.host ?? this.defaultHost;
    const port = options?.port ?? this.defaultPort;

    if (!host) {
      throw new Error('NFS host not specified and no default configured');
    }

    return new NfsMount({
      host,
      port,
      exportPath: `/${sandboxId}`,
      mountPoint: options?.mountPoint,
    });
  }

  /**
   * Create an NFS mount from a full NFS URL
   * @param nfsUrl NFS URL in format nfs://host:port/path
   * @param mountPoint Optional local mount point
   * @returns NfsMount instance
   */
  mountFromUrl(nfsUrl: string, mountPoint?: string): NfsMount {
    const url = new URL(nfsUrl);
    const host = url.hostname;
    const port = url.port ? parseInt(url.port, 10) : this.defaultPort;
    const exportPath = url.pathname;

    return new NfsMount({
      host,
      port,
      exportPath,
      mountPoint,
    });
  }

  /**
   * Check if NFS mount is available on this system
   */
  static isAvailable(): boolean {
    try {
      execSync('which mount.nfs', { stdio: 'pipe' });
      return true;
    } catch {
      return false;
    }
  }
}
