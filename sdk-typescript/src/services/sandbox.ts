/**
 * Sandbox service for managing sandbox lifecycle via gRPC
 */

import * as grpc from '@grpc/grpc-js';
import { Sandbox, CreateSandboxParams, SandboxState } from '../types';
import { SandboxServiceClient, createMetadata, promisifyUnary } from '../grpc';
import { convertGrpcError } from '../errors';

// Proto SandboxState enum values
const SANDBOX_STATE_MAP: Record<string, SandboxState> = {
  'SANDBOX_STATE_STARTING': 'starting',
  'SANDBOX_STATE_RUNNING': 'running',
  'SANDBOX_STATE_STOPPING': 'stopping',
  'SANDBOX_STATE_STOPPED': 'stopped',
  'SANDBOX_STATE_ERROR': 'error',
};

/**
 * Service for managing sandboxes
 */
export class SandboxService {
  constructor(
    private readonly client: SandboxServiceClient,
    private readonly apiKey?: string
  ) {}

  private metadata(): grpc.Metadata {
    return createMetadata(this.apiKey);
  }

  /**
   * Create a new sandbox bound to a namespace
   */
  async create(params: CreateSandboxParams): Promise<Sandbox> {
    try {
      const request: any = {
        namespaceId: params.namespaceId || params.workspaceId,
        env: params.env || {},
        metadata: params.metadata || {},
      };
      // Keep workspaceId for backward compat with older servers
      if (params.workspaceId && !params.namespaceId) {
        request.workspaceId = params.workspaceId;
      }
      if (params.template) request.template = params.template;
      if (params.name) request.name = params.name;
      if (params.timeout) request.timeout = params.timeout;
      if (params.rootPath) request.rootPath = params.rootPath;
      if (params.mounts) {
        request.mounts = params.mounts.map((m) => ({
          shareId: m.shareId,
          mountPath: m.mountPath,
        }));
      }

      const response = await promisifyUnary(
        this.client,
        this.client.createSandbox,
        request,
        this.metadata()
      );
      return this.transformSandbox(response.sandbox);
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Get a sandbox by ID
   */
  async get(id: string): Promise<Sandbox> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.getSandbox,
        { id },
        this.metadata()
      );
      return this.transformSandbox(response.sandbox);
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * List all sandboxes
   */
  async list(): Promise<Sandbox[]> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.listSandboxes,
        {},
        this.metadata()
      );
      return (response.sandboxes || []).map((s: any) => this.transformSandbox(s));
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Delete a sandbox
   */
  async delete(id: string, force: boolean = false): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.deleteSandbox,
        { id, force },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Check if a sandbox exists
   */
  async exists(id: string): Promise<boolean> {
    try {
      await this.get(id);
      return true;
    } catch (error) {
      if ((error as any).code === 'NOT_FOUND') {
        return false;
      }
      throw error;
    }
  }

  /**
   * Transform proto Sandbox to SDK Sandbox type
   */
  private transformSandbox(data: any): Sandbox {
    return {
      id: data.id,
      workspaceId: data.workspaceId || data.namespaceId,
      namespaceId: data.namespaceId || data.workspaceId,
      name: data.name || undefined,
      template: data.template,
      state: this.transformState(data.state),
      rootPath: data.rootPath || '/',
      env: data.env || {},
      metadata: data.metadata || {},
      createdAt: this.transformTimestamp(data.createdAt),
      updatedAt: this.transformTimestamp(data.updatedAt),
      timeout: data.timeout ? parseInt(data.timeout, 10) : undefined,
      errorMessage: data.errorMessage || undefined,
      mounts: data.mounts?.map((m: any) => ({
        sandboxId: m.sandboxId || data.id,
        shareId: m.shareId,
        mountPath: m.mountPath,
      })),
    };
  }

  /**
   * Transform proto SandboxState to SDK SandboxState
   */
  private transformState(state: string | number): SandboxState {
    if (typeof state === 'string') {
      return SANDBOX_STATE_MAP[state] || 'starting';
    }
    // Handle numeric enum values (proto3 enum: 0=UNSPECIFIED, 1=STARTING, 2=RUNNING, etc.)
    const stateNames = ['starting', 'starting', 'running', 'stopping', 'stopped', 'error'];
    return (stateNames[state] as SandboxState) || 'starting';
  }

  /**
   * Transform proto Timestamp to ISO string
   */
  private transformTimestamp(ts: any): string {
    if (!ts) return new Date().toISOString();
    if (ts.seconds) {
      const ms = parseInt(ts.seconds, 10) * 1000 + Math.floor((ts.nanos || 0) / 1000000);
      return new Date(ms).toISOString();
    }
    return new Date().toISOString();
  }
}
