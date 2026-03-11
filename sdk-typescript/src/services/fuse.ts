/**
 * FUSE Service for mounting workspaces locally via workspace-fuse client.
 *
 * This service automatically downloads the workspace-fuse binary and manages
 * FUSE mounts for workspaces.
 */

import { execSync, spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as https from 'https';
import * as http from 'http';
import * as os from 'os';
import * as path from 'path';
import { URL } from 'url';

// Default version and download URL template
const DEFAULT_VERSION = 'latest';
const GITHUB_RELEASE_URL =
  'https://github.com/OpenElevo/ElevoSandbox/releases/download/{version}/workspace-fuse-{platform}-{arch}';
const GITHUB_LATEST_URL =
  'https://github.com/OpenElevo/ElevoSandbox/releases/latest/download/workspace-fuse-{platform}-{arch}';

/**
 * Get current platform and architecture
 */
function getPlatformInfo(): { platform: string; arch: string } {
  const system = os.platform();
  const machine = os.arch();

  // Normalize platform
  let plat: string;
  if (system === 'darwin') {
    plat = 'darwin';
  } else if (system === 'linux') {
    plat = 'linux';
  } else {
    throw new Error(`Unsupported platform: ${system}`);
  }

  // Normalize architecture
  let arch: string;
  if (machine === 'x64' || machine === 'amd64') {
    arch = 'amd64';
  } else if (machine === 'arm64' || machine === 'aarch64') {
    arch = 'arm64';
  } else {
    throw new Error(`Unsupported architecture: ${machine}`);
  }

  return { platform: plat, arch };
}

/**
 * Get the directory for storing workspace-fuse binary
 */
function getBinDir(): string {
  // Try ~/.elevo/bin first
  const home = os.homedir();
  const binDir = path.join(home, '.elevo', 'bin');

  try {
    fs.mkdirSync(binDir, { recursive: true });
    return binDir;
  } catch {
    // Fall back to /usr/local/bin
    const usrLocal = '/usr/local/bin';
    try {
      fs.accessSync(usrLocal, fs.constants.W_OK);
      return usrLocal;
    } catch {
      throw new Error('Cannot find writable directory for workspace-fuse binary');
    }
  }
}

/**
 * Download a file from URL
 */
function downloadFile(url: string, destPath: string, proxy?: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const parsedUrl = new URL(url);
    const protocol = parsedUrl.protocol === 'https:' ? https : http;

    const options: http.RequestOptions = {
      hostname: parsedUrl.hostname,
      port: parsedUrl.port,
      path: parsedUrl.pathname + parsedUrl.search,
      method: 'GET',
      headers: {
        'User-Agent': 'workspace-sdk',
      },
    };

    const file = fs.createWriteStream(destPath);

    const request = protocol.get(options, (response) => {
      // Handle redirects
      if (response.statusCode === 301 || response.statusCode === 302) {
        const redirectUrl = response.headers.location;
        if (redirectUrl) {
          file.close();
          fs.unlinkSync(destPath);
          downloadFile(redirectUrl, destPath, proxy).then(resolve).catch(reject);
          return;
        }
      }

      if (response.statusCode !== 200) {
        file.close();
        fs.unlinkSync(destPath);
        reject(new Error(`Failed to download: HTTP ${response.statusCode}`));
        return;
      }

      response.pipe(file);

      file.on('finish', () => {
        file.close();
        resolve();
      });
    });

    request.on('error', (err) => {
      file.close();
      try {
        fs.unlinkSync(destPath);
      } catch {}
      reject(err);
    });

    request.setTimeout(300000, () => {
      request.destroy();
      file.close();
      try {
        fs.unlinkSync(destPath);
      } catch {}
      reject(new Error('Download timeout'));
    });
  });
}

/**
 * Try to download file from URL, returns true if successful
 */
async function tryDownloadFile(url: string, destPath: string, proxy?: string): Promise<boolean> {
  try {
    await downloadFile(url, destPath, proxy);
    return true;
  } catch {
    return false;
  }
}

/**
 * Try to download workspace-fuse binary from workspace server
 * @param serverUrl HTTP server URL (e.g., http://localhost:8080)
 */
async function tryDownloadFromServer(
  serverUrl: string,
  destPath: string,
  proxy?: string
): Promise<boolean> {
  const { platform, arch } = getPlatformInfo();

  const downloadUrl = `${serverUrl}/api/v1/downloads/workspace-fuse/${platform}/${arch}`;
  return tryDownloadFile(downloadUrl, destPath, proxy);
}

