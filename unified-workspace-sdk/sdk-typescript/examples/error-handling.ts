/**
 * Example: Error Handling
 *
 * This example demonstrates proper error handling with the SDK.
 *
 * Run: npx ts-node error-handling.ts
 */

import {
  WorkspaceClient,
  WorkspaceError,
  SandboxNotFoundError,
  TemplateNotFoundError,
  RunCommandOptions,
  CommandResult,
} from '../src';

async function main() {
  console.log('=== Error Handling Example ===\n');

  const client = new WorkspaceClient({
    apiUrl: 'http://localhost:8080',
    timeout: 30000,
  });

  // 1. Handle not found error
  console.log('1. Handling non-existent sandbox error...');
  try {
    await client.sandbox.get('non-existent-sandbox-id');
  } catch (error) {
    if (error instanceof SandboxNotFoundError) {
      console.log('   SandboxNotFoundError caught');
      console.log(`   Error code: ${error.code}`);
    } else if (error instanceof WorkspaceError) {
      console.log(`   WorkspaceError [${error.code}]: ${error.message}`);
    } else {
      console.log(`   Unknown error: ${error}`);
    }
  }
  console.log();

  // 2. Create a sandbox for more tests
  console.log('2. Creating sandbox for error tests...');
  const sandbox = await client.sandbox.create({
    template: 'workspace-test:latest',
  });
  console.log(`   Created: ${sandbox.id}\n`);

  try {
    // 3. Handle command failure
    console.log('3. Handling command that returns non-zero exit code...');
    const result = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', 'exit 42'],
    });
    console.log(`   Exit code: ${result.exitCode} (expected: 42)`);
    if (result.exitCode === 42) {
      console.log('   Non-zero exit code correctly captured');
    }
    console.log();

    // 4. Handle command with stderr
    console.log('4. Handling command that writes to stderr...');
    const result2 = await client.process.run(sandbox.id, 'bash', {
      args: ['-c', "echo 'error message' >&2; exit 1"],
    });
    console.log(`   Exit code: ${result2.exitCode}`);
    console.log(`   Stderr: ${result2.stderr.trim()}`);
    console.log();

    // 5. Handle missing file
    console.log('5. Handling missing file...');
    const result3 = await client.process.run(sandbox.id, 'cat', {
      args: ['/nonexistent/file.txt'],
    });
    if (result3.exitCode !== 0) {
      console.log(`   Command failed with exit code: ${result3.exitCode}`);
      console.log(`   Stderr: ${result3.stderr.trim()}`);
    }
    console.log();

    // 6. Handle invalid command
    console.log('6. Handling invalid command...');
    const result4 = await client.process.run(sandbox.id, 'nonexistent_command_xyz', {
      args: [],
    });
    if (result4.exitCode !== 0) {
      console.log(`   Command failed with exit code: ${result4.exitCode}`);
    }
    console.log();

    // 7. Using a helper function pattern
    console.log('7. Best practice - using error handling helper...');

    async function safeRun(
      client: WorkspaceClient,
      sandboxId: string,
      cmd: string,
      opts: RunCommandOptions = {}
    ): Promise<CommandResult | null> {
      try {
        const result = await client.process.run(sandboxId, cmd, opts);
        if (result.exitCode !== 0) {
          console.log(`   Warning: Command '${cmd}' exited with code ${result.exitCode}`);
          if (result.stderr) {
            console.log(`   Stderr: ${result.stderr.trim()}`);
          }
          return null;
        }
        return result;
      } catch (error) {
        if (error instanceof WorkspaceError) {
          console.log(`   API Error: ${error.message}`);
        } else {
          console.log(`   Unknown error: ${error}`);
        }
        return null;
      }
    }

    // Test the helper
    const safeResult = await safeRun(client, sandbox.id, 'bash', {
      args: ['-c', 'exit 1'],
    });
    console.log(`   Result: ${safeResult}`);
    console.log();

    // 8. Checking sandbox exists before operations
    console.log('8. Checking sandbox exists before operations...');
    const exists = await client.sandbox.exists(sandbox.id);
    console.log(`   Sandbox exists: ${exists}`);

    const fakeExists = await client.sandbox.exists('fake-sandbox-id');
    console.log(`   Fake sandbox exists: ${fakeExists}`);
  } finally {
    // Cleanup
    console.log('\n9. Cleaning up...');
    await client.sandbox.delete(sandbox.id, true);
    console.log('   Done!');
  }
}

main().catch(console.error);
