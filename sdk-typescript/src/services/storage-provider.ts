import * as grpc from '@grpc/grpc-js';
import * as fs from 'fs/promises';
import * as path from 'path';
import { ClientStorageServiceClient, createMetadata } from '../grpc';
import { StorageProviderConfig } from '../types';
import { PathGuard } from './storage-provider-path';
import { StorageOps, OperationResponse } from './storage-provider-ops';
import { FileLockMap, Semaphore } from './storage-provider-lock';
import { FileWatcher, FileChangeEvent } from './storage-provider-watch';

const DEFAULT_WORKER_POOL_SIZE = 64;
const DEFAULT_RESPONSE_BUFFER_SIZE = 256;
const DEFAULT_MAX_CONCURRENT_DATA_STREAMS = 8;
const DEFAULT_OPERATION_TIMEOUT_MS = 10000;
const DATA_STREAM_CHUNK_SIZE = 64 * 1024; // 64KB
const LIST_DIR_PAGE_SIZE = 200;

/**
 * StorageProvider shares a local directory with the Server via gRPC reverse stream.
 *
 * The Server sends file operation requests through a bidirectional Connect stream;
 * the provider executes them on the local filesystem and returns the results.
 * A FileWatcher monitors changes and pushes notifications.
 * Large file transfers use separate ReadFileStream/WriteFileStream RPCs.
 */
export class StorageProvider {
  private readonly config: Required<StorageProviderConfig>;
  private readonly grpcClient: ClientStorageServiceClient;

  private pathGuard: PathGuard | null = null;
  private storageOps: StorageOps | null = null;
  private fileLocks: FileLockMap | null = null;
  private fileWatcher: FileWatcher | null = null;
  private workerSemaphore: Semaphore | null = null;
  private dataStreamSemaphore: Semaphore | null = null;

  private abortController: AbortController | null = null;
  private _connected = false;
  private stream: grpc.ClientDuplexStream<any, any> | null = null;

  // Incoming message queue: a persistent 'data' listener pushes messages here,
  // keeping the stream in flowing mode so HTTP/2 flow control (WINDOW_UPDATE)
  // is never disrupted. recvMessage() drains from this queue.
  private incomingQueue: any[] = [];
  private incomingWaiters: Array<(msg: any | null) => void> = [];
  private streamEnded = false;
  private streamError: Error | null = null;

  // Response queue: all writes to the gRPC stream go through this queue
  // to serialize concurrent writes (gRPC streams are not thread-safe).
  private responseQueue: Array<any> = [];
  private draining = false;

  // Inflight operation tracking — wait for all to complete before returning
  // from connectAndServe, preventing goroutine-equivalent leaks between reconnects.
  private inflightOps = new Set<Promise<void>>();

  constructor(grpcClient: ClientStorageServiceClient, config: StorageProviderConfig) {
    this.grpcClient = grpcClient;
    this.config = {
      localDir: config.localDir,
      workspaceId: config.workspaceId,
      token: config.token,
      workerPoolSize: config.workerPoolSize ?? DEFAULT_WORKER_POOL_SIZE,
      responseBufferSize: config.responseBufferSize ?? DEFAULT_RESPONSE_BUFFER_SIZE,
      maxConcurrentDataStreams: config.maxConcurrentDataStreams ?? DEFAULT_MAX_CONCURRENT_DATA_STREAMS,
      operationTimeoutMs: config.operationTimeoutMs ?? DEFAULT_OPERATION_TIMEOUT_MS,
    };
  }

  /** Whether the provider is currently connected to the server. */
  isConnected(): boolean {
    return this._connected;
  }

  /** Stop the provider, causing share() to return. */
  stop(): void {
    this.abortController?.abort();
  }