/**
 * Download workspace-fuse binary for current platform
 *
 * Download priority:
 * 1. From workspace server (if serverUrl provided and binary available)
 * 2. From GitHub Releases (fallback)
 */
async function downloadBinary(
  version: string = DEFAULT_VERSION,
  proxy?: string,
  serverUrl?: string
): Promise<string> {
  const { platform, arch } = getPlatformInfo();
  const binDir = getBinDir();
  const binPath = path.join(binDir, 'workspace-fuse');
  const tempPath = binPath + '.tmp';

  try {
    let downloaded = false;

    // Try server first if URL provided
    if (serverUrl) {
      downloaded = await tryDownloadFromServer(serverUrl, tempPath, proxy);
    }

    // Fallback to GitHub
    if (!downloaded) {
      let url: string;
      if (version === 'latest') {
        url = GITHUB_LATEST_URL.replace('{platform}', platform).replace('{arch}', arch);
      } else {
        url = GITHUB_RELEASE_URL.replace('{version}', version)
          .replace('{platform}', platform)
          .replace('{arch}', arch);
      }

      if (!(await tryDownloadFile(url, tempPath, proxy))) {
        throw new Error('Failed to download workspace-fuse from both server and GitHub');
      }
    }

    // Make executable
    fs.chmodSync(tempPath, 0o755);

    // Verify it's a valid executable
    try {
      execSync(`"${tempPath}" --version`, { stdio: 'pipe', timeout: 10000 });
    } catch (e: any) {
      throw new Error(`Downloaded binary is not valid: ${e.message}`);
    }

    // Move to final location
    fs.renameSync(tempPath, binPath);
    return binPath;
  } catch (e) {
    try {
      fs.unlinkSync(tempPath);
    } catch {}
    throw e;
  }
}

/**
 * Ensure workspace-fuse binary is available
 */
async function ensureBinary(
  version: string = DEFAULT_VERSION,
  forceDownload: boolean = false,
  proxy?: string,
  serverUrl?: string
): Promise<string> {
  const binDir = getBinDir();
  const binPath = path.join(binDir, 'workspace-fuse');

  if (fs.existsSync(binPath) && !forceDownload) {
    // Verify it works
    try {
      execSync(`"${binPath}" --version`, { stdio: 'pipe', timeout: 10000 });
      return binPath;
    } catch {}
  }

  return downloadBinary(version, proxy, serverUrl);
}

/**
 * Options for FUSE mount
 */
export interface FuseMountOptions {
  /** gRPC server URL */
  server: string;
  /** Workspace ID to mount */
  workspaceId: string;
  /** Authentication token (optional if server doesn't require auth) */
  token?: string;
  /** Local mount point (auto-created if not specified) */
  mountPoint?: string;
  /** Path to workspace-fuse binary */
  binaryPath?: string;
  /** Metadata cache TTL in seconds (default: 5) */
  cacheTtl?: number;
  /** Read cache size in MB (default: 256) */
  readCacheSize?: number;
  /** Block size for reads (default: 128KB) */
  blockSize?: number;
  /** Allow other users to access the mount */
  allowOther?: boolean;
  /** Allow root to access the mount */
  allowRoot?: boolean;
  /** Mount as read-only */
  readOnly?: boolean;
  /** Enable debug logging */
  debug?: boolean;
}

/**
 * Represents an active FUSE mount for a workspace
 */
export class FuseMount {
  private readonly server: string;
  private readonly workspaceId: string;
  private readonly token?: string;
  private _mountPoint: string;
  private _binaryPath: string | null;
  private readonly cacheTtl: number;
  private readonly readCacheSize: number;
  private readonly blockSize: number;
  private readonly allowOther: boolean;
  private readonly allowRoot: boolean;
  private readonly readOnly: boolean;
  private readonly debug: boolean;

  private _tempDir: string | null = null;
  private _process: ChildProcess | null = null;
  private _mounted: boolean = false;

  constructor(options: FuseMountOptions) {
    this.server = options.server;
    this.workspaceId = options.workspaceId;
    this.token = options.token;
    this._mountPoint = options.mountPoint ?? '';
    this._binaryPath = options.binaryPath ?? null;
    this.cacheTtl = options.cacheTtl ?? 5;
    this.readCacheSize = options.readCacheSize ?? 256;
    this.blockSize = options.blockSize ?? 131072;
    this.allowOther = options.allowOther ?? false;
    this.allowRoot = options.allowRoot ?? false;
    this.readOnly = options.readOnly ?? false;
    this.debug = options.debug ?? false;
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
   * Alias for mountPoint
   */
  get path(): string {
    return this.mountPoint;
  }

  /**
   * Check if currently mounted
   */
  get isMounted(): boolean {
    return this._mounted && this._process !== null && this._process.exitCode === null;
  }

  /**
   * Mount the workspace
   * @param timeout Timeout in milliseconds (default: 30000)
   * @returns The mount point path
   */
  async mount(timeout: number = 30000): Promise<string> {
    if (this._mounted) {
      return this._mountPoint;
    }

    // Ensure binary is available
    if (!this._binaryPath) {
      this._binaryPath = await ensureBinary();
    }

    // Create mount point if not specified
    if (!this._mountPoint) {
      this._tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'workspace_fuse_'));
      this._mountPoint = this._tempDir;
    } else {
      fs.mkdirSync(this._mountPoint, { recursive: true });
    }

