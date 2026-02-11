/**
 * gRPC client utilities for the Workspace SDK
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';
import { fileURLToPath } from 'url';

// Proto loader options
const PROTO_LOADER_OPTIONS: protoLoader.Options = {
  keepCase: false,
  longs: String,
  enums: String,
  defaults: true,
  oneofs: true,
};

// Get directory name for ES modules
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Proto file paths (relative to package root)
const PROTO_DIR = path.resolve(__dirname, '../../proto/workspace/v1');

// Service client types
export interface WorkspaceServiceClient extends grpc.Client {
  createWorkspace(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  getWorkspace(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  listWorkspaces(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  deleteWorkspace(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  readFile(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  writeFile(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  listFiles(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  mkdir(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  deleteFile(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  moveFile(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  copyFile(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  getFileInfo(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
}

export interface SandboxServiceClient extends grpc.Client {
  createSandbox(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  getSandbox(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  listSandboxes(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  deleteSandbox(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
}

export interface ProcessServiceClient extends grpc.Client {
  runCommand(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  runCommandStream(
    request: any,
    metadata: grpc.Metadata
  ): grpc.ClientReadableStream<any>;
  killProcess(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
}

export interface PtyServiceClient extends grpc.Client {
  createPty(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  resizePty(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  killPty(
    request: any,
    metadata: grpc.Metadata,
    callback: (error: grpc.ServiceError | null, response: any) => void
  ): void;
  ptyStream(metadata: grpc.Metadata): grpc.ClientDuplexStream<any, any>;
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

  const protoFiles = [
    path.join(PROTO_DIR, 'workspace.proto'),
    path.join(PROTO_DIR, 'sandbox.proto'),
    path.join(PROTO_DIR, 'process.proto'),
    path.join(PROTO_DIR, 'pty.proto'),
  ];

  const packageDefinition = protoLoader.loadSync(protoFiles, {
    ...PROTO_LOADER_OPTIONS,
    includeDirs: [path.resolve(__dirname, '../../proto')],
  });

  loadedProtos = grpc.loadPackageDefinition(packageDefinition);
  return loadedProtos;
}

/**
 * Create gRPC service clients
 */
export function createClients(
  serverAddr: string,
  credentials: grpc.ChannelCredentials = grpc.credentials.createInsecure()
): {
  workspace: WorkspaceServiceClient;
  sandbox: SandboxServiceClient;
  process: ProcessServiceClient;
  pty: PtyServiceClient;
} {
  const protos = loadProtos();
  const workspaceV1 = protos.workspace.v1;

  return {
    workspace: new workspaceV1.WorkspaceService(
      serverAddr,
      credentials
    ) as WorkspaceServiceClient,
    sandbox: new workspaceV1.SandboxService(
      serverAddr,
      credentials
    ) as SandboxServiceClient,
    process: new workspaceV1.ProcessService(
      serverAddr,
      credentials
    ) as ProcessServiceClient,
    pty: new workspaceV1.PtyService(serverAddr, credentials) as PtyServiceClient,
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
