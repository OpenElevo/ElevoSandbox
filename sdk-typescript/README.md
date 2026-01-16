# Workspace SDK for TypeScript

TypeScript/JavaScript SDK for the Elevo Workspace service. Provides a fully typed API for managing workspaces, sandboxes, executing commands, and interacting with containerized development environments.

## Installation

```bash
npm install @elevo/workspace-sdk
# or
yarn add @elevo/workspace-sdk
# or
pnpm add @elevo/workspace-sdk
```

### Dependencies

- Node.js 18+
- axios
- eventsource (for SSE streaming)
- ws (for WebSocket PTY sessions)

## Quick Start

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk';

const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
});

// Create a workspace (persistent storage)
const workspace = await client.workspace.create({
  name: 'my-workspace',
});

console.log(`Created workspace: ${workspace.id}`);

// Create a sandbox bound to the workspace
const sandbox = await client.sandbox.create({
  workspaceId: workspace.id,
  template: 'workspace-test:latest',
});

console.log(`Created sandbox: ${sandbox.id}`);

// Run a command
const result = await client.process.run(sandbox.id, 'echo', {
  args: ['Hello', 'World'],
});

console.log(`Output: ${result.stdout}`);

// Write a file to workspace
await client.workspace.writeFile(workspace.id, 'hello.txt', 'Hello, World!');

// Read the file
const content = await client.workspace.readFile(workspace.id, 'hello.txt');
console.log(`File content: ${content}`);

// Cleanup
await client.sandbox.delete(sandbox.id, true);
await client.workspace.delete(workspace.id);
```

## Features

- **Full TypeScript Support**: Complete type definitions for all APIs
- **Workspace Management**: Create, list, get, and delete workspaces (persistent storage)
- **Sandbox Management**: Create, list, get, and delete sandboxes (bound to workspaces)
- **Process Execution**: Run commands with full control over args, env, and working directory
- **Streaming Output**: Real-time stdout/stderr streaming via SSE
- **PTY Support**: Interactive terminal sessions via WebSocket
- **File System**: Read, write, and manage files in workspaces
- **NFS Mount**: Mount workspaces via NFS for local development
- **Error Handling**: Rich error types with detailed information

## API Reference

### Client Initialization

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk';

const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
  apiKey: 'your-api-key',        // Optional
  timeout: 60000,                 // Request timeout in ms (default: 30000)
  nfsHost: '192.168.1.100',       // Optional: NFS server host for mounting
  nfsPort: 2049,                  // Optional: NFS server port (default: 2049)
});

// Health check
const health = await client.health();
console.log(`Server status: ${health.status}`);
```

### Workspace Service

```typescript
import { CreateWorkspaceParams } from '@elevo/workspace-sdk';

// Create a workspace
const workspace = await client.workspace.create({
  name: 'my-workspace',
  metadata: { project: 'demo' },
});

// Get workspace by ID
const workspace = await client.workspace.get('workspace-id');

// List all workspaces
const workspaces = await client.workspace.list();

// Delete workspace (fails if sandboxes are using it)
await client.workspace.delete('workspace-id');

// File operations on workspace
await client.workspace.writeFile(workspace.id, 'src/main.py', 'print("hello")');
const content = await client.workspace.readFile(workspace.id, 'src/main.py');
const files = await client.workspace.listFiles(workspace.id, 'src');
await client.workspace.mkdir(workspace.id, 'src/components');
await client.workspace.copyFile(workspace.id, 'src/main.py', 'src/backup.py');
await client.workspace.moveFile(workspace.id, 'src/backup.py', 'src/old.py');
await client.workspace.deleteFile(workspace.id, 'src/old.py');
const info = await client.workspace.getFileInfo(workspace.id, 'src/main.py');
const exists = await client.workspace.exists(workspace.id, 'src/main.py');
```

### Sandbox Service

