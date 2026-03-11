/**
 * Workspace service for managing workspaces and file operations via gRPC
 */

import * as grpc from '@grpc/grpc-js';
import { Workspace, CreateWorkspaceParams, FileInfo, StorageType } from '../types';
import { WorkspaceServiceClient, createMetadata, promisifyUnary } from '../grpc';
import { convertGrpcError, isNotFound } from '../errors';

/**
 * Service for managing workspaces and file operations
 */
export class WorkspaceService {
  constructor(
    private readonly client: WorkspaceServiceClient,
    private readonly apiKey?: string
  ) {}

  private metadata(): grpc.Metadata {
    return createMetadata(this.apiKey);
  }

  // ==================== Workspace CRUD ====================

  /**
   * Create a new workspace
   */
  async create(params: CreateWorkspaceParams = {}): Promise<Workspace> {
    try {
      const request: any = {
        name: params.name,
        metadata: params.metadata || {},
      };
      if (params.storageType) {
        request.storageType = params.storageType;
      }
      const response = await promisifyUnary(
        this.client,
        this.client.createWorkspace,
        request,
        this.metadata()
      );
      return this.transformWorkspace(response.workspace);
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Get a workspace by ID
   */
  async get(id: string): Promise<Workspace> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.getWorkspace,
        { id },
        this.metadata()
      );
      return this.transformWorkspace(response.workspace);
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * List all workspaces
   */
  async list(): Promise<Workspace[]> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.listWorkspaces,
        {},
        this.metadata()
      );
      return (response.workspaces || []).map((w: any) => this.transformWorkspace(w));
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Delete a workspace
   */
  async delete(id: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.deleteWorkspace,
        { id },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  // ==================== File Operations ====================

  /**
   * Read a file from workspace
   */
  async readFile(workspaceId: string, path: string): Promise<string> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.readFile,
        { workspaceId, path },
        this.metadata()
      );
      // Content is returned as bytes, convert to string
      const content = response.content;
      if (content instanceof Buffer) {
        return content.toString('utf-8');
      }
      return content?.toString() || '';
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Read a file as bytes from workspace
   */
  async readFileBytes(workspaceId: string, path: string): Promise<Uint8Array> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.readFile,
        { workspaceId, path },
        this.metadata()
      );
      const content = response.content;
      if (content instanceof Buffer) {
        return new Uint8Array(content);
      }
      return new Uint8Array(content || []);
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Write a file to workspace
   */
  async writeFile(workspaceId: string, path: string, content: string | Uint8Array): Promise<void> {
    try {
      const contentBuffer = typeof content === 'string'
        ? Buffer.from(content, 'utf-8')
        : Buffer.from(content);
      await promisifyUnary(
        this.client,
        this.client.writeFile,
        { workspaceId, path, content: contentBuffer },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Create a directory in workspace
   */
  async mkdir(workspaceId: string, path: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.mkdir,
        { workspaceId, path },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * List directory contents in workspace
   */
  async listFiles(workspaceId: string, path: string): Promise<FileInfo[]> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.listFiles,
        { workspaceId, path },
        this.metadata()
      );
      return (response.files || []).map((f: any) => this.transformFileInfo(f));
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Delete a file or directory in workspace
   */
  async deleteFile(workspaceId: string, path: string, recursive: boolean = false): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.deleteFile,
        { workspaceId, path, recursive },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Move/rename a file or directory in workspace
   */
  async moveFile(workspaceId: string, source: string, destination: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.moveFile,
        { workspaceId, source, destination },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Copy a file or directory in workspace
   */
  async copyFile(workspaceId: string, source: string, destination: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.copyFile,
        { workspaceId, source, destination },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Get file information in workspace
   */
  async getFileInfo(workspaceId: string, path: string): Promise<FileInfo> {
    try {
      const response = await promisifyUnary(
        this.client,
        this.client.getFileInfo,
        { workspaceId, path },
        this.metadata()
      );
      return this.transformFileInfo(response.file);
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Check if a file or directory exists in workspace
   */
  async fileExists(workspaceId: string, path: string): Promise<boolean> {
    try {
      await this.getFileInfo(workspaceId, path);
      return true;
    } catch (error) {
      if (isNotFound(error)) {
        return false;
      }
      throw error;
    }
  }

  /**
   * Check if a workspace exists
   */
  async exists(id: string): Promise<boolean> {
    try {
      await this.get(id);
      return true;
    } catch (error) {
      if (isNotFound(error)) {
        return false;
      }
      throw error;
    }
  }

  // ==================== NFS Transport ====================

  /**
   * Switch a remote workspace from gRPC to NFS transport
   */
  async registerNfsTransport(workspaceId: string, nfsUrl: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.registerNfsTransport,
        { workspaceId, nfsUrl },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Switch a remote workspace from NFS back to gRPC transport
   */
  async unregisterNfsTransport(workspaceId: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.unregisterNfsTransport,
        { workspaceId },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  // ==================== Transform Helpers ====================

  /**
   * Transform proto Workspace to SDK Workspace type
   */
  private transformWorkspace(data: any): Workspace {
    const storageType = (data.storageType || 'managed') as StorageType;
    return {
      id: data.id,
      name: data.name || undefined,
      nfsUrl: data.nfsUrl || undefined,
      metadata: data.metadata || {},
      createdAt: this.transformTimestamp(data.createdAt),
      updatedAt: this.transformTimestamp(data.updatedAt),
      storageType,
      storageConfig: data.storageConfig || undefined,
    };
  }

  /**
   * Transform proto FileInfo to SDK FileInfo type
   */
  private transformFileInfo(data: any): FileInfo {
    return {
      name: data.name,
      path: data.path,
      type: data.type as 'file' | 'directory' | 'symlink',
      size: parseInt(data.size || '0', 10),
      modifiedAt: data.modifiedAt ? this.transformTimestamp(data.modifiedAt) : undefined,
    };
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
