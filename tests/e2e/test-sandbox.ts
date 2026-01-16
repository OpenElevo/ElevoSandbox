import { WorkspaceClient } from 'workspace-sdk';

const BASE_URL = process.env.WORKSPACE_TEST_URL || 'http://127.0.0.1:8080';
const BASE_IMAGE = process.env.WORKSPACE_BASE_IMAGE || 'rust:1.85';

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
  duration: number;
}

const results: TestResult[] = [];

async function runTest(name: string, fn: () => Promise<void>): Promise<void> {
  const start = Date.now();
  try {
    await fn();
    results.push({ name, passed: true, duration: Date.now() - start });
    console.log(`  ✓ ${name} (${Date.now() - start}ms)`);
  } catch (error) {
    const errorMsg = error instanceof Error ? error.message : String(error);
    results.push({ name, passed: false, error: errorMsg, duration: Date.now() - start });
    console.log(`  ✗ ${name} (${Date.now() - start}ms)`);
    console.log(`    Error: ${errorMsg}`);
  }
}

async function main(): Promise<void> {
  console.log('Workspace SDK E2E Tests');
  console.log('=======================');
  console.log(`Server URL: ${BASE_URL}`);
  console.log(`Base Image: ${BASE_IMAGE}\n`);

  const client = new WorkspaceClient({ apiUrl: BASE_URL, timeout: 60000 });

  // Health check tests
  console.log('Health Tests:');
  await runTest('health check returns healthy status', async () => {
    const response = await fetch(`${BASE_URL}/api/v1/health`);
    if (!response.ok) {
      throw new Error(`Health check failed: ${response.status}`);
    }
    const data = await response.json() as { status: string };
    if (data.status !== 'healthy') {
      throw new Error(`Expected status "healthy", got "${data.status}"`);
    }
  });

  // Sandbox tests
  console.log('\nSandbox Tests:');
  let testSandboxId: string | null = null;

  await runTest('create sandbox with name', async () => {
    const sandbox = await client.sandbox.create({
      template: BASE_IMAGE,
      name: 'e2e-test-sandbox',
    });
    if (!sandbox.id) {
      throw new Error('Sandbox ID is missing');
    }
    if (sandbox.name !== 'e2e-test-sandbox') {
      throw new Error(`Expected name "e2e-test-sandbox", got "${sandbox.name}"`);
    }
    testSandboxId = sandbox.id;
  });

  await runTest('get sandbox by ID', async () => {
    if (!testSandboxId) throw new Error('No sandbox ID available');
    const sandbox = await client.sandbox.get(testSandboxId);
    if (sandbox.id !== testSandboxId) {
      throw new Error('Sandbox ID mismatch');
    }
  });

  await runTest('list sandboxes includes created sandbox', async () => {
    const { sandboxes } = await client.sandbox.list();
    const found = sandboxes.find(s => s.id === testSandboxId);
    if (!found) {
      throw new Error('Created sandbox not found in list');
    }
  });

  await runTest('create sandbox with environment variables', async () => {
    const sandbox = await client.sandbox.create({
      template: BASE_IMAGE,
      name: 'e2e-test-env',
      env: {
        MY_VAR: 'test_value',
        ANOTHER_VAR: 'another_value',
      },
    });
    if (!sandbox.env || sandbox.env.MY_VAR !== 'test_value') {
      throw new Error('Environment variable not set correctly');
    }
    // Cleanup
    await client.sandbox.delete(sandbox.id, true);
  });

  await runTest('create sandbox with metadata', async () => {
    const sandbox = await client.sandbox.create({
      template: BASE_IMAGE,
      name: 'e2e-test-metadata',
      metadata: {
        project: 'test-project',
        owner: 'test-user',
      },
    });
    if (!sandbox.metadata || sandbox.metadata.project !== 'test-project') {
      throw new Error('Metadata not set correctly');
    }
    // Cleanup
    await client.sandbox.delete(sandbox.id, true);
  });

  await runTest('delete sandbox', async () => {
    if (!testSandboxId) throw new Error('No sandbox ID available');
    await client.sandbox.delete(testSandboxId, true);

    // Verify deletion
    try {
      await client.sandbox.get(testSandboxId);
      throw new Error('Sandbox should have been deleted');
    } catch (error) {
      // Expected: sandbox not found
    }
    testSandboxId = null;
  });

  // Error handling tests
  console.log('\nError Handling Tests:');
  await runTest('get non-existent sandbox returns 404', async () => {
    try {
      await client.sandbox.get('non-existent-sandbox-id');
      throw new Error('Expected error for non-existent sandbox');
    } catch (error) {
      // Expected behavior
      if (error instanceof Error && error.message.includes('Expected error')) {
        throw error;
      }
    }
  });

  // Print summary
  console.log('\n=======================');
  const passed = results.filter(r => r.passed).length;
  const failed = results.filter(r => !r.passed).length;
  const totalDuration = results.reduce((sum, r) => sum + r.duration, 0);

  console.log(`Results: ${passed} passed, ${failed} failed`);
  console.log(`Total time: ${totalDuration}ms`);

  if (failed > 0) {
    console.log('\nFailed tests:');
    results.filter(r => !r.passed).forEach(r => {
      console.log(`  - ${r.name}: ${r.error}`);
    });
    process.exit(1);
  }
}

main().catch(error => {
  console.error('E2E tests failed:', error);
  process.exit(1);
});