```typescript
import { CreateSandboxParams, SandboxState } from '@elevo/workspace-sdk';

// Create a sandbox bound to a workspace
const sandbox = await client.sandbox.create({
  workspaceId: workspace.id,      // Required: workspace to bind to
  template: 'workspace-test:latest',
  name: 'my-sandbox',
  env: { APP_ENV: 'development' },
  metadata: { project: 'demo' },
  timeout: 3600,
});

// Get sandbox by ID
const sandbox = await client.sandbox.get('sandbox-id');

// List all sandboxes
const sandboxes = await client.sandbox.list();

// List with state filter
const runningSandboxes = await client.sandbox.list('running');

// Check if sandbox exists
const exists = await client.sandbox.exists('sandbox-id');

// Delete sandbox
await client.sandbox.delete('sandbox-id', true); // force=true

// Wait for sandbox state
const readySandbox = await client.sandbox.waitForState('sandbox-id', 'running');
```

### Process Service

```typescript
import { RunCommandOptions, ProcessEvent } from '@elevo/workspace-sdk';

// Run a command and wait for completion
const result = await client.process.run(sandboxId, 'ls', {
  args: ['-la', '/workspace'],
  env: { LC_ALL: 'C' },
  cwd: '/workspace',
  timeout: 30000,
});

console.log(`Exit code: ${result.exitCode}`);
console.log(`Stdout: ${result.stdout}`);
console.log(`Stderr: ${result.stderr}`);

// Stream command output
for await (const event of client.process.runStream(sandboxId, 'tail', {
  args: ['-f', '/var/log/app.log'],
})) {
  switch (event.type) {
    case 'stdout':
      process.stdout.write(event.data);
      break;
    case 'stderr':
      process.stderr.write(event.data);
      break;
    case 'exit':
      console.log(`\nExited with code: ${event.code}`);
      break;
    case 'error':
      console.error(`Error: ${event.message}`);
      break;
  }
}

// Kill a process
await client.process.kill(sandboxId, pid, 15); // SIGTERM
```

### PTY Service

```typescript
import { PtyOptions, PtyHandle } from '@elevo/workspace-sdk';

// Create and connect to PTY
const pty = await client.pty.connect(sandboxId, {
  cols: 120,
  rows: 40,
  shell: '/bin/bash',
  env: { TERM: 'xterm-256color' },
});

// Handle output
pty.onData((data: Uint8Array) => {
  process.stdout.write(data);
});

// Handle close
pty.onClose(() => {
  console.log('PTY closed');
});

// Write to PTY
await pty.write('ls -la\n');

// Resize PTY
await pty.resize(100, 50);

// Kill PTY
await pty.kill();
```

### NFS Service

```typescript
// Mount a workspace via NFS
const mountPoint = await client.nfs.mount(workspace.id, '/mnt/workspace');

// Check if mounted
const isMounted = await client.nfs.isMounted('/mnt/workspace');

// Unmount
await client.nfs.unmount('/mnt/workspace');
```

## Error Handling

```typescript
import {
  WorkspaceError,
  SandboxNotFoundError,
  WorkspaceNotFoundError,
  TemplateNotFoundError,
  FileNotFoundError,
  PermissionDeniedError,
  ProcessTimeoutError,
  PtyNotFoundError,
  AgentNotConnectedError,
} from '@elevo/workspace-sdk';

try {
  const sandbox = await client.sandbox.get('invalid-id');
} catch (error) {
  if (error instanceof SandboxNotFoundError) {
    console.log(`Sandbox not found`);
  } else if (error instanceof WorkspaceNotFoundError) {
    console.log(`Workspace not found`);
  } else if (error instanceof WorkspaceError) {
    console.log(`API error [${error.code}]: ${error.message}`);
  } else {
    console.log(`Unknown error: ${error}`);
  }
}

// Workspace deletion protection
try {
  await client.workspace.delete('workspace-with-sandboxes');
} catch (error) {
  // Error: Workspace has active sandboxes (code: 7002)
  console.log(error.message);
}
```

