/**
 * FileSystem service for file operations
 */

import { AxiosInstance } from 'axios';
import { FileInfo } from '../types';

/**
 * Service for file operations in sandboxes
 */
export class FileSystemService {
  constructor(
    private readonly httpClient: AxiosInstance,
    private readonly apiUrl: string
  ) {}

  /**
   * Read a file
   */
  async read(sandboxId: string, path: string): Promise<string> {
    const response = await this.httpClient.get(
      `/sandboxes/${sandboxId}/files`,
      { params: { path } }
    );
    return response.data.content;
  }

  /**
   * Read a file as bytes
   */
  async readBytes(sandboxId: string, path: string): Promise<Uint8Array> {
    const response = await this.httpClient.get(
      `/sandboxes/${sandboxId}/files`,
      {
        params: { path },
        responseType: 'arraybuffer',
      }
    );
    return new Uint8Array(response.data);
  }

  /**
   * Write a file
   */
  async write(sandboxId: string, path: string, content: string | Uint8Array): Promise<void> {
    const isBuffer = content instanceof Uint8Array;
    await this.httpClient.put(
      `/sandboxes/${sandboxId}/files`,
      isBuffer ? Buffer.from(content) : { content },
      {
        params: { path },
        headers: isBuffer ? { 'Content-Type': 'application/octet-stream' } : undefined,
      }
    );
  }

  /**
   * Create a directory
   */
  async mkdir(sandboxId: string, path: string, recursive: boolean = false): Promise<void> {
    await this.httpClient.post(`/sandboxes/${sandboxId}/files/mkdir`, {
      path,
      recursive,
    });
  }

  /**
   * List directory contents
   */
  async list(sandboxId: string, path: string): Promise<FileInfo[]> {
    const response = await this.httpClient.get(
      `/sandboxes/${sandboxId}/files/list`,
      { params: { path } }
    );
    return response.data.files.map((f: any) => this.transformFileInfo(f));
  }

  /**
   * Remove a file or directory
   */
  async remove(sandboxId: string, path: string, recursive: boolean = false): Promise<void> {
    await this.httpClient.delete(`/sandboxes/${sandboxId}/files`, {
      params: { path, recursive },
    });
  }

  /**
   * Move/rename a file or directory
   */
  async move(sandboxId: string, source: string, destination: string): Promise<void> {
    await this.httpClient.post(`/sandboxes/${sandboxId}/files/move`, {
      source,
      destination,
    });
  }

  /**
   * Copy a file or directory
   */
  async copy(sandboxId: string, source: string, destination: string): Promise<void> {
    await this.httpClient.post(`/sandboxes/${sandboxId}/files/copy`, {
      source,
      destination,
    });
  }

  /**
   * Get file information
   */
  async getInfo(sandboxId: string, path: string): Promise<FileInfo> {
    const response = await this.httpClient.get(
      `/sandboxes/${sandboxId}/files/info`,
      { params: { path } }
    );
    return this.transformFileInfo(response.data);
  }

  /**
   * Check if a file or directory exists
   */
  async exists(sandboxId: string, path: string): Promise<boolean> {
    try {
      await this.getInfo(sandboxId, path);
      return true;
    } catch {
      return false;
    }
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
