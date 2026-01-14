/**
 * Example: Streaming Command Output
 *
 * This example demonstrates streaming command output using SSE.
 *
 * Run: npx ts-node streaming.ts
 */

import { WorkspaceClient } from '../src';

async function main() {
  console.log('=== Streaming Output Example ===\n');

  const client = new WorkspaceClient({
    apiUrl: 'http://localhost:8080',
    timeout: 120000,
  });

  // Create sandbox
  console.log('1. Creating sandbox...');
  const sandbox = await client.sandbox.create({
    template: 'workspace-test:latest',
  });
  console.log(`   Created: ${sandbox.id}\n`);

  try {
    // 2. Stream a simple loop
    console.log('2. Streaming loop output...');
    process.stdout.write('   Output: ');

    for await (const event of client.process.runStream(sandbox.id, 'bash', {
      args: [
        '-c',
        `
        for i in 1 2 3 4 5; do
          echo "Line $i"
          sleep 0.5
        done
      `,
      ],
    })) {
      switch (event.type) {
        case 'stdout':
          process.stdout.write(event.data);
          break;
        case 'stderr':
          process.stderr.write(event.data);
          break;
        case 'exit':
          console.log(`\n   Exited with code: ${event.code}\n`);
          break;
        case 'error':
          console.log(`\n   Error: ${event.message}\n`);
          break;
      }
    }

    // 3. Stream with interleaved stdout/stderr
    console.log('3. Streaming with stdout and stderr...');
    console.log('   Output:');

    for await (const event of client.process.runStream(sandbox.id, 'bash', {
      args: [
        '-c',
        `
        echo "This goes to stdout"
        echo "This goes to stderr" >&2
        echo "Back to stdout"
        echo "More stderr" >&2
      `,
      ],
    })) {
      switch (event.type) {
        case 'stdout':
          console.log(`   [stdout] ${event.data.trim()}`);
          break;
        case 'stderr':
          console.log(`   [stderr] ${event.data.trim()}`);
          break;
        case 'exit':
          console.log(`   Exit code: ${event.code}\n`);
          break;
      }
    }

    // 4. Stream a longer running command
    console.log('4. Streaming progress updates...');
    process.stdout.write('   ');

    for await (const event of client.process.runStream(sandbox.id, 'bash', {
      args: [
        '-c',
        `
        for i in $(seq 1 10); do
          echo -n "."
          sleep 0.2
        done
        echo " Done!"
      `,
      ],
    })) {
      switch (event.type) {
        case 'stdout':
          process.stdout.write(event.data);
          break;
        case 'exit':
          console.log(`   (exit: ${event.code})`);
          break;
      }
    }
  } finally {
    // Cleanup
    console.log('\n5. Cleaning up...');
    await client.sandbox.delete(sandbox.id, true);
    console.log('   Done!');
  }
}

main().catch(console.error);
