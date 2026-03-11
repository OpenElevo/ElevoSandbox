import * as chokidar from 'chokidar';
import * as fs from 'fs';
import * as path from 'path';

export interface FileChangeEvent {
  /** Workspace-relative path */
  path: string;
  /** Event type matching proto FileChangeType */
  eventType: string;
}

const DEFAULT_IGNORE_DIRS = new Set([
  '.git',
  'node_modules',
  '__pycache__',
  'target',
  'build',
  '.elevo',
]);

/** Coalescing window before flush (ms) */
const COALESCE_DELAY_MS = 50;
/** Max latency cap before forced flush (ms) */
const MAX_LATENCY_MS = 200;
/** Degraded mode poll interval (ms) */
const DEGRADED_POLL_INTERVAL_MS = 5000;

/**
 * FileWatcher monitors a local directory for changes and emits batched
 * FileChangeEvent arrays via the onFlush callback.
 *
 * Features:
 * - Event coalescing: 50ms window + 200ms max-latency cap
 * - Per-path dedup (last event wins within a window)
 * - Default ignore dirs (.git, node_modules, etc.)
 * - .elevoignore support (glob patterns, one per line)
 * - Degraded mode: 5s full_purge polling on watcher errors
 */
export class FileWatcher {
  private watcher: chokidar.FSWatcher | null = null;
  private pending = new Map<string, FileChangeEvent>();
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private maxLatencyTimer: ReturnType<typeof setTimeout> | null = null;
  private ignoreRules: string[] = [];
  private closed = false;

  // Degraded mode: when the watcher hits system limits or errors,
  // fall back to sending full_purge every 5 seconds.
  private degraded = false;
  private degradedTimer: ReturnType<typeof setInterval> | null = null;

  constructor(
    private readonly rootDir: string,
    private readonly onFlush: (events: FileChangeEvent[]) => void,
  ) {
    this.ignoreRules = loadElevoIgnore(rootDir);
  }

  /**
   * Start watching. Resolves when the watcher is ready.
   */
  async start(): Promise<void> {
    // Check if directory tree would exceed inotify limits.
    if (shouldDegrade(this.rootDir)) {
      this.enterDegradedMode();
      return;
    }

    const ignoredPaths = buildIgnoredPatterns(this.rootDir);

    this.watcher = chokidar.watch(this.rootDir, {
      ignored: ignoredPaths,
      ignoreInitial: true,
      persistent: true,
      followSymlinks: false,
    });

    this.watcher
      .on('add', (p) => this.handleEvent(p, 'FILE_CHANGE_TYPE_CREATED'))
      .on('addDir', (p) => this.handleEvent(p, 'FILE_CHANGE_TYPE_CREATED'))
      .on('change', (p) => this.handleEvent(p, 'FILE_CHANGE_TYPE_MODIFIED'))
      .on('unlink', (p) => this.handleEvent(p, 'FILE_CHANGE_TYPE_DELETED'))
      .on('unlinkDir', (p) => this.handleEvent(p, 'FILE_CHANGE_TYPE_DELETED'))
      .on('error', (err: unknown) => {
        const error = err instanceof Error ? err : new Error(String(err));
        this.handleWatcherError(error);
      });

    return new Promise<void>((resolve) => {
      this.watcher!.on('ready', () => resolve());
    });
  }

  /**
   * Close the watcher and flush any pending events.
   */
  async close(): Promise<void> {
    this.closed = true;
    this.stopDegradedMode();
    this.flush();
    if (this.watcher) {
      await this.watcher.close();
      this.watcher = null;
    }
  }

  /**
   * Send a full_purge event (empty array signals full purge to caller).
   */
  sendFullPurge(): void {
    this.onFlush([]);
  }

  /** Whether the watcher is in degraded (polling) mode. */
  isDegraded(): boolean {
    return this.degraded;
  }

  private handleEvent(absPath: string, eventType: string): void {
    if (this.closed || this.degraded) return;

    const relPath = path.relative(this.rootDir, absPath);
    if (!relPath || relPath.startsWith('..')) return;

    // Check .elevoignore rules.
    if (this.matchesIgnoreRules(relPath)) return;

    // Per-path dedup: last event wins.
    this.pending.set(relPath, { path: relPath, eventType });

    // Start coalescing timer if not running.
    if (!this.flushTimer) {
      this.flushTimer = setTimeout(() => this.flush(), COALESCE_DELAY_MS);
    }

    // Start max-latency timer if not running.
    if (!this.maxLatencyTimer) {
      this.maxLatencyTimer = setTimeout(() => this.flush(), MAX_LATENCY_MS);
    }
  }

