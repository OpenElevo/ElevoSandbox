/**
 * Common types for the Workspace SDK
 */

/**
 * Workspace resource
 */
export interface Workspace {
  /** Unique identifier */
  id: string;
  /** Optional human-readable name */
  name?: string;
  /** NFS mount URL */
  nfsUrl?: string;
  /** Custom metadata */
  metadata?: Record<string, string>;
  /** Creation timestamp */
  createdAt: string;
  /** Last update timestamp */
  updatedAt: string;
}

/**
 * Parameters for creating a workspace
 */
export interface CreateWorkspaceParams {
  /** Optional name */
  name?: string;
  /** Custom metadata */
  metadata?: Record<string, string>;
}

/**
 * Sandbox state
 */
export type SandboxState = 'starting' | 'running' | 'stopping' | 'stopped' | 'error';

/**
 * Sandbox resource
 */
export interface Sandbox {
  /** Unique identifier */
  id: string;
  /** @deprecated Use namespaceId instead */
  workspaceId?: string;
  /** Namespace (tenant) ID this sandbox runs in */
  namespaceId?: string;
  /** Optional human-readable name */
  name?: string;
  /** Template used to create this sandbox */
  template: string;
  /** Current state */
  state: SandboxState;
  /** Root path within the namespace */
  rootPath?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Custom metadata */
  metadata?: Record<string, string>;
  /** Creation timestamp */
  createdAt: string;
  /** Last update timestamp */
  updatedAt: string;
  /** Timeout in seconds */
  timeout?: number;
  /** Error message if state is error */
  errorMessage?: string;
  /** Mounted shares */
  mounts?: SandboxMount[];
}

/**
 * Mount request for attaching a share to a sandbox
 */
export interface MountRequest {
  /** Share ID to mount */
  shareId: string;
  /** Path inside the sandbox where the share is mounted */
  mountPath: string;
}

/**
 * Sandbox mount info (after creation)
 */
export interface SandboxMount {
  /** Sandbox ID */
  sandboxId: string;
  /** Share ID */
  shareId: string;
  /** Mount path inside the sandbox */
  mountPath: string;
}

/**
 * Parameters for creating a sandbox
 */
export interface CreateSandboxParams {
  /** @deprecated Use namespaceId instead */
  workspaceId?: string;
  /** Namespace (tenant) ID — usually set automatically from API key */
  namespaceId?: string;
  /** Template to use */
  template?: string;
  /** Optional name */
  name?: string;
  /** Root path within the namespace (default: /) */
  rootPath?: string;
  /** Environment variables */
  env?: Record<string, string>;
  /** Custom metadata */
  metadata?: Record<string, string>;
  /** Timeout in seconds */
  timeout?: number;
  /** Shares to mount in the sandbox */
  mounts?: MountRequest[];
}

/**
 * Command result
 */
export interface CommandResult {
  /** Exit code */
  exitCode: number;
  /** Standard output */
  stdout: string;
  /** Standard error */
  stderr: string;
}

/**
 * Options for running a command
 */
export interface RunCommandOptions {
  /** Command arguments */
  args?: string[];
  /** Environment variables */
  env?: Record<string, string>;
  /** Working directory */
  cwd?: string;
  /** Timeout in milliseconds */
  timeout?: number;
}

/**
 * Process event for streaming
 */
export type ProcessEvent =
  | { type: 'stdout'; data: string }
  | { type: 'stderr'; data: string }
  | { type: 'exit'; code: number }
  | { type: 'error'; message: string };

/**
 * PTY options
 */
export interface PtyOptions {
  /** Terminal columns */
  cols?: number;
  /** Terminal rows */
  rows?: number;
  /** Shell to use */
  shell?: string;
  /** Environment variables */
  env?: Record<string, string>;
}

/**
 * PTY handle
 */
export interface PtyHandle {
  /** PTY ID */
  id: string;
  /** Terminal columns */
  cols: number;
  /** Terminal rows */
  rows: number;
  /** Write data to PTY */
  write(data: string | Uint8Array): Promise<void>;
  /** Resize PTY */
  resize(cols: number, rows: number): Promise<void>;
  /** Kill PTY */
  kill(): Promise<void>;
  /** Event handler for output */
  onData(callback: (data: Uint8Array) => void): void;
  /** Event handler for close */
  onClose(callback: () => void): void;
}

/**
 * File type
 */
export type FileType = 'file' | 'directory' | 'symlink';

/**
 * File information
 */
export interface FileInfo {
  /** File name */
  name: string;
  /** Full path */
  path: string;
  /** File type */
  type: FileType;
  /** Size in bytes */
  size: number;
  /** Last modified timestamp */
  modifiedAt?: string;
}
