/**
 * PTY service for interactive terminals
 */

import { AxiosInstance } from 'axios';
import WebSocket from 'ws';
import { PtyOptions, PtyHandle } from '../types';

/**
 * Service for managing interactive terminals
 */
export class PtyService {
  constructor(
    private readonly httpClient: AxiosInstance,
    private readonly apiUrl: string
  ) {}

  /**
   * Create a new PTY
   */
  async create(sandboxId: string, options: PtyOptions = {}): Promise<PtyHandle> {
    const response = await this.httpClient.post(`/sandboxes/${sandboxId}/pty`, {
      cols: options.cols || 80,
      rows: options.rows || 24,
      shell: options.shell,
      env: options.env,
    });

    const ptyId = response.data.id;
    const cols = response.data.cols;
    const rows = response.data.rows;

    // Create WebSocket connection
    const wsUrl = this.apiUrl.replace(/^http/, 'ws');
    const ws = new WebSocket(`${wsUrl}/api/v1/sandboxes/${sandboxId}/pty/${ptyId}`);

    let dataCallback: ((data: Uint8Array) => void) | null = null;
    let closeCallback: (() => void) | null = null;

    ws.on('message', (data: Buffer) => {
      if (dataCallback) {
        dataCallback(new Uint8Array(data));
      }
    });

    ws.on('close', () => {
      if (closeCallback) {
        closeCallback();
      }
    });

    const handle: PtyHandle = {
      id: ptyId,
      cols,
      rows,

      write: async (data: string | Uint8Array) => {
        const buffer = typeof data === 'string' ? Buffer.from(data) : Buffer.from(data);
        ws.send(buffer);
      },

      resize: async (newCols: number, newRows: number) => {
        await this.httpClient.post(`/sandboxes/${sandboxId}/pty/${ptyId}/resize`, {
          cols: newCols,
          rows: newRows,
        });
      },

      kill: async () => {
        ws.close();
        await this.httpClient.delete(`/sandboxes/${sandboxId}/pty/${ptyId}`);
      },

      onData: (callback: (data: Uint8Array) => void) => {
        dataCallback = callback;
      },

      onClose: (callback: () => void) => {
        closeCallback = callback;
      },
    };

    return handle;
  }

  /**
   * Resize a PTY
   */
  async resize(sandboxId: string, ptyId: string, cols: number, rows: number): Promise<void> {
    await this.httpClient.post(`/sandboxes/${sandboxId}/pty/${ptyId}/resize`, {
      cols,
      rows,
    });
  }

  /**
   * Kill a PTY
   */
  async kill(sandboxId: string, ptyId: string): Promise<void> {
    await this.httpClient.delete(`/sandboxes/${sandboxId}/pty/${ptyId}`);
  }
}
