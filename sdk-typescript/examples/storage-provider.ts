/**
 * Example: Share a local directory with a remote workspace using StorageProvider.
 *
 * Usage:
 *   npx tsx examples/storage-provider.ts <server-addr> <workspace-id> <local-dir> <token>
 */

import { WorkspaceClient } from '../src/client';

async function main() {
  const [serverAddr, workspaceId, localDir, token] = process.argv.slice(2);

  if (!serverAddr || !workspaceId || !localDir || !token) {
    console.error(
      'Usage: npx tsx examples/storage-provider.ts <server-addr> <workspace-id> <local-dir> <token>',
    );
    process.exit(1);
  }

  const client = new WorkspaceClient(serverAddr, { apiKey: token });

  const provider = client.newStorageProvider({
    localDir,
    workspaceId,
    token,
  });

  // Handle graceful shutdown.
  const ac = new AbortController();
  process.on('SIGINT', () => {
    console.log('\nStopping storage provider...');
    ac.abort();
  });
  process.on('SIGTERM', () => {
    ac.abort();
  });

  console.log(`Sharing "${localDir}" with workspace ${workspaceId} via ${serverAddr}`);
  console.log('Press Ctrl+C to stop.\n');

  try {
    await provider.share(ac.signal);
  } finally {
    client.close();
  }

  console.log('Storage provider stopped.');
}

main().catch((err) => {
  console.error('Fatal error:', err);
  process.exit(1);
});