## Type Definitions

### Workspace

```typescript
interface Workspace {
  id: string;
  name?: string;
  nfsUrl?: string;                 // NFS mount URL
  metadata?: Record<string, string>;
  createdAt: string;
  updatedAt: string;
}
```

### Sandbox

```typescript
interface Sandbox {
  id: string;
  workspaceId: string;             // Bound workspace ID
  name?: string;
  template: string;
  state: SandboxState;  // 'starting' | 'running' | 'stopping' | 'stopped' | 'error'
  env?: Record<string, string>;
  metadata?: Record<string, string>;
  createdAt: string;
  updatedAt: string;
  timeout?: number;
  errorMessage?: string;
}
```

### CommandResult

```typescript
interface CommandResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}
```

### ProcessEvent

```typescript
type ProcessEvent =
  | { type: 'stdout'; data: string }
  | { type: 'stderr'; data: string }
  | { type: 'exit'; code: number }
  | { type: 'error'; message: string };
```

### FileInfo

```typescript
interface FileInfo {
  name: string;
  path: string;
  type: FileType;  // 'file' | 'directory' | 'symlink'
  size: number;
  modifiedAt?: string;
}
```

## Concurrent Usage

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk';

const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
});

// Create a shared workspace
const workspace = await client.workspace.create({ name: 'shared-workspace' });

// Create multiple sandboxes sharing the same workspace
const sandboxes = await Promise.all([
  client.sandbox.create({ workspaceId: workspace.id, template: 'workspace-test:latest' }),
  client.sandbox.create({ workspaceId: workspace.id, template: 'workspace-test:latest' }),
  client.sandbox.create({ workspaceId: workspace.id, template: 'workspace-test:latest' }),
]);

console.log(`Created ${sandboxes.length} sandboxes sharing workspace ${workspace.id}`);

// Run commands in all sandboxes concurrently
const results = await Promise.all(
  sandboxes.map((s, i) =>
    client.process.run(s.id, 'echo', { args: [`Worker ${i}`] })
  )
);

for (const [i, result] of results.entries()) {
  console.log(`Worker ${i}: ${result.stdout.trim()}`);
}

// Cleanup all sandboxes first
await Promise.all(
  sandboxes.map((s) => client.sandbox.delete(s.id, true))
);

// Then delete the workspace
await client.workspace.delete(workspace.id);
```

## Timeout Handling

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk';

// Client-level timeout
const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
  timeout: 60000,
});

// Command-level timeout
const result = await client.process.run(sandboxId, 'sleep', {
  args: ['10'],
  timeout: 5000,  // Will timeout after 5 seconds
});

// Using AbortController (Node.js 18+)
const controller = new AbortController();
const timeoutId = setTimeout(() => controller.abort(), 5000);

try {
  // Note: Requires custom implementation with abort signal support
  const result = await client.process.run(sandboxId, 'sleep', {
    args: ['60'],
  });
} catch (error) {
  if (error.name === 'AbortError') {
    console.log('Request aborted');
  }
} finally {
  clearTimeout(timeoutId);
}
```

## ESM and CommonJS Support

The SDK supports both ESM and CommonJS:

```typescript
// ESM
import { WorkspaceClient } from '@elevo/workspace-sdk';

// CommonJS
const { WorkspaceClient } = require('@elevo/workspace-sdk');
```

## Examples

See the `examples/` directory for more usage examples:

- `examples/basic.ts` - Basic usage with workspace, sandbox and process operations
- `examples/streaming.ts` - Streaming command output
- `examples/concurrent.ts` - Concurrent operations with Promise.all
- `examples/error-handling.ts` - Error handling patterns
- `examples/pty-session.ts` - Interactive PTY sessions
- `examples/nfs-mount.ts` - NFS mounting for local development

## Building from Source

```bash
# Install dependencies
npm install

# Build
npm run build

# Run tests
npm test

# Type check
npm run typecheck
```

## License

MIT License
