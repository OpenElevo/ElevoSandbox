# SDK StorageProvider Design

## Overview

Implement `StorageProvider` for TypeScript and Python SDKs, porting the full functionality from the Go SDK. This enables SDK users to share a local directory to a remote workspace via gRPC reverse stream.

Execution order: TypeScript first, then Python.

## Go SDK Reference

The Go SDK implementation spans 4 files (~2000 lines):

- `storage_provider.go` — connection loop, handshake, worker pool, data transfer, reconnection
- `storage_provider_ops.go` — 16 file operation handlers + response helpers
- `storage_provider_path.go` — `pathGuard` (openat-based TOCTOU-safe path traversal)
- `storage_provider_watch.go` — `fileWatcher` (fsnotify + event coalescing + degraded mode)

## TypeScript SDK Design

### Proto Integration

Add `client_storage.proto` to `sdk-typescript/proto/workspace/v1/` and load it in `grpc.ts` via `@grpc/proto-loader`.

### Path Safety — `PathGuard`

Two-layer defense (no native `openat` in Node.js):

1. **String validation**: reject `..` components, absolute paths
2. **Resolve + lstat**: `path.resolve` then verify `startsWith(rootDir + sep)`, `lstat` to reject symlinks at leaf

### File Operations — 16 Handlers

All operations use `fs/promises`:

| Operation | Node.js API |
|---|---|
| Stat | `fs.lstat` |
| ListDir | `fs.readdir` + `fs.lstat`, paginated at 200 entries |
| Exists | `fs.access` |
| ReadFileRange | `fs.open` + `fileHandle.read(offset, length)` |
| WriteFileAt | `fs.open` + `fileHandle.write(offset)` |
| CreateFile | `fs.open` with O_CREAT, O_EXCL |
| Mkdir | `fs.mkdir({ recursive })` |
| RemoveFile | `fs.unlink` |
| RemoveDir | `fs.rm({ recursive })` |
| Rename | `fs.rename` (NOREPLACE simulated via link+unlink) |
| Copy | `fs.cp({ recursive })` |
| SetFileSize | `fs.truncate` |
| SetPermissions | `fs.chmod` |
| SetTimes | `fs.utimes` |
| Symlink | `fs.symlink` |
| ReadLink | `fs.readlink` |
| StatFs | Node.js `statfs` (18.15+) |

### Connection Management — `StorageProvider`

```typescript
interface StorageProviderConfig {
  localDir: string;
  workspaceId: string;
  token: string;
  workerPoolSize?: number;        // default 64
  responseBufferSize?: number;    // default 256
  maxConcurrentDataStreams?: number; // default 8
  operationTimeoutMs?: number;    // default 10000
}

class StorageProvider {
  share(signal?: AbortSignal): Promise<void>;
  stop(): void;
  isConnected(): boolean;
}
```

Key mechanisms:
- Bidirectional gRPC stream via `client.Connect()`
- Response serialization via async queue (single writer)
- Worker pool using Promise-based semaphore (default 64 concurrent)
- Exponential backoff reconnection: 1s -> 30s max
- Heartbeat: Ping -> Pong

### File Watching — `FileWatcher`

Library: `chokidar` or `@parcel/watcher`.

- Event coalescing: 50ms window + 200ms max-latency cap
- Default ignore: `.git`, `node_modules`, `__pycache__`, `target`, `build`, `.elevo`
- `.elevoignore` support
- Degraded mode: 5s full_purge polling on watcher errors

### Data Stream Transfer

- **ReadFileStream**: open local file -> client-streaming 64KB chunks
- **WriteFileStream**: server-streaming receive -> write to local file, cleanup partial on failure
- Concurrency: semaphore default 8

### File Structure

```
sdk-typescript/
  proto/workspace/v1/
    client_storage.proto          # NEW
  src/
    services/
      storage-provider.ts         # Connection management + main loop
      storage-provider-ops.ts     # 16 operation handlers
      storage-provider-path.ts    # PathGuard
      storage-provider-watch.ts   # FileWatcher
    grpc.ts                       # Add ClientStorageServiceClient
    client.ts                     # Add newStorageProvider()
    types/index.ts                # Add StorageProviderConfig etc.
```

### Differences from Go SDK

| Aspect | Go SDK | TS SDK |
|---|---|---|
| Path safety | openat + O_NOFOLLOW (fd-based) | resolve + lstat + startsWith |
| Concurrency | goroutine + chan | async/await + semaphore |
| File locks | chanMutex (per-file) | Map<string, Promise chain> |
| Rename flags | renameat2 (NOREPLACE/EXCHANGE) | fs.rename + link/unlink for NOREPLACE |
| File watching | fsnotify | chokidar or @parcel/watcher |

## Python SDK Design (Phase 2)

### Path Safety — `PathGuard`

Python 3 natively supports `dir_fd` parameter in `os.open()` (backed by `openat` syscall), enabling fd-based path traversal identical to Go SDK.

### Concurrency

`asyncio` + `grpc.aio` for bidirectional streaming. `asyncio.Semaphore` for worker pool and data stream limits.

### File Watching

`watchdog` library with same coalescing strategy.

### Details deferred until TypeScript SDK is complete.