    // Place a sentinel file in the mount point directory. When the FUSE
    // filesystem mounts over the directory, the sentinel disappears — this
    // is a reliable signal that the mount is active (matches Go SDK).
    const sentinelPath = path.join(this._mountPoint, '.fuse_mount_sentinel');
    fs.writeFileSync(sentinelPath, 'pending');

    // Build command arguments
    const args = [
      'mount',
      '--server',
      this.server,
      '--workspace',
      this.workspaceId,
      '--target',
      this._mountPoint,
      '--foreground',
      '--cache-ttl',
      String(this.cacheTtl),
      '--read-cache-size',
      String(this.readCacheSize),
      '--block-size',
      String(this.blockSize),
    ];

    // Token is optional
    if (this.token) {
      args.push('--token', this.token);
    }

    if (this.allowOther) {
      args.push('--allow-other');
    }

    if (this.allowRoot) {
      args.push('--allow-root');
    }

    if (this.readOnly) {
      args.push('--read-only');
    }

    if (this.debug) {
      args.push('--debug');
    }

    // Collect stderr for error reporting
    let stderrData = '';

    // Start the FUSE process
    try {
      this._process = spawn(this._binaryPath, args, {
        stdio: ['ignore', 'pipe', 'pipe'],
      });
    } catch (e: any) {
      try { fs.unlinkSync(sentinelPath); } catch {}
      this._cleanup();
      throw new Error(`Failed to start workspace-fuse: ${e.message}`);
    }

    // Collect stderr asynchronously for error reporting
    this._process.stderr?.on('data', (chunk: Buffer) => {
      stderrData += chunk.toString();
    });

    // Wait for mount to be ready: sentinel file disappears when FUSE overlays the directory.
    // IMPORTANT: Use async fs.promises.access instead of synchronous fs.existsSync.
    // When the StorageProvider runs in the same Node.js process, a synchronous stat on the
    // FUSE mount would deadlock: the stat blocks the event loop, but the FUSE operation
    // needs the StorageProvider (same event loop) to respond. Async operations use libuv
    // worker threads, keeping the main event loop free for gRPC message processing.
    const startTime = Date.now();
    while (Date.now() - startTime < timeout) {
      // Check if process died
      if (this._process.exitCode !== null) {
        try { fs.unlinkSync(sentinelPath); } catch {}
        this._cleanup();
        throw new Error(`workspace-fuse exited unexpectedly: ${stderrData.trim()}`);
      }

      // Sentinel gone = FUSE filesystem has mounted over the directory
      let sentinelExists = true;
      try {
        await fs.promises.access(sentinelPath);
      } catch {
        sentinelExists = false;
      }
      if (!sentinelExists) {
        this._mounted = true;
        return this._mountPoint;
      }

      await this._sleep(100);
    }

    // Timeout — clean up sentinel and report
    try { fs.unlinkSync(sentinelPath); } catch {}
    this._cleanup();
    const errMsg = stderrData.trim();
    if (errMsg) {
      throw new Error(`Timeout waiting for mount: ${errMsg}`);
    }
    throw new Error(`Timeout waiting for mount to be ready after ${timeout}ms`);
  }

  /**
   * Unmount the workspace
   */
  unmount(): void {
    if (!this._mounted) {
      return;
    }
    this._cleanup();
  }

  private _cleanup(): void {
    // Terminate the FUSE process
    if (this._process !== null) {
      try {
        this._process.kill('SIGTERM');
      } catch {}
      this._process = null;
    }

    // Try fusermount as fallback
    if (this._mountPoint) {
      try {
        execSync(`fusermount -u "${this._mountPoint}"`, { stdio: 'pipe', timeout: 5000 });
      } catch {
        try {
          execSync(`fusermount -uz "${this._mountPoint}"`, { stdio: 'pipe', timeout: 5000 });
        } catch {}
      }
    }

    this._mounted = false;

    // Clean up temp directory
    if (this._tempDir) {
      try {
        fs.rmdirSync(this._tempDir);
      } catch {}
      this._tempDir = null;
    }
  }

  private _sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  private _readStream(stream: NodeJS.ReadableStream | null): Promise<string> {
    return new Promise((resolve) => {
      if (!stream) {
        resolve('');
        return;
      }
      let data = '';
      stream.on('data', (chunk) => (data += chunk));
      stream.on('end', () => resolve(data));
      stream.on('error', () => resolve(data));
      setTimeout(() => resolve(data), 1000);
    });
  }
}

