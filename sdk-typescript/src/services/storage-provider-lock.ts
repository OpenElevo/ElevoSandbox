/**
 * Per-file lock using Promise chains.
 * Ensures only one write operation runs on a given file at a time.
 */
export class FileLockMap {
  private locks = new Map<string, Promise<void>>();
  private timeoutMs: number;

  constructor(timeoutMs: number = 10000) {
    this.timeoutMs = timeoutMs;
  }

  /**
   * Acquire a lock for the given file path.
   * Returns a release function. Throws if timeout is exceeded.
   */
  async acquire(filePath: string): Promise<() => void> {
    let release!: () => void;
    const newLock = new Promise<void>(resolve => { release = resolve; });

    const prev = this.locks.get(filePath) ?? Promise.resolve();
    this.locks.set(filePath, newLock);

    const timeout = new Promise<never>((_, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`file lock timeout: ${filePath}`)),
        this.timeoutMs,
      );
      // Don't block Node.js exit.
      if (typeof timer === 'object' && 'unref' in timer) timer.unref();
    });

    await Promise.race([prev, timeout]);
    return release;
  }

  /** Remove a lock entry (for cleanup after file deletion). */
  delete(filePath: string): void {
    this.locks.delete(filePath);
  }
}

/**
 * Counting semaphore for limiting concurrency.
 */
export class Semaphore {
  private current = 0;
  private queue: Array<() => void> = [];

  constructor(private readonly max: number) {}

  async acquire(): Promise<() => void> {
    if (this.current < this.max) {
      this.current++;
      return () => this.release();
    }

    return new Promise<() => void>(resolve => {
      this.queue.push(() => {
        this.current++;
        resolve(() => this.release());
      });
    });
  }

  private release(): void {
    this.current--;
    const next = this.queue.shift();
    if (next) next();
  }
}
