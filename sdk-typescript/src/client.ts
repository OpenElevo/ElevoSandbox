/**
 * Workspace SDK Client - Main entry point for SDK usage via gRPC
 */

import * as grpc from '@grpc/grpc-js';
import { createClients } from './grpc';
import { WorkspaceService } from './services/workspace';
import { SandboxService } from './services/sandbox';
import { ProcessService } from './services/process';
import { PtyService } from './services/pty';

/**
 * Client options
 */
export interface ClientOptions {
  /** API key for authentication */
  apiKey?: string;
  /** gRPC credentials (defaults to insecure) */
  credentials?: grpc.ChannelCredentials;
}

/**
 * Main client for interacting with the Workspace service via gRPC
 */
export class WorkspaceClient {
  /** Workspace management service */
  public readonly workspace: WorkspaceService;
  /** Sandbox management service */
  public readonly sandbox: SandboxService;
  /** Process execution service */
  public readonly process: ProcessService;
  /** PTY terminal service */
  public readonly pty: PtyService;

  private readonly clients: ReturnType<typeof createClients>;

  /**
   * Create a new WorkspaceClient
   *
   * @param serverAddr - gRPC server address (e.g., "localhost:9090")
   * @param options - Client options
   */
  constructor(serverAddr: string, options: ClientOptions = {}) {
    const credentials = options.credentials || grpc.credentials.createInsecure();
    this.clients = createClients(serverAddr, credentials);

    this.workspace = new WorkspaceService(this.clients.workspace, options.apiKey);
    this.sandbox = new SandboxService(this.clients.sandbox, options.apiKey);
    this.process = new ProcessService(this.clients.process, options.apiKey);
    this.pty = new PtyService(this.clients.pty, options.apiKey);
  }

  /**
   * Close all gRPC connections
   */
  close(): void {
    this.clients.workspace.close();
    this.clients.sandbox.close();
    this.clients.process.close();
    this.clients.pty.close();
  }
}

// Re-export types
export * from './types';
export * from './errors';
export { PtySession } from './services/pty';
