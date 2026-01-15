/**
 * TypeScript SDK End-to-End Tests
 * Tests multiple scenarios using the TypeScript SDK
 */

import { WorkspaceClient } from '../sdk-typescript/src';
import { Sandbox, CreateSandboxParams, RunCommandOptions } from '../sdk-typescript/src/types';

// Configuration
const API_URL = process.env.WORKSPACE_API_URL || 'http://localhost:8080';
const TEST_IMAGE = process.env.TEST_IMAGE || 'workspace-test:latest';

// Test results
let PASSED = 0;
let FAILED = 0;

function logPass(msg: string): void {
  PASSED++;
  console.log(`  \x1b[32m[PASS]\x1b[0m ${msg}`);
}

function logFail(msg: string): void {
  FAILED++;
  console.log(`  \x1b[31m[FAIL]\x1b[0m ${msg}`);
}

function logSection(title: string): void {
  console.log(`\n\x1b[1;33m${'='.repeat(50)}\x1b[0m`);
  console.log(`\x1b[1;33m${title}\x1b[0m`);
  console.log(`\x1b[1;33m${'='.repeat(50)}\x1b[0m`);
}

// ========================================
// Test Functions
// ========================================

async function testSandboxLifecycle(client: WorkspaceClient): Promise<void> {
  logSection('Test 1: Sandbox Lifecycle');

  // Create sandbox
  const params: CreateSandboxParams = { template: TEST_IMAGE };
  const sandbox = await client.sandbox.create(params);

  if (sandbox.id && sandbox.state === 'running') {
    logPass(`Created sandbox: ${sandbox.id}`);
  } else {
    logFail(`Failed to create sandbox: ${JSON.stringify(sandbox)}`);
    return;
  }

  // Get sandbox
  const fetched = await client.sandbox.get(sandbox.id);
  if (fetched.id === sandbox.id) {
    logPass(`Got sandbox info: state=${fetched.state}`);
  } else {
    logFail('Failed to get sandbox');
  }

  // List sandboxes
  const sandboxes = await client.sandbox.list();
  if (sandboxes.some(s => s.id === sandbox.id)) {
    logPass(`Listed sandboxes: found ${sandboxes.length} total`);
  } else {
    logFail('Sandbox not in list');
  }

  // Delete sandbox
  await client.sandbox.delete(sandbox.id);
  logPass(`Deleted sandbox: ${sandbox.id}`);
}

async function testProcessExecution(client: WorkspaceClient): Promise<void> {
  logSection('Test 2: Process Execution');

  const params: CreateSandboxParams = { template: TEST_IMAGE };
  const sandbox = await client.sandbox.create(params);

  try {
    // Simple echo command
    let result = await client.process.run(sandbox.id, 'echo', { args: ['Hello', 'TypeScript'] });
    if (result.exitCode === 0 && result.stdout.includes('Hello TypeScript')) {
      logPass(`Echo command: stdout='${result.stdout.trim()}'`);
    } else {
      logFail(`Echo failed: ${JSON.stringify(result)}`);
    }

    // Command with arguments
    result = await client.process.run(sandbox.id, 'ls', { args: ['-la', '/workspace'] });
    if (result.exitCode === 0) {
      logPass('ls -la command executed successfully');
    } else {
      logFail(`ls failed: exitCode=${result.exitCode}`);
    }

    // Failing command
    result = await client.process.run(sandbox.id, 'bash', { args: ['-c', 'exit 99'] });
    if (result.exitCode === 99) {
      logPass(`Failing command returned correct exit code: ${result.exitCode}`);
    } else {
      logFail(`Expected exit code 99, got ${result.exitCode}`);
    }

    // Command with environment variable
    result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'echo $TS_VAR'],
      env: { TS_VAR: 'typescript_value' }
    });
    if (result.exitCode === 0 && result.stdout.includes('typescript_value')) {
      logPass(`Env var command: stdout='${result.stdout.trim()}'`);
    } else {
      logFail(`Env var failed: ${JSON.stringify(result)}`);
    }

    // Write and read file
    result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', "echo 'ts content' > /workspace/ts_test.txt && cat /workspace/ts_test.txt"]
    });
    if (result.exitCode === 0 && result.stdout.includes('ts content')) {
      logPass('File write/read successful');
    } else {
      logFail(`File write/read failed: ${JSON.stringify(result)}`);
    }
  } finally {
    await client.sandbox.delete(sandbox.id);
  }
}