  /**
   * Handle watcher errors. If the error is related to inotify limits,
   * switch to degraded mode.
   */
  private handleWatcherError(err: Error): void {
    if (this.closed) return;

    const msg = err.message || '';
    const isLimitError = msg.includes('no space left on device') ||
                         msg.includes('too many open files') ||
                         msg.includes('ENOSPC');

    if (isLimitError && !this.degraded) {
      console.error(`[FileWatcher] inotify limit reached, switching to degraded mode: ${msg}`);
      this.enterDegradedMode();
    } else {
      console.error(`[FileWatcher] error: ${msg}`);
    }
  }

  /**
   * Enter degraded mode: stop normal watching, start polling with full_purge.
   */
  private enterDegradedMode(): void {
    this.degraded = true;

    // Clear any pending normal events.
    this.pending.clear();
    if (this.flushTimer) { clearTimeout(this.flushTimer); this.flushTimer = null; }
    if (this.maxLatencyTimer) { clearTimeout(this.maxLatencyTimer); this.maxLatencyTimer = null; }

    // Send an immediate full_purge.
    this.sendFullPurge();

    // Start periodic full_purge polling.
    this.degradedTimer = setInterval(() => {
      if (!this.closed) {
        this.sendFullPurge();
      }
    }, DEGRADED_POLL_INTERVAL_MS);

    // Don't block Node.js exit.
    if (typeof this.degradedTimer === 'object' && 'unref' in this.degradedTimer) {
      this.degradedTimer.unref();
    }
  }

  private stopDegradedMode(): void {
    if (this.degradedTimer) {
      clearInterval(this.degradedTimer);
      this.degradedTimer = null;
    }
  }

  private flush(): void {
    if (this.flushTimer) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }
    if (this.maxLatencyTimer) {
      clearTimeout(this.maxLatencyTimer);
      this.maxLatencyTimer = null;
    }

    if (this.pending.size === 0) return;

    const events = Array.from(this.pending.values());
    this.pending.clear();
    this.onFlush(events);
  }

  private matchesIgnoreRules(relPath: string): boolean {
    const fileName = path.basename(relPath);
    for (const rule of this.ignoreRules) {
      if (matchGlob(rule, fileName) || matchGlob(rule, relPath)) {
        return true;
      }
    }
    return false;
  }
}

/**
 * Build chokidar ignored patterns from default dirs.
 */
function buildIgnoredPatterns(rootDir: string): Array<string | RegExp> {
  const patterns: Array<string | RegExp> = [];
  for (const dir of DEFAULT_IGNORE_DIRS) {
    patterns.push(path.join(rootDir, dir));
    // Also match nested occurrences (e.g. sub/node_modules).
    patterns.push(new RegExp(`(^|[\\/\\\\])${escapeRegExp(dir)}[\\/\\\\]`));
  }
  return patterns;
}

function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Load .elevoignore rules from the root directory.
 */
function loadElevoIgnore(rootDir: string): string[] {
  const ignorePath = path.join(rootDir, '.elevoignore');
  try {
    const content = fs.readFileSync(ignorePath, 'utf-8');
    return content
      .split('\n')
      .map(line => line.trim())
      .filter(line => line && !line.startsWith('#'));
  } catch {
    return [];
  }
}

/**
 * Simple glob matching (supports * and ? wildcards).
 */
function matchGlob(pattern: string, str: string): boolean {
  const regexStr = '^' + pattern
    .replace(/[.+^${}()|[\]\\]/g, '\\$&')
    .replace(/\*/g, '.*')
    .replace(/\?/g, '.') + '$';
  return new RegExp(regexStr).test(str);
}

/**
 * Check if the directory tree would exceed 80% of the inotify watch limit.
 */
function shouldDegrade(rootDir: string): boolean {
  // Only relevant on Linux.
  try {
    const data = fs.readFileSync('/proc/sys/fs/inotify/max_user_watches', 'utf-8');
    const maxWatches = parseInt(data.trim(), 10);
    if (isNaN(maxWatches) || maxWatches <= 0) return false;

    const dirCount = countDirectories(rootDir);
    return dirCount > (maxWatches * 80 / 100);
  } catch {
    // Not Linux or can't read — proceed optimistically.
    return false;
  }
}

/**
 * Count directories under root, respecting default ignore list.
 */
function countDirectories(root: string): number {
  let count = 0;
  const walk = (dir: string) => {
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isDirectory()) continue;
        if (DEFAULT_IGNORE_DIRS.has(entry.name)) continue;
        count++;
        walk(path.join(dir, entry.name));
      }
    } catch {
      // Skip inaccessible directories.
    }
  };
  count++; // Count root itself.
  walk(root);
  return count;
}