  /**
   * Start the storage provider. Connects to the server, performs the handshake,
   * and serves file operations until stopped. Reconnects automatically with
   * exponential backoff on connection errors.
   */
  async share(signal?: AbortSignal): Promise<void> {
    this.abortController = new AbortController();
    const internalSignal = this.abortController.signal;

    // Link external signal if provided.
    if (signal) {
      signal.addEventListener('abort', () => this.abortController?.abort(), { once: true });
    }

    // Initialize components.
    this.pathGuard = new PathGuard(this.config.localDir);
    this.fileLocks = new FileLockMap(this.config.operationTimeoutMs);
    this.storageOps = new StorageOps(this.pathGuard, this.fileLocks);
    this.workerSemaphore = new Semaphore(this.config.workerPoolSize);
    this.dataStreamSemaphore = new Semaphore(this.config.maxConcurrentDataStreams);

    // Reconnection loop with exponential backoff.
    let backoff = 1000;
    const maxBackoff = 30000;

    while (!internalSignal.aborted) {
      let connectedSuccessfully = false;
      try {
        connectedSuccessfully = await this.connectAndServe(internalSignal);
      } catch (err) {
        // Connection error — will reconnect.
        if (!internalSignal.aborted) {
          const msg = err instanceof Error ? err.message : String(err);
          console.error(`[StorageProvider] connection error: ${msg}, reconnecting in ${backoff}ms`);
        }
      }
      this._connected = false;

      if (internalSignal.aborted) break;

      // Reset backoff if connection was successful.
      if (connectedSuccessfully) {
        backoff = 1000;
      }

      // Wait before reconnecting.
      await this.sleep(backoff, internalSignal);
      if (internalSignal.aborted) break;

      backoff = Math.min(backoff * 2, maxBackoff);
    }

    // Cleanup.
    await this.fileWatcher?.close();
    this.fileWatcher = null;
  }

  /**
   * One connection cycle: handshake → main loop → drain inflight → cleanup.
   * Returns true if connection was successfully established.
   */
  private async connectAndServe(signal: AbortSignal): Promise<boolean> {
    const metadata = createMetadata(this.config.token);

    // Establish bidirectional stream.
    this.stream = this.grpcClient.connect(metadata);
    this.responseQueue = [];
    this.draining = false;
    this.inflightOps.clear();

    // Reset incoming message queue and install persistent stream listeners.
    // Keeping a persistent 'data' listener ensures the Node.js Readable stays
    // in flowing mode, which is critical for HTTP/2 flow control — without it,
    // WINDOW_UPDATE frames stop being sent and the server's write window exhausts.
    this.incomingQueue = [];
    this.incomingWaiters = [];
    this.streamEnded = false;
    this.streamError = null;

    this.stream.on('data', (data: any) => {
      if (this.incomingWaiters.length > 0) {
        const waiter = this.incomingWaiters.shift()!;
        waiter(data);
      } else {
        this.incomingQueue.push(data);
      }
    });
    this.stream.on('end', () => {
      this.streamEnded = true;
      while (this.incomingWaiters.length > 0) {
        this.incomingWaiters.shift()!(null);
      }
    });
    this.stream.on('error', (err: Error) => {
      this.streamError = err;
      while (this.incomingWaiters.length > 0) {
        this.incomingWaiters.shift()!(null);
      }
    });

    try {
      // Send handshake (direct write — no concurrent writers yet).
      this.stream.write({
        handshake: {
          workspaceId: this.config.workspaceId,
          token: this.config.token,
        },
      });

      // Wait for handshake ack.
      const ack = await this.recvMessage(signal);
      if (!ack?.handshakeAck?.success) {
        const errMsg = ack?.handshakeAck?.error || 'unknown error';
        throw new Error(`handshake failed: ${errMsg}`);
      }

      this._connected = true;

      // Start file watcher (pushes change notifications to the stream).
      if (!this.fileWatcher) {
        this.fileWatcher = new FileWatcher(this.config.localDir, (events) => {
          this.sendFileChangedNotification(events);
        });
        await this.fileWatcher.start();
      } else {
        // Reconnect: send full_purge so server knows state may have drifted.
        this.enqueueResponse({
          fileChanged: { events: [], fullPurge: true },
        });
      }

      // Main loop: read and dispatch server messages.
      await this.mainLoop(signal);

      return true;
    } finally {
      // Wait for all inflight operations to complete before tearing down.
      if (this.inflightOps.size > 0) {
        await Promise.allSettled(this.inflightOps);
      }
      this.inflightOps.clear();

      // Drain any remaining queued responses.
      this.drainQueue();

      this.stream?.end();
      this.stream = null;
      this._connected = false;
    }
  }

  /** Main receive loop: dispatch server messages. */
  private async mainLoop(signal: AbortSignal): Promise<void> {
    while (!signal.aborted) {
      const msg = await this.recvMessage(signal);
      if (!msg) break;

      if (msg.operationRequest) {
        this.trackInflight(this.handleOperationRequest(msg.operationRequest));
      } else if (msg.ping) {
        this.enqueueResponse({ pong: { timestamp: msg.ping.timestamp } });
      } else if (msg.startDataTransfer) {
        this.trackInflight(this.handleDataTransfer(msg.startDataTransfer, signal));
      }
    }
  }

