/**
 * gRPC client utilities for the Workspace SDK
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';
import { existsSync } from 'fs';
import { fileURLToPath } from 'url';

// Proto loader options
const PROTO_LOADER_OPTIONS: protoLoader.Options = {
  keepCase: false,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
};

function getCurrentModulePath(): string | undefined {
  if (typeof __filename === 'string') {
    return __filename;
  }

  const stack = new Error().stack;
  const match = stack?.match(/file:\/\/[^)\s]+/);
  if (!match) {
    return undefined;
  }

  return fileURLToPath(match[0].replace(/:\d+:\d+$/, ''));
}

function getProtoDir(): string {
  const currentModulePath = getCurrentModulePath();
  if (!currentModulePath) {
    throw new Error('Unable to locate @openelevo/workspace-sdk proto files');
  }

  const moduleDir = path.dirname(currentModulePath);
  const protoDir = path.resolve(
    moduleDir,
    path.basename(moduleDir) === 'cjs'
      ? '../../proto/workspace/v1'
      : '../proto/workspace/v1'
  );

  if (existsSync(protoDir)) {
    return protoDir;
  }

  throw new Error('Unable to locate @openelevo/workspace-sdk proto files');
}

// Service client types
// Helper type for unary gRPC method signature
type UnaryMethod = (
  request: any,
  metadata: grpc.Metadata,
  callback: (error: grpc.ServiceError | null, response: any) => void
) => void;

export interface WorkspaceServiceClient extends grpc.Client {
  createWorkspace: UnaryMethod;
  getWorkspace: UnaryMethod;
  listWorkspaces: UnaryMethod;
  deleteWorkspace: UnaryMethod;
  readFile: UnaryMethod;
  writeFile: UnaryMethod;
  listFiles: UnaryMethod;
  mkdir: UnaryMethod;
  deleteFile: UnaryMethod;
  moveFile: UnaryMethod;
  copyFile: UnaryMethod;
  getFileInfo: UnaryMethod;
  registerNfsTransport: UnaryMethod;
  unregisterNfsTransport: UnaryMethod;
}

export interface SandboxServiceClient extends grpc.Client {
  createSandbox: UnaryMethod;
  getSandbox: UnaryMethod;
  listSandboxes: UnaryMethod;
  deleteSandbox: UnaryMethod;
}

export interface ProcessServiceClient extends grpc.Client {
  runCommand: UnaryMethod;
  runCommandStream(
    request: any,
    metadata: grpc.Metadata
  ): grpc.ClientReadableStream<any>;
  killProcess: UnaryMethod;
}

export interface PtyServiceClient extends grpc.Client {
  createPty: UnaryMethod;
  resizePty: UnaryMethod;
  killPty: UnaryMethod;
  ptyStream(metadata: grpc.Metadata): grpc.ClientDuplexStream<any, any>;
}

export interface ClientStorageServiceClient extends grpc.Client {
  connect(metadata: grpc.Metadata): grpc.ClientDuplexStream<any, any>;
  readFileStream(metadata: grpc.Metadata): grpc.ClientWritableStream<any>;
  writeFileStream(
    request: any,
    metadata: grpc.Metadata
  ): grpc.ClientReadableStream<any>;
}

export interface FileSystemServiceClient extends grpc.Client {
  downloadBinary(
    request: any,
    metadata: grpc.Metadata
  ): grpc.ClientReadableStream<any>;
}

// Loaded proto definitions cache
let loadedProtos: any = null;

/**
 * Load proto definitions
 */
export function loadProtos(): any {
  if (loadedProtos) {
    return loadedProtos;
  }

  const protoDir = getProtoDir();
  const protoFiles = [
    path.join(protoDir, 'workspace.proto'),
    path.join(protoDir, 'sandbox.proto'),
    path.join(protoDir, 'process.proto'),
    path.join(protoDir, 'pty.proto'),
    path.join(protoDir, 'client_storage.proto'),
    path.join(protoDir, 'filesystem.proto'),
  ];

  const packageDefinition = protoLoader.loadSync(protoFiles, {
    ...PROTO_LOADER_OPTIONS,
    includeDirs: [path.resolve(protoDir, '../..')],
  });

  loadedProtos = grpc.loadPackageDefinition(packageDefinition);
  return loadedProtos;
}

/**
 * Create gRPC service clients
 */
// Default gRPC channel options.
// - flow_control_window: 16MB — prevents HTTP/2 flow control stalls with
//   @grpc/grpc-js's pull-based read model (default 65KB is too small for
//   bidirectional streams, causing the server's send window to exhaust).
// - keepalive: detect dead connections early.
const DEFAULT_CHANNEL_OPTIONS: Record<string, any> = {
  'grpc-node.flow_control_window': 16 * 1024 * 1024,
  'grpc.keepalive_time_ms': 10000,
  'grpc.keepalive_timeout_ms': 5000,
  'grpc.keepalive_permit_without_calls': 1,
};

export function createClients(
  serverAddr: string,
  credentials: grpc.ChannelCredentials = grpc.credentials.createInsecure(),
  channelOptions?: Record<string, any>
): {
  workspace: WorkspaceServiceClient;
  sandbox: SandboxServiceClient;
  process: ProcessServiceClient;
  pty: PtyServiceClient;
  clientStorage: ClientStorageServiceClient;
  fileSystem: FileSystemServiceClient;
} {
  const protos = loadProtos();
  const workspaceV1 = protos.workspace.v1;
  const opts = { ...DEFAULT_CHANNEL_OPTIONS, ...channelOptions };

  return {
    workspace: new workspaceV1.WorkspaceService(
      serverAddr,
      credentials,
      opts
    ) as WorkspaceServiceClient,
    sandbox: new workspaceV1.SandboxService(
      serverAddr,
      credentials,
      opts
    ) as SandboxServiceClient,
    process: new workspaceV1.ProcessService(
      serverAddr,
      credentials,
      opts
    ) as ProcessServiceClient,
    pty: new workspaceV1.PtyService(serverAddr, credentials, opts) as PtyServiceClient,
    clientStorage: new workspaceV1.ClientStorageService(
      serverAddr,
      credentials,
      opts
    ) as ClientStorageServiceClient,
    fileSystem: new workspaceV1.FileSystemService(
      serverAddr,
      credentials,
      opts
    ) as FileSystemServiceClient,
  };
}

/**
 * Create gRPC metadata with auth token
 */
export function createMetadata(apiKey?: string): grpc.Metadata {
  const metadata = new grpc.Metadata();
  if (apiKey) {
    metadata.set('authorization', `Bearer ${apiKey}`);
  }
  return metadata;
}

/**
 * Promisify a gRPC unary call
 */
export function promisifyUnary<TRequest, TResponse>(
  client: grpc.Client,
  method: (
    request: TRequest,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: TResponse) => void
  ) => void,
  request: TRequest,
  metadata: grpc.Metadata
): Promise<TResponse> {
  return new Promise((resolve, reject) => {
    method.call(client, request, metadata, (error, response) => {
      if (error) {
        reject(error);
      } else {
        resolve(response);
      }
    });
  });
}
