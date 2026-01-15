/**
 * Workspace Client - Main entry point for the SDK
 */

import axios, { AxiosInstance, AxiosError } from 'axios';
import { SandboxService } from './services/sandbox';
import { ProcessService } from './services/process';
import { PtyService } from './services/pty';
import { FileSystemService } from './services/filesystem';
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
}

/**
 * Main client for interacting with the Workspace service
 */
export class WorkspaceClient {
  private readonly httpClient: AxiosInstance;
  private readonly options: WorkspaceClientOptions;

  /** Sandbox service for managing sandboxes */
  public readonly sandbox: SandboxService;
  /** Process service for executing commands */
  public readonly process: ProcessService;
  /** PTY service for interactive terminals */
  public readonly pty: PtyService;
  /** FileSystem service for file operations */
  public readonly filesystem: FileSystemService;

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
    this.sandbox = new SandboxService(this.httpClient, this.options.apiUrl);
    this.process = new ProcessService(this.httpClient, this.options.apiUrl);
    this.pty = new PtyService(this.httpClient, this.options.apiUrl);
    this.filesystem = new FileSystemService(this.httpClient, this.options.apiUrl);
  }

  /**
   * Check if the server is healthy
   */
  async health(): Promise<{ status: string; version: string }> {
    const response = await this.httpClient.get('/health');
    return response.data;
  }
}
