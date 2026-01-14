# Workspace SDK for TypeScript

TypeScript/JavaScript SDK for the Unified Workspace service. Provides a fully typed API for managing sandboxes, executing commands, and interacting with containerized development environments.

## Installation

```bash
npm install @anthropic/workspace-sdk
# or
yarn add @anthropic/workspace-sdk
# or
pnpm add @anthropic/workspace-sdk
```

### Dependencies

- Node.js 18+
- axios
- eventsource (for SSE streaming)
- ws (for WebSocket PTY sessions)

## Quick Start

```typescript
import { WorkspaceClient } from '@anthropic/workspace-sdk';

const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
});

// Create a sandbox
const sandbox = await client.sandbox.create({
  template: 'workspace-test:latest',
});

console.log(`Created sandbox: ${sandbox.id}`);

// Run a command
const result = await client.process.run(sandbox.id, 'echo', {
  args: ['Hello', 'World'],
});

console.log(`Output: ${result.stdout}`);

// Cleanup
await client.sandbox.delete(sandbox.id, true);
```

## Features

- **Full TypeScript Support**: Complete type definitions for all APIs
- **Sandbox Management**: Create, list, get, and delete sandboxes
- **Process Execution**: Run commands with full control over args, env, and working directory
- **Streaming Output**: Real-time stdout/stderr streaming via SSE
- **PTY Support**: Interactive terminal sessions via WebSocket
- **File System**: Read, write, and manage files in sandboxes
- **Error Handling**: Rich error types with detailed information

## API Reference

### Client Initialization

```typescript
import { WorkspaceClient } from '@anthropic/workspace-sdk';

const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
  apiKey: 'your-api-key',        // Optional
  timeout: 60000,                 // Request timeout in ms (default: 30000)
});

// Health check
const health = await client.health();
console.log(`Server status: ${health.status}`);
```

### Sandbox Service

```typescript
import { CreateSandboxParams, SandboxState } from '@anthropic/workspace-sdk';

// Create a sandbox
const sandbox = await client.sandbox.create({
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
import { RunCommandOptions, ProcessEvent } from '@anthropic/workspace-sdk';

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
import { PtyOptions, PtyHandle } from '@anthropic/workspace-sdk';

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

### FileSystem Service

```typescript
import { FileInfo } from '@anthropic/workspace-sdk';

// Read file as Buffer
const content: Buffer = await client.filesystem.read(sandboxId, '/workspace/config.json');

// Read file as string
const text: string = await client.filesystem.readString(sandboxId, '/workspace/README.md');

// Write file from Buffer
await client.filesystem.write(sandboxId, '/workspace/data.bin', Buffer.from([0x00, 0x01]));

// Write file from string
await client.filesystem.writeString(sandboxId, '/workspace/hello.txt', 'Hello, World!');

// Create directory
await client.filesystem.mkdir(sandboxId, '/workspace/src/components', true); // recursive

// List directory
const files: FileInfo[] = await client.filesystem.list(sandboxId, '/workspace');
for (const f of files) {
  console.log(`${f.type.padEnd(10)} ${f.size.toString().padStart(8)} ${f.name}`);
}

// Get file info
const info = await client.filesystem.stat(sandboxId, '/workspace/file.txt');

// Check if exists
const exists = await client.filesystem.exists(sandboxId, '/workspace/file.txt');

// Remove file
await client.filesystem.remove(sandboxId, '/workspace/temp.txt');

// Remove directory recursively
await client.filesystem.remove(sandboxId, '/workspace/old_dir', true);

// Move/rename
await client.filesystem.move(sandboxId, '/workspace/old.txt', '/workspace/new.txt');

// Copy
await client.filesystem.copy(sandboxId, '/workspace/src.txt', '/workspace/dst.txt');
```

## Error Handling

```typescript
import {
  WorkspaceError,
  SandboxNotFoundError,
  TemplateNotFoundError,
  FileNotFoundError,
  PermissionDeniedError,
  ProcessTimeoutError,
  PtyNotFoundError,
  AgentNotConnectedError,
} from '@anthropic/workspace-sdk';

try {
  const sandbox = await client.sandbox.get('invalid-id');
} catch (error) {
  if (error instanceof SandboxNotFoundError) {
    console.log(`Sandbox not found`);
  } else if (error instanceof WorkspaceError) {
    console.log(`API error [${error.code}]: ${error.message}`);
  } else {
    console.log(`Unknown error: ${error}`);
  }
}

// Process errors
try {
  const result = await client.process.run(sandboxId, 'invalid-command');
} catch (error) {
  if (error instanceof ProcessTimeoutError) {
    console.log('Command timed out');
  } else if (error instanceof WorkspaceError) {
    console.log(`Error: ${error.message}`);
  }
}
```

## Type Definitions

### Sandbox

```typescript
interface Sandbox {
  id: string;
  name?: string;
  template: string;
  state: SandboxState;  // 'starting' | 'running' | 'stopping' | 'stopped' | 'error'
  env?: Record<string, string>;
  metadata?: Record<string, string>;
  nfsUrl?: string;
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
import { WorkspaceClient } from '@anthropic/workspace-sdk';

const client = new WorkspaceClient({
  apiUrl: 'http://localhost:8080',
});

// Create multiple sandboxes concurrently
const sandboxes = await Promise.all([
  client.sandbox.create({ template: 'workspace-test:latest' }),
  client.sandbox.create({ template: 'workspace-test:latest' }),
  client.sandbox.create({ template: 'workspace-test:latest' }),
]);

console.log(`Created ${sandboxes.length} sandboxes`);

// Run commands in all sandboxes concurrently
const results = await Promise.all(
  sandboxes.map((s, i) =>
    client.process.run(s.id, 'echo', { args: [`Worker ${i}`] })
  )
);

for (const [i, result] of results.entries()) {
  console.log(`Worker ${i}: ${result.stdout.trim()}`);
}

// Cleanup all sandboxes concurrently
await Promise.all(
  sandboxes.map((s) => client.sandbox.delete(s.id, true))
);
```

## Timeout Handling

```typescript
import { WorkspaceClient } from '@anthropic/workspace-sdk';

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
import { WorkspaceClient } from '@anthropic/workspace-sdk';

// CommonJS
const { WorkspaceClient } = require('@anthropic/workspace-sdk');
```

## Examples

See the `examples/` directory for more usage examples:

- `examples/basic.ts` - Basic usage with sandbox and process operations
- `examples/streaming.ts` - Streaming command output
- `examples/concurrent.ts` - Concurrent operations with Promise.all
- `examples/error-handling.ts` - Error handling patterns
- `examples/pty-session.ts` - Interactive PTY sessions

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