  /**
   * Track an inflight async operation so we can drain it on disconnect.
   */
  private trackInflight(op: Promise<void>): void {
    this.inflightOps.add(op);
    op.finally(() => this.inflightOps.delete(op));
  }

  /**
   * Handle an operation request: acquire worker semaphore, execute, send response.
   */
  private async handleOperationRequest(req: any): Promise<void> {
    const release = await this.workerSemaphore!.acquire();
    try {
      const results = await this.executeOperation(req);
      if (results) {
        for (const resp of results) {
          this.enqueueResponse({
            operationResponse: this.toProtoResponse(resp),
          });
        }
      }
    } finally {
      release();
    }
  }

  /**
   * Dispatch a single operation request to the appropriate StorageOps method.
   * Returns an array of responses (usually 1, but ListDir may return pages).
   */
  private async executeOperation(req: any): Promise<OperationResponse[] | null> {
    const cid = req.correlationId;
    const ops = this.storageOps!;

    if (req.stat) return [await ops.opStat(cid, req.stat)];
    if (req.listDir) return ops.opListDir(cid, req.listDir);
    if (req.exists) return [await ops.opExists(cid, req.exists)];
    if (req.readFileRange) return [await ops.opReadFileRange(cid, req.readFileRange)];
    if (req.writeFileAt) return [await ops.opWriteFileAt(cid, req.writeFileAt)];
    if (req.createFile) return [await ops.opCreateFile(cid, req.createFile)];
    if (req.mkdir) return [await ops.opMkdir(cid, req.mkdir)];
    if (req.removeFile) return [await ops.opRemoveFile(cid, req.removeFile)];
    if (req.removeDir) return [await ops.opRemoveDir(cid, req.removeDir)];
    if (req.rename) return [await ops.opRename(cid, req.rename)];
    if (req.copy) return [await ops.opCopy(cid, req.copy)];
    if (req.setFileSize) return [await ops.opSetFileSize(cid, req.setFileSize)];
    if (req.setPermissions) return [await ops.opSetPermissions(cid, req.setPermissions)];
    if (req.setTimes) {
      const atime = req.setTimes.atime ? protoTimestampToDate(req.setTimes.atime) : undefined;
      const mtime = req.setTimes.mtime ? protoTimestampToDate(req.setTimes.mtime) : undefined;
      return [await ops.opSetTimes(cid, { path: req.setTimes.path, atime, mtime })];
    }
    if (req.symlink) return [await ops.opSymlink(cid, req.symlink)];
    if (req.readLink) return [await ops.opReadLink(cid, req.readLink)];
    if (req.statFs !== undefined) return [await ops.opStatFs(cid)];

    return null;
  }

  /** Handle a data transfer request (read or write file stream). */
  private async handleDataTransfer(req: any, signal: AbortSignal): Promise<void> {
    // Acquire semaphore with timeout.
    const release = await this.acquireDataStreamSlot(req.transferId, signal);
    if (!release) return; // Timeout or aborted — failure already sent.

    try {
      const op = req.operation;
      if (op === 'DATA_TRANSFER_OPERATION_READ_FILE' || op === 1) {
        await this.handleReadFileTransfer(req, signal);
      } else if (op === 'DATA_TRANSFER_OPERATION_WRITE_FILE' || op === 2) {
        await this.handleWriteFileTransfer(req, signal);
      } else {
        this.sendDataTransferFailed(req.transferId, `unknown data transfer operation: ${op}`);
      }
    } catch (err: any) {
      this.sendDataTransferFailed(req.transferId, err.message);
    } finally {
      release();
    }
  }

