import { describe, it, expect } from 'vitest';
import { FileLockMap, Semaphore } from '../src/services/storage-provider-lock';

describe('FileLockMap', () => {
  it('serializes concurrent operations on same file', async () => {
    const locks = new FileLockMap(5000);
    const order: number[] = [];

    const op = async (id: number) => {
      const release = await locks.acquire('file.txt');
      order.push(id);
      await new Promise(r => setTimeout(r, 10));
      release();
    };

    await Promise.all([op(1), op(2), op(3)]);
    expect(order).toHaveLength(3);
  });

  it('allows concurrent operations on different files', async () => {
    const locks = new FileLockMap(5000);
    let maxConcurrent = 0;
    let current = 0;

    const op = async (file: string) => {
      const release = await locks.acquire(file);
      current++;
      maxConcurrent = Math.max(maxConcurrent, current);
      await new Promise(r => setTimeout(r, 20));
      current--;
      release();
    };

    await Promise.all([op('a.txt'), op('b.txt'), op('c.txt')]);
    expect(maxConcurrent).toBeGreaterThan(1);
  });

  it('times out when lock held too long', async () => {
    const locks = new FileLockMap(50);
    const release = await locks.acquire('busy.txt');

    await expect(locks.acquire('busy.txt')).rejects.toThrow('file lock timeout');
    release();
  });
});

describe('Semaphore', () => {
  it('limits concurrency', async () => {
    const sem = new Semaphore(2);
    let maxConcurrent = 0;
    let current = 0;

    const op = async () => {
      const release = await sem.acquire();
      current++;
      maxConcurrent = Math.max(maxConcurrent, current);
      await new Promise(r => setTimeout(r, 20));
      current--;
      release();
    };

    await Promise.all([op(), op(), op(), op()]);
    expect(maxConcurrent).toBeLessThanOrEqual(2);
  });

  it('releases correctly after errors', async () => {
    const sem = new Semaphore(1);

    const op = async (shouldThrow: boolean) => {
      const release = await sem.acquire();
      try {
        if (shouldThrow) throw new Error('test error');
      } finally {
        release();
      }
    };

    await expect(op(true)).rejects.toThrow('test error');
    // Semaphore should still be usable after error.
    const release = await sem.acquire();
    release();
  });
});
