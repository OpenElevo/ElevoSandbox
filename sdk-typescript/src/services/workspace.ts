/**
 * Workspace service for managing workspaces and file operations
 */

import { AxiosInstance } from 'axios';
import { Workspace, CreateWorkspaceParams, FileInfo } from '../types';

/**
 * Service for managing workspaces and file operations
 */
export class WorkspaceService {
  constructor(
    private readonly httpClient: AxiosInstance,
    private readonly apiUrl: string
  ) {}

  // ==================== Workspace CRUD ====================

  /**
   * Create a new workspace
   */
  async create(params: CreateWorkspaceParams = {}): Promise<Workspace> {
    const response = await this.httpClient.post('/workspaces', params);
    return this.transformWorkspace(response.data);
  }

  /**
   * Get a workspace by ID
   */
  async get(id: string): Promise<Workspace> {
    const response = await this.httpClient.get(`/workspaces/${id}`);
    return this.transformWorkspace(response.data);
  }

  /**
   * List all workspaces
   */
  async list(): Promise<Workspace[]> {
    const response = await this.httpClient.get('/workspaces');
    return response.data.workspaces.map((w: any) => this.transformWorkspace(w));
  }

  /**
   * Delete a workspace
   */
  async delete(id: string): Promise<void> {
    await this.httpClient.delete(`/workspaces/${id}`);
  }

  // ==================== File Operations ====================

  /**
   * Read a file from workspace
   */
  async readFile(workspaceId: string, path: string): Promise<string> {
    const response = await this.httpClient.get(
      `/workspaces/${workspaceId}/files`,
      { params: { path } }
    );
    return response.data.content;
  }

  /**
   * Read a file as bytes from workspace
   */
  async readFileBytes(workspaceId: string, path: string): Promise<Uint8Array> {
    const response = await this.httpClient.get(
      `/workspaces/${workspaceId}/files`,
      {
        params: { path },
        responseType: 'arraybuffer',
      }
    );
    return new Uint8Array(response.data);
  }

  /**
   * Write a file to workspace
   */
  async writeFile(workspaceId: string, path: string, content: string | Uint8Array): Promise<void> {
    const isBuffer = content instanceof Uint8Array;
    await this.httpClient.put(
      `/workspaces/${workspaceId}/files`,
      isBuffer ? Buffer.from(content) : { content },
      {
        params: { path },
        headers: isBuffer ? { 'Content-Type': 'application/octet-stream' } : undefined,
      }
    );
  }

  /**
   * Create a directory in workspace
   */
  async mkdir(workspaceId: string, path: string): Promise<void> {
    await this.httpClient.post(`/workspaces/${workspaceId}/files/mkdir`, { path });
  }

  /**
   * List directory contents in workspace
   */
  async listFiles(workspaceId: string, path: string): Promise<FileInfo[]> {
    const response = await this.httpClient.get(
      `/workspaces/${workspaceId}/files/list`,
      { params: { path } }
    );
    return response.data.files.map((f: any) => this.transformFileInfo(f));
  }

  /**
   * Delete a file or directory in workspace
   */
  async deleteFile(workspaceId: string, path: string, recursive: boolean = false): Promise<void> {
    await this.httpClient.delete(`/workspaces/${workspaceId}/files`, {
      params: { path, recursive: recursive ? 'true' : 'false' },
    });
  }

  /**
   * Move/rename a file or directory in workspace
   */
  async moveFile(workspaceId: string, source: string, destination: string): Promise<void> {
    await this.httpClient.post(`/workspaces/${workspaceId}/files/move`, {
      source,
      destination,
    });
  }

  /**
   * Copy a file or directory in workspace
   */
  async copyFile(workspaceId: string, source: string, destination: string): Promise<void> {
    await this.httpClient.post(`/workspaces/${workspaceId}/files/copy`, {
      source,
      destination,
    });
  }

  /**
   * Get file information in workspace
   */
  async getFileInfo(workspaceId: string, path: string): Promise<FileInfo> {
    const response = await this.httpClient.get(
      `/workspaces/${workspaceId}/files/info`,
      { params: { path } }
    );
    return this.transformFileInfo(response.data);
  }

  /**
   * Check if a file or directory exists in workspace
   */
  async exists(workspaceId: string, path: string): Promise<boolean> {
    try {
      await this.getFileInfo(workspaceId, path);
      return true;
    } catch {
      return false;
    }
  }

  // ==================== Transform Helpers ====================

  /**
   * Transform API response to Workspace type
   */
  private transformWorkspace(data: any): Workspace {
    return {
      id: data.id,
      name: data.name,
      nfsUrl: data.nfs_url,
      metadata: data.metadata,
      createdAt: data.created_at,
      updatedAt: data.updated_at,
    };
  }

  /**
   * Transform API response to FileInfo type
   */
  private transformFileInfo(data: any): FileInfo {
    return {
      name: data.name,
      path: data.path,
      type: data.type,
      size: data.size,
      modifiedAt: data.modified_at,
    };
  }
}
