/**
 * Workspace Client - Main entry point for the SDK
 */

import axios, { AxiosInstance, AxiosError } from 'axios';
import { WorkspaceService } from './services/workspace';
import { SandboxService } from './services/sandbox';
import { ProcessService } from './services/process';
import { PtyService } from './services/pty';
import { NfsService } from './services/nfs';
import { WorkspaceError, parseErrorResponse } from './errors';

/**
 * Options for creating a WorkspaceClient
 */
export interface WorkspaceClientOptions {
  /** Base URL of the workspace server */
  apiUrl: string;
  /** API key for authentication (optional) */
  apiKey?: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** NFS server host for mounting workspaces (optional) */
  nfsHost?: string;
  /** NFS server port (default: 2049) */
  nfsPort?: number;
}

/**
 * Main client for interacting with the Workspace service
 */
export class WorkspaceClient {
  private readonly httpClient: AxiosInstance;
  private readonly options: WorkspaceClientOptions;

  /** Workspace service for managing workspaces and file operations */
  public readonly workspace: WorkspaceService;
  /** Sandbox service for managing sandboxes */
  public readonly sandbox: SandboxService;
  /** Process service for executing commands */
  public readonly process: ProcessService;
  /** PTY service for interactive terminals */
  public readonly pty: PtyService;
  /** NFS service for mounting workspaces */
  public readonly nfs: NfsService;

  constructor(options: WorkspaceClientOptions) {
    this.options = {
      timeout: 30000,
      ...options,
    };

    // Create HTTP client
    this.httpClient = axios.create({
      baseURL: `${this.options.apiUrl}/api/v1`,
      timeout: this.options.timeout,
      headers: {
        'Content-Type': 'application/json',
        ...(this.options.apiKey && { 'Authorization': `Bearer ${this.options.apiKey}` }),
      },
    });

    // Add error interceptor
    this.httpClient.interceptors.response.use(
      (response) => response,
      (error: AxiosError) => {
        throw parseErrorResponse(error);
      }
    );

    // Initialize services
    this.workspace = new WorkspaceService(this.httpClient, this.options.apiUrl);
    this.sandbox = new SandboxService(this.httpClient, this.options.apiUrl);
    this.process = new ProcessService(this.httpClient, this.options.apiUrl);
    this.pty = new PtyService(this.httpClient, this.options.apiUrl);
    this.nfs = new NfsService(this.options.nfsHost, this.options.nfsPort ?? 2049);
  }

  /**
   * Check if the server is healthy
   */
  async health(): Promise<{ status: string; version: string }> {
    const response = await this.httpClient.get('/health');
    return response.data;
  }
}