/**
 * Service for managing FUSE mounts for workspaces
 */
export class FuseService {
  private readonly server: string;
  private readonly defaultToken?: string;
  private readonly binaryVersion: string;
  private readonly proxy?: string;
  private readonly httpServer?: string;
  private _binaryPath: string | null = null;
  private _mounts: Map<string, FuseMount> = new Map();

  /**
   * Create a FUSE service
   * @param server gRPC server URL
   * @param defaultToken Default authentication token
   * @param binaryVersion workspace-fuse version to use
   * @param proxy HTTP proxy for downloading binary
   * @param httpServer HTTP server URL for downloading binary (optional, auto-derived from server if not set)
   */
  constructor(
    server: string,
    defaultToken?: string,
    binaryVersion: string = DEFAULT_VERSION,
    proxy?: string,
    httpServer?: string
  ) {
    this.server = server;
    this.defaultToken = defaultToken;
    this.binaryVersion = binaryVersion;
    this.proxy = proxy;
    this.httpServer = httpServer;
  }

  private async _ensureBinary(): Promise<string> {
    if (!this._binaryPath) {
      this._binaryPath = await ensureBinary(this.binaryVersion, false, this.proxy, this.httpServer);
    }
    return this._binaryPath;
  }

  /**
   * Mount a workspace via FUSE
   * @param workspaceId Workspace ID to mount
   * @param options Mount options
   * @returns FuseMount instance
   *
   * @example
   * ```typescript
   * const mount = await fuse.mount('workspace-123');
   * try {
   *   await mount.mount();
   *   // Access files at mount.path
   *   fs.writeFileSync(`${mount.path}/test.txt`, 'Hello');
   * } finally {
   *   mount.unmount();
   * }
   * ```
   */
  async mount(
    workspaceId: string,
    options?: {
      token?: string;
      mountPoint?: string;
      cacheTtl?: number;
      readCacheSize?: number;
      blockSize?: number;
      allowOther?: boolean;
      allowRoot?: boolean;
      readOnly?: boolean;
      debug?: boolean;
    }
  ): Promise<FuseMount> {
    const token = options?.token ?? this.defaultToken;
    // Token is now optional - server may not require authentication

    // Check if already mounted
    const existing = this._mounts.get(workspaceId);
    if (existing?.isMounted) {
      return existing;
    }

    const mount = new FuseMount({
      server: this.server,
      workspaceId,
      token,
      mountPoint: options?.mountPoint,
      binaryPath: await this._ensureBinary(),
      cacheTtl: options?.cacheTtl,
      readCacheSize: options?.readCacheSize,
      blockSize: options?.blockSize,
      allowOther: options?.allowOther,
      allowRoot: options?.allowRoot,
      readOnly: options?.readOnly,
      debug: options?.debug,
    });

    this._mounts.set(workspaceId, mount);
    return mount;
  }

  /**
   * Unmount a workspace
   * @param workspaceId Workspace ID to unmount
   */
  unmount(workspaceId: string): void {
    const mount = this._mounts.get(workspaceId);
    if (mount) {
      mount.unmount();
      this._mounts.delete(workspaceId);
    }
  }

  /**
   * Unmount all workspaces
   */
  unmountAll(): void {
    for (const mount of this._mounts.values()) {
      mount.unmount();
    }
    this._mounts.clear();
  }

  /**
   * List all active mounts
   * @returns Map of workspace_id to mount_point
   */
  listMounts(): Map<string, string> {
    const result = new Map<string, string>();
    for (const [wsId, mount] of this._mounts) {
      if (mount.isMounted) {
        result.set(wsId, mount.mountPoint);
      }
    }
    return result;
  }

  /**
   * Check if FUSE is available on this system
   */
  static isAvailable(): boolean {
    // Check for fusermount
    try {
      execSync('which fusermount', { stdio: 'pipe' });
    } catch {
      return false;
    }

    // Check for /dev/fuse
    if (!fs.existsSync('/dev/fuse')) {
      return false;
    }

    return true;
  }
}
