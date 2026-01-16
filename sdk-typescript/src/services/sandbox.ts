/**
 * Sandbox service for managing sandbox lifecycle
 */

import { AxiosInstance } from 'axios';
import { Sandbox, CreateSandboxParams } from '../types';

/**
 * Service for managing sandboxes
 */
export class SandboxService {
  constructor(
    private readonly httpClient: AxiosInstance,
    private readonly apiUrl: string
  ) {}

  /**
   * Create a new sandbox bound to a workspace
   */
  async create(params: CreateSandboxParams): Promise<Sandbox> {
    const response = await this.httpClient.post('/sandboxes', {
      workspace_id: params.workspaceId,
      template: params.template,
      name: params.name,
      env: params.env,
      metadata: params.metadata,
      timeout: params.timeout,
    });
    return this.transformSandbox(response.data);
  }

  /**
   * Get a sandbox by ID
   */
  async get(id: string): Promise<Sandbox> {
    const response = await this.httpClient.get(`/sandboxes/${id}`);
    return this.transformSandbox(response.data);
  }

  /**
   * List all sandboxes
   */
  async list(): Promise<Sandbox[]> {
    const response = await this.httpClient.get('/sandboxes');
    return response.data.sandboxes.map((s: any) => this.transformSandbox(s));
  }

  /**
   * Delete a sandbox
   */
  async delete(id: string): Promise<void> {
    await this.httpClient.delete(`/sandboxes/${id}`);
  }

  /**
   * Transform API response to Sandbox type
   */
  private transformSandbox(data: any): Sandbox {
    return {
      id: data.id,
      workspaceId: data.workspace_id,
      name: data.name,
      template: data.template,
      state: data.state,
      env: data.env,
      metadata: data.metadata,
      createdAt: data.created_at,
      updatedAt: data.updated_at,
      timeout: data.timeout,
      errorMessage: data.error_message,
    };
  }
}
