/**
 * Test: gRPC workspace operations with ElevoOne ES256 token
 *
 * Run: npx tsx examples/test-elevoone-workspace.ts
 */

import { WorkspaceClient } from '../src';

const SERVER_ADDR = '172.30.0.188:9090';
const ELEVOONE_TOKEN = process.argv[2];

if (!ELEVOONE_TOKEN) {
  console.error('Usage: npx tsx examples/test-elevoone-workspace.ts <access_token>');
  process.exit(1);
}

async function main() {
  console.log('=== Test: Workspace operations with ElevoOne ES256 token ===\n');

  // Connect to gRPC server with ElevoOne token as apiKey
  // The SDK sends it as "Bearer <token>" in gRPC metadata,
  // and the server's auth layer dispatches ES256 tokens to OIDC verification.
  const client = new WorkspaceClient(SERVER_ADDR, {
    apiKey: ELEVOONE_TOKEN,
  });

  try {
    // 1. List existing workspaces
    console.log('1. Listing workspaces...');
    const workspaces = await client.workspace.list();
    console.log(`   Found ${workspaces.length} workspace(s):`);
    for (const ws of workspaces) {
      console.log(`   - ${ws.id} | ${ws.name || '(unnamed)'} | ${ws.storageType} | created ${ws.createdAt}`);
    }

    // 2. Create a workspace
    console.log('\n2. Creating workspace...');
    const wsName = `test-sdk-${Date.now()}`;
    const workspace = await client.workspace.create({ name: wsName });
    console.log(`   Created: id=${workspace.id}, name=${workspace.name}, storageType=${workspace.storageType}`);

    // 3. Verify the new workspace appears in the list
    console.log('\n3. Listing workspaces again...');
    const updatedWorkspaces = await client.workspace.list();
    console.log(`   Found ${updatedWorkspaces.length} workspace(s):`);
    const found = updatedWorkspaces.find(w => w.id === workspace.id);
    if (found) {
      console.log(`   ✓ New workspace ${workspace.id} ("${wsName}") confirmed in list`);
    } else {
      console.error(`   ✗ New workspace ${workspace.id} NOT found in list!`);
    }

    // 4. Get the workspace by ID
    console.log('\n4. Getting workspace by ID...');
    const fetched = await client.workspace.get(workspace.id);
    console.log(`   Got: id=${fetched.id}, name=${fetched.name}, storageType=${fetched.storageType}`);

    // 5. Write and read a file in the workspace
    console.log('\n5. Writing file to workspace...');
    await client.workspace.writeFile(workspace.id, '/hello.txt', 'Hello from ElevoOne ES256 token!');
    console.log('   File written successfully');

    console.log('\n6. Reading file from workspace...');
    const content = await client.workspace.readFile(workspace.id, '/hello.txt');
    console.log(`   Content: "${content}"`);
    if (content === 'Hello from ElevoOne ES256 token!') {
      console.log('   ✓ File content matches');
    } else {
      console.error(`   ✗ File content mismatch!`);
    }

    // 7. Cleanup: delete the workspace
    console.log('\n7. Cleaning up...');
    await client.workspace.delete(workspace.id);
    console.log(`   Deleted workspace ${workspace.id}`);

    console.log('\n=== All tests passed! ===');
  } catch (error: any) {
    console.error('\n✗ Error:', error.message || error);
    if (error.code === 16) {
      console.error('   (UNAUTHENTICATED) — token may be expired or OIDC verification failed');
    } else if (error.code === 12) {
      console.error('   (NOT_FOUND) — resource not found');
    } else if (error.code === 2) {
      console.error('   (UNKNOWN) — internal server error');
    }
    process.exit(1);
  } finally {
    client.close();
  }
}

main();