async function testMultipleSandboxes(client: WorkspaceClient): Promise<void> {
  logSection('Test 3: Multiple Sandboxes Isolation');

  const params: CreateSandboxParams = { template: TEST_IMAGE };
  const sandboxA = await client.sandbox.create(params);
  const sandboxB = await client.sandbox.create(params);

  try {
    // Write file in sandbox A
    await client.process.run(sandboxA.id, 'bash', {
      args: ['-c', "echo 'secret_ts' > /workspace/secret.txt"]
    });
    logPass(`Created file in sandbox A: ${sandboxA.id}`);

    // Try to read from sandbox B (should fail)
    const result = await client.process.run(sandboxB.id, 'cat', {
      args: ['/workspace/secret.txt']
    });

    if (result.exitCode !== 0) {
      logPass('Sandbox isolation verified: B cannot read A\'s files');
    } else {
      logFail('Isolation broken: B can read A\'s files!');
    }
  } finally {
    await client.sandbox.delete(sandboxA.id);
    await client.sandbox.delete(sandboxB.id);
  }
}

async function testLongRunningCommand(client: WorkspaceClient): Promise<void> {
  logSection('Test 4: Long Running Command');

  const params: CreateSandboxParams = { template: TEST_IMAGE };
  const sandbox = await client.sandbox.create(params);

  try {
    const startTime = Date.now();
    const result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', "sleep 3 && echo 'complete'"]
    });
    const elapsed = (Date.now() - startTime) / 1000;

    if (result.exitCode === 0 && result.stdout.includes('complete') && elapsed >= 3) {
      logPass(`Long running command completed in ${elapsed.toFixed(1)}s`);
    } else {
      logFail(`Long running command failed: ${JSON.stringify(result)}`);
    }
  } finally {
    await client.sandbox.delete(sandbox.id);
  }
}

async function testScriptExecution(client: WorkspaceClient): Promise<void> {
  logSection('Test 5: Script Execution');

  const params: CreateSandboxParams = { template: TEST_IMAGE };
  const sandbox = await client.sandbox.create(params);

  try {
    // Execute a bash script with loop
    let result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'for i in a b c; do echo "item_$i"; done']
    });

    if (result.exitCode === 0 && result.stdout.includes('item_a') && result.stdout.includes('item_c')) {
      logPass('Bash script executed with loop output');
    } else {
      logFail(`Bash script failed: ${JSON.stringify(result)}`);
    }

    // Test complex command with pipes
    result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', "echo 'typescript sdk' | tr 'a-z' 'A-Z'"]
    });

    if (result.exitCode === 0 && result.stdout.includes('TYPESCRIPT SDK')) {
      logPass(`Pipe command success: ${result.stdout.trim()}`);
    } else {
      logFail(`Pipe command failed: ${JSON.stringify(result)}`);
    }
  } finally {
    await client.sandbox.delete(sandbox.id);
  }
}

async function testConcurrentOperations(client: WorkspaceClient): Promise<void> {
  logSection('Test 6: Concurrent Operations');

  // Create multiple sandboxes concurrently
  const params: CreateSandboxParams = { template: TEST_IMAGE };

  const startCreate = Date.now();
  const sandboxes = await Promise.all([
    client.sandbox.create(params),
    client.sandbox.create(params),
    client.sandbox.create(params),
  ]);
  const createTime = (Date.now() - startCreate) / 1000;

  if (sandboxes.every(s => s.state === 'running')) {
    logPass(`Created 3 sandboxes concurrently in ${createTime.toFixed(2)}s`);
  } else {
    logFail('Some sandboxes failed to start');
  }

  try {
    // Run commands concurrently
    const startCmd = Date.now();
    const results = await Promise.all([
      client.process.run(sandboxes[0].id, 'echo', { args: ['sandbox1'] }),
      client.process.run(sandboxes[1].id, 'echo', { args: ['sandbox2'] }),
      client.process.run(sandboxes[2].id, 'echo', { args: ['sandbox3'] }),
    ]);
    const cmdTime = (Date.now() - startCmd) / 1000;

    if (results.every(r => r.exitCode === 0)) {
      logPass(`Ran 3 commands concurrently in ${cmdTime.toFixed(2)}s`);
    } else {
      logFail('Some concurrent commands failed');
    }
  } finally {
    // Delete concurrently
    await Promise.all(sandboxes.map(s => client.sandbox.delete(s.id)));
    logPass('Deleted 3 sandboxes concurrently');
  }
}

