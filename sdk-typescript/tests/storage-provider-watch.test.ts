import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { FileWatcher, FileChangeEvent } from '../src/services/storage-provider-watch';

describe('FileWatcher', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'filewatcher-test-'));
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('detects file creation', async () => {
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'new.txt'), 'hello');

    await new Promise(r => setTimeout(r, 400));
    await watcher.close();

    expect(events.length).toBeGreaterThan(0);
    const allEvents = events.flat();
    const created = allEvents.find(e => e.path === 'new.txt');
    expect(created).toBeDefined();
  });

  it('detects file deletion', async () => {
    fs.writeFileSync(path.join(tmpDir, 'del.txt'), 'hello');

    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.unlinkSync(path.join(tmpDir, 'del.txt'));

    await new Promise(r => setTimeout(r, 400));
    await watcher.close();

    const allEvents = events.flat();
    const deleted = allEvents.find(
      e => e.path === 'del.txt' && e.eventType === 'FILE_CHANGE_TYPE_DELETED',
    );
    expect(deleted).toBeDefined();
  });

  it('coalesces rapid events on same file', async () => {
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    // Rapid writes to the same file.
    for (let i = 0; i < 5; i++) {
      fs.writeFileSync(path.join(tmpDir, 'rapid.txt'), `v${i}`);
    }

    await new Promise(r => setTimeout(r, 400));
    await watcher.close();

    const allEvents = events.flat();
    const rapidEvents = allEvents.filter(e => e.path === 'rapid.txt');
    // Coalesced: fewer than 5 events.
    expect(rapidEvents.length).toBeLessThan(5);
  });

  it('respects default ignore dirs', async () => {
    fs.mkdirSync(path.join(tmpDir, 'node_modules'), { recursive: true });
    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'node_modules', 'pkg.json'), '{}');
    await new Promise(r => setTimeout(r, 400));
    await watcher.close();

    const allEvents = events.flat();
    const nodeModuleEvents = allEvents.filter(e => e.path.startsWith('node_modules'));
    expect(nodeModuleEvents).toHaveLength(0);
  });

  it('respects .elevoignore rules', async () => {
    fs.writeFileSync(path.join(tmpDir, '.elevoignore'), '*.log\n');

    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'app.log'), 'log data');
    fs.writeFileSync(path.join(tmpDir, 'app.ts'), 'code');
    await new Promise(r => setTimeout(r, 400));
    await watcher.close();

    const allEvents = events.flat();
    const logEvents = allEvents.filter(e => e.path.endsWith('.log'));
    expect(logEvents).toHaveLength(0);
    const tsEvents = allEvents.filter(e => e.path === 'app.ts');
    expect(tsEvents.length).toBeGreaterThan(0);
  });

  it('detects nested file changes', async () => {
    fs.mkdirSync(path.join(tmpDir, 'src'));

    const events: FileChangeEvent[][] = [];
    const watcher = new FileWatcher(tmpDir, (batch) => events.push(batch));
    await watcher.start();

    fs.writeFileSync(path.join(tmpDir, 'src', 'main.ts'), 'code');
    await new Promise(r => setTimeout(r, 400));
    await watcher.close();

    const allEvents = events.flat();
    const found = allEvents.find(e => e.path === path.join('src', 'main.ts'));
    expect(found).toBeDefined();
  });
});
