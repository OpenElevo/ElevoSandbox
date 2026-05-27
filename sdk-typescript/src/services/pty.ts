/**
 * PTY service for interactive terminals via gRPC
 */

import * as grpc from '@grpc/grpc-js';
import { EventEmitter } from 'events';
import { PtyOptions, PtyHandle } from '../types/index.js';
import { PtyServiceClient, createMetadata, promisifyUnary } from '../grpc.js';
import { convertGrpcError } from '../errors/index.js';

/**
 * PTY session with bidirectional gRPC stream
 */
export class PtySession extends EventEmitter implements PtyHandle {
  public readonly id: string;
  public cols: number;
  public rows: number;

  private stream: grpc.ClientDuplexStream<any, any>;
  private closed = false;

  constructor(
    id: string,
    cols: number,
    rows: number,
    stream: grpc.ClientDuplexStream<any, any>
  ) {
    super();
    this.id = id;
    this.cols = cols;
    this.rows = rows;
    this.stream = stream;

    this.setupStreamHandlers();
  }

  private setupStreamHandlers(): void {
    this.stream.on('data', (data: any) => {
      if (data.output) {
        const output = data.output instanceof Buffer
          ? new Uint8Array(data.output)
          : new Uint8Array(data.output || []);
        this.emit('data', output);
      } else if (data.exitCode !== undefined) {
        this.emit('exit', data.exitCode);
        this.close();
      } else if (data.error) {
        this.emit('error', new Error(data.error));
        this.close();
      }
    });

    this.stream.on('error', (err: grpc.ServiceError) => {
      if (!this.closed) {
        this.emit('error', convertGrpcError(err));
        this.close();
      }
    });

    this.stream.on('end', () => {
      if (!this.closed) {
        this.close();
      }
    });
  }

  /**
   * Write data to PTY
   */
  async write(data: string | Uint8Array): Promise<void> {
    if (this.closed) {
      throw new Error('PTY session is closed');
    }

    const input = typeof data === 'string'
      ? Buffer.from(data, 'utf-8')
      : Buffer.from(data);

    return new Promise((resolve, reject) => {
      this.stream.write({ input }, (err: Error | null) => {
        if (err) {
          reject(err);
        } else {
          resolve();
        }
      });
    });
  }

  /**
   * Resize PTY
   */
  async resize(cols: number, rows: number): Promise<void> {
    if (this.closed) {
      throw new Error('PTY session is closed');
    }

    this.cols = cols;
    this.rows = rows;

    return new Promise((resolve, reject) => {
      this.stream.write({ resize: { cols, rows } }, (err: Error | null) => {
        if (err) {
          reject(err);
        } else {
          resolve();
        }
      });
    });
  }

  /**
   * Kill PTY
   */
  async kill(): Promise<void> {
    this.close();
  }

  /**
   * Set data callback
   */
  onData(callback: (data: Uint8Array) => void): void {
    this.on('data', callback);
  }

  /**
   * Set close callback
   */
  onClose(callback: () => void): void {
    this.on('close', callback);
  }

  /**
   * Close the PTY session
   */
  private close(): void {
    if (this.closed) return;
    this.closed = true;

    try {
      this.stream.end();
    } catch {
      // Ignore errors when closing
    }

    this.emit('close');
  }
}

/**
 * Service for managing PTY sessions
 */
export class PtyService {
  constructor(
    private readonly client: PtyServiceClient,
    private readonly apiKey?: string
  ) {}

  private metadata(): grpc.Metadata {
    return createMetadata(this.apiKey);
  }

  /**
   * Create a new PTY and establish bidirectional stream
   */
  async connect(sandboxId: string, options: PtyOptions = {}): Promise<PtySession> {
    // First create the PTY
    const handle = await this.create(sandboxId, options);

    // Then establish bidirectional stream
    const stream = this.client.ptyStream(this.metadata());

    // Send init message
    await new Promise<void>((resolve, reject) => {
      stream.write(
        {
          init: {
            sandboxId,
            ptyId: handle.id,
          },
        },
        (err: Error | null) => {
          if (err) {
            reject(err);
          } else {
            resolve();
          }
        }
      );
    });

    return new PtySession(handle.id, handle.cols, handle.rows, stream);
  }

  /**
   * Create a new PTY (without stream)
   */
  async create(sandboxId: string, options: PtyOptions = {}): Promise<{ id: string; cols: number; rows: number }> {
    try {
      const request: any = {
        sandboxId,
        cols: options.cols || 80,
        rows: options.rows || 24,
        env: options.env || {},
      };
      if (options.shell) request.shell = options.shell;

      const response = await promisifyUnary(
        this.client,
        this.client.createPty,
        request,
        this.metadata()
      );

      return {
        id: response.pty.id,
        cols: response.pty.cols,
        rows: response.pty.rows,
      };
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Resize a PTY
   */
  async resize(sandboxId: string, ptyId: string, cols: number, rows: number): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.resizePty,
        { sandboxId, ptyId, cols, rows },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Kill a PTY
   */
  async kill(sandboxId: string, ptyId: string): Promise<void> {
    try {
      await promisifyUnary(
        this.client,
        this.client.killPty,
        { sandboxId, ptyId },
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }
}