  /**
   * Acquire a data stream semaphore slot with timeout.
   * Returns the release function, or null if timed out / aborted.
   */
  private acquireDataStreamSlot(
    transferId: string,
    signal: AbortSignal,
  ): Promise<(() => void) | null> {
    return new Promise<(() => void) | null>(resolve => {
      if (signal.aborted) { resolve(null); return; }

      let settled = false;

      const timer = setTimeout(() => {
        if (!settled) {
          settled = true;
          signal.removeEventListener('abort', onAbort);
          this.sendDataTransferFailed(transferId, 'data stream semaphore timeout');
          resolve(null);
        }
      }, this.config.operationTimeoutMs);
      if (typeof timer === 'object' && 'unref' in timer) timer.unref();

      const onAbort = () => {
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          resolve(null);
        }
      };
      signal.addEventListener('abort', onAbort, { once: true });

      this.dataStreamSemaphore!.acquire().then(release => {
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          signal.removeEventListener('abort', onAbort);
          resolve(release);
        } else {
          // We timed out but semaphore was acquired — release immediately.
          release();
        }
      });
    });
  }

  /**
   * Read a local file and stream its content to the server via ReadFileStream.
   */
  private async handleReadFileTransfer(req: any, signal: AbortSignal): Promise<void> {
    const resolved = this.pathGuard!.resolve(req.path);
    const metadata = createMetadata(this.config.token);
    const readStream = this.grpcClient.readFileStream(metadata);

    try {
      // Send header.
      readStream.write({
        header: {
          transferId: req.transferId,
          workspaceId: this.config.workspaceId,
        },
      });

      // Stream file data in 64KB chunks.
      const fh = await fs.open(resolved.fullPath, 'r');
      try {
        const stat = await fh.stat();
        let offset = 0;
        const size = Number(stat.size);

        while (offset < size && !signal.aborted) {
          const remaining = size - offset;
          const chunkLen = Math.min(remaining, DATA_STREAM_CHUNK_SIZE);
          const buf = Buffer.alloc(chunkLen);
          const { bytesRead } = await fh.read(buf, 0, chunkLen, offset);
          if (bytesRead === 0) break;
          readStream.write({ data: buf.subarray(0, bytesRead) });
          offset += bytesRead;
        }
      } finally {
        await fh.close();
      }

      // Close the client stream and wait for response.
      await new Promise<void>((resolve, reject) => {
        readStream.end();
        readStream.on('data', () => {}); // Consume response.
        readStream.on('end', () => resolve());
        readStream.on('error', (err: Error) => reject(err));
      });
    } catch (err: any) {
      // Ensure the stream is always closed on error.
      try { readStream.destroy(); } catch {}
      this.sendDataTransferFailed(req.transferId, `read transfer: ${err.message}`);
    }
  }

  /**
   * Receive data from server's WriteFileStream and write to a local file.
   */
  private async handleWriteFileTransfer(req: any, signal: AbortSignal): Promise<void> {
    const resolved = this.pathGuard!.resolve(req.path);
    const metadata = createMetadata(this.config.token);
    const writeStream = this.grpcClient.writeFileStream(
      { transferId: req.transferId, workspaceId: this.config.workspaceId },
      metadata,
    );

    // Create/truncate the target file.
    const fh = await fs.open(resolved.fullPath, 'w');
    let completed = false;

    try {
      for await (const msg of writeStream) {
        if (signal.aborted) break;
        if (msg.data) {
          const buf = Buffer.isBuffer(msg.data) ? msg.data : Buffer.from(msg.data);
          await fh.write(buf);
        } else if (msg.done) {
          completed = true;
          break;
        }
      }
    } catch (err: any) {
      this.sendDataTransferFailed(req.transferId, `write transfer: ${err.message}`);
    } finally {
      await fh.close();
      // Remove partial file on failure.
      if (!completed) {
        try { await fs.unlink(resolved.fullPath); } catch {}
      }
    }
  }

  // ==================== Proto conversion helpers ====================

  private toProtoResponse(resp: OperationResponse): any {
    const result: any = { correlationId: resp.correlationId };

    if (resp.error) {
      result.error = { code: resp.error.code, message: resp.error.message };
      return result;
    }

    if (!resp.success) return result;
    const s = resp.success;
    const protoSuccess: any = { isLast: s.isLast };

    if (s.stat) {
      protoSuccess.stat = {
        name: s.stat.name,
        path: s.stat.path,
        fileType: s.stat.fileType,
        size: s.stat.size,
        mode: s.stat.mode,
        uid: s.stat.uid,
        gid: s.stat.gid,
        modifiedAt: s.stat.modifiedAt ? dateToProtoTimestamp(s.stat.modifiedAt) : undefined,
        accessedAt: s.stat.accessedAt ? dateToProtoTimestamp(s.stat.accessedAt) : undefined,
        createdAt: s.stat.createdAt ? dateToProtoTimestamp(s.stat.createdAt) : undefined,
      };
    }
    if (s.listDir) {
      protoSuccess.listDir = {
        entries: s.listDir.entries.map(e => ({
          name: e.name,
          path: e.path,
          fileType: e.fileType,
          size: e.size,
          mode: e.mode,
          uid: e.uid,
          gid: e.gid,
          modifiedAt: e.modifiedAt ? dateToProtoTimestamp(e.modifiedAt) : undefined,
          accessedAt: e.accessedAt ? dateToProtoTimestamp(e.accessedAt) : undefined,
          createdAt: e.createdAt ? dateToProtoTimestamp(e.createdAt) : undefined,
        })),
      };
    }
    if (s.exists) protoSuccess.exists = s.exists;
    if (s.readData) protoSuccess.readData = { data: s.readData.data };
    if (s.writeData) protoSuccess.writeData = { bytesWritten: s.writeData.bytesWritten };
    if (s.readLink) protoSuccess.readLink = s.readLink;
    if (s.statFs) protoSuccess.statFs = s.statFs;
    if (s.empty) protoSuccess.empty = {};

    result.success = protoSuccess;
    return result;
  }

  // ==================== Stream I/O helpers ====================

  /**
   * Enqueue a message to be written to the gRPC stream.
   * All concurrent callers go through this queue to prevent
   * concurrent stream.write() calls which corrupt the gRPC stream.
   */
  private enqueueResponse(msg: any): void {
    this.responseQueue.push(msg);
    this.drainQueue();
  }

  /**
   * Drain the response queue one message at a time.
   * Only one drain loop runs at a time (guarded by this.draining).
   * This serializes all writes to the gRPC stream.
   */
  private drainQueue(): void {
    if (this.draining || !this.stream) return;
    this.draining = true;
    while (this.responseQueue.length > 0 && this.stream) {
      const msg = this.responseQueue.shift()!;
      try {
        this.stream.write(msg);
      } catch {
        // Stream may have been closed — drop remaining messages.
        this.responseQueue.length = 0;
        break;
      }
    }
    this.draining = false;
  }

  /**
   * Receive the next message from the incoming queue.
   *
   * The stream has a persistent 'data' listener (set up in connectAndServe)
   * that keeps it in flowing mode. This method either dequeues a buffered
   * message or registers a waiter that the 'data' listener will fulfill.
   */
  private recvMessage(signal: AbortSignal): Promise<any | null> {
    // Fast paths: stream already ended, errored, or has buffered messages.
    if (this.streamError) return Promise.reject(this.streamError);
    if (this.streamEnded) return Promise.resolve(null);
    if (!this.stream) return Promise.resolve(null);
    if (signal.aborted) return Promise.resolve(null);
    if (this.incomingQueue.length > 0) return Promise.resolve(this.incomingQueue.shift()!);

    // No buffered message — wait for the next one.
    return new Promise<any | null>((resolve, reject) => {
      let settled = false;

      const waiterFn = (msg: any | null) => {
        if (settled) return;
        settled = true;
        signal.removeEventListener('abort', onAbort);
        if (msg === null && this.streamError) {
          reject(this.streamError);
        } else {
          resolve(msg);
        }
      };

      const onAbort = () => {
        if (settled) return;
        settled = true;
        // Remove ourselves from the waiters list.
        const idx = this.incomingWaiters.indexOf(waiterFn);
        if (idx >= 0) this.incomingWaiters.splice(idx, 1);
        resolve(null);
      };

      signal.addEventListener('abort', onAbort, { once: true });
      this.incomingWaiters.push(waiterFn);
    });
  }

  private sendFileChangedNotification(events: FileChangeEvent[]): void {
    const protoEvents = events.map(e => ({
      path: e.path,
      eventType: e.eventType,
    }));
    this.enqueueResponse({
      fileChanged: {
        events: protoEvents,
        fullPurge: events.length === 0,
      },
    });
  }

  private sendDataTransferFailed(transferId: string, reason: string): void {
    this.enqueueResponse({
      dataTransferFailed: { transferId, reason },
    });
  }

  private sleep(ms: number, signal: AbortSignal): Promise<void> {
    return new Promise(resolve => {
      if (signal.aborted) { resolve(); return; }
      const timer = setTimeout(() => {
        signal.removeEventListener('abort', onAbort);
        resolve();
      }, ms);
      const onAbort = () => {
        clearTimeout(timer);
        resolve();
      };
      signal.addEventListener('abort', onAbort, { once: true });
    });
  }
}

// ==================== Timestamp helpers ====================

function protoTimestampToDate(ts: any): Date | undefined {
  if (!ts) return undefined;
  const seconds = typeof ts.seconds === 'string' ? parseInt(ts.seconds, 10) : (ts.seconds ?? 0);
  const nanos = ts.nanos ?? 0;
  return new Date(seconds * 1000 + Math.floor(nanos / 1_000_000));
}

function dateToProtoTimestamp(date: Date): { seconds: string; nanos: number } {
  const ms = date.getTime();
  const seconds = Math.floor(ms / 1000);
  const nanos = (ms % 1000) * 1_000_000;
  return { seconds: String(seconds), nanos };
}