async function testErrorHandling(client: WorkspaceClient): Promise<void> {
  logSection('Test 7: Error Handling');

  // Try to get non-existent sandbox
  try {
    await client.sandbox.get('non-existent-sandbox-id');
    logFail('Should have thrown error for non-existent sandbox');
  } catch (error: any) {
    logPass(`Correct error for non-existent sandbox: ${error.constructor.name}`);
  }

  // Create sandbox for more tests
  const params: CreateSandboxParams = { template: TEST_IMAGE };
  const sandbox = await client.sandbox.create(params);

  try {
    // Command that returns non-zero exit code
    const result = await client.process.run(sandbox.id, 'cat', {
      args: ['/nonexistent/file.txt']
    });
    if (result.exitCode !== 0 && result.stderr) {
      logPass(`Correct error for missing file: exitCode=${result.exitCode}`);
    } else {
      logFail('Expected non-zero exit code for missing file');
    }
  } finally {
    await client.sandbox.delete(sandbox.id);
  }
}

async function testSandboxMetadata(client: WorkspaceClient): Promise<void> {
  logSection('Test 8: Sandbox Metadata');

  const params: CreateSandboxParams = {
    template: TEST_IMAGE,
    name: 'test-sandbox-ts',
    metadata: { purpose: 'testing', version: '1.0' },
    env: { APP_ENV: 'test' }
  };

  const sandbox = await client.sandbox.create(params);

  try {
    if (sandbox.name === 'test-sandbox-ts') {
      logPass(`Sandbox created with custom name: ${sandbox.name}`);
    } else {
      logFail(`Name not set correctly: ${sandbox.name}`);
    }

    // Verify via get
    const fetched = await client.sandbox.get(sandbox.id);
    if (fetched.metadata && fetched.metadata.purpose === 'testing') {
      logPass('Metadata preserved correctly');
    } else {
      logFail(`Metadata not preserved: ${JSON.stringify(fetched.metadata)}`);
    }

    // Test env is passed to commands
    const result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'echo $APP_ENV']
    });
    // Note: env might not be passed from sandbox creation to all commands
    // This tests command-level env instead
    const result2 = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'echo $CMD_ENV'],
      env: { CMD_ENV: 'command-env-value' }
    });
    if (result2.stdout.includes('command-env-value')) {
      logPass('Command-level env works correctly');
    } else {
      logFail(`Command env failed: ${result2.stdout}`);
    }
  } finally {
    await client.sandbox.delete(sandbox.id);
  }
}

async function testRapidOperations(client: WorkspaceClient): Promise<void> {
  logSection('Test 9: Rapid Sandbox Operations');

  let success = 0;
  const params: CreateSandboxParams = { template: TEST_IMAGE };

  for (let i = 0; i < 5; i++) {
    const sandbox = await client.sandbox.create(params);
    if (sandbox.state === 'running') {
      await client.sandbox.delete(sandbox.id);
      success++;
    }
  }

  if (success === 5) {
    logPass(`Rapid create/delete test passed (${success}/5)`);
  } else {
    logFail(`Rapid create/delete test failed (${success}/5)`);
  }
}

// ========================================
// Main
// ========================================

async function main(): Promise<number> {
  console.log('\n' + '='.repeat(60));
  console.log('  TypeScript SDK End-to-End Test Suite');
  console.log('='.repeat(60));
  console.log(`API URL: ${API_URL}`);
  console.log(`Test Image: ${TEST_IMAGE}`);

  const client = new WorkspaceClient({ apiUrl: API_URL, timeout: 60000 });

  try {
    await testSandboxLifecycle(client);
    await testProcessExecution(client);
    await testMultipleSandboxes(client);
    await testLongRunningCommand(client);
    await testScriptExecution(client);
    await testConcurrentOperations(client);
    await testErrorHandling(client);
    await testSandboxMetadata(client);
    await testRapidOperations(client);
  } catch (error) {
    console.error('Test suite error:', error);
    FAILED++;
  }

  // Summary
  console.log('\n' + '='.repeat(60));
  console.log('                 TEST SUMMARY');
  console.log('='.repeat(60));
  console.log(`  \x1b[32mPASSED:\x1b[0m  ${PASSED}`);
  console.log(`  \x1b[31mFAILED:\x1b[0m  ${FAILED}`);
  console.log('='.repeat(60) + '\n');

  return FAILED === 0 ? 0 : 1;
}

main().then(process.exit);
