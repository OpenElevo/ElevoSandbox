/**
 * Process service for executing commands
 */

import { AxiosInstance } from 'axios';
import EventSource from 'eventsource';
import { CommandResult, RunCommandOptions, ProcessEvent } from '../types';

/**
 * Service for executing commands in sandboxes
 */
export class ProcessService {
  constructor(
    private readonly httpClient: AxiosInstance,
    private readonly apiUrl: string
  ) {}

  /**
   * Run a command and wait for completion
   */
  async run(sandboxId: string, command: string, options: RunCommandOptions = {}): Promise<CommandResult> {
    const response = await this.httpClient.post(`/sandboxes/${sandboxId}/process/run`, {
      command,
      args: options.args || [],
      env: options.env || {},
      cwd: options.cwd,
      timeout: options.timeout,
    });

    return {
      exitCode: response.data.exit_code,
      stdout: response.data.stdout,
      stderr: response.data.stderr,
    };
  }

  /**
   * Run a command with streaming output
   */
  runStream(
    sandboxId: string,
    command: string,
    options: RunCommandOptions = {}
  ): AsyncIterable<ProcessEvent> {
    const url = `${this.apiUrl}/api/v1/sandboxes/${sandboxId}/process/run/stream`;
    const params = new URLSearchParams({
      command,
      args: JSON.stringify(options.args || []),
      env: JSON.stringify(options.env || {}),
      ...(options.cwd && { cwd: options.cwd }),
      ...(options.timeout && { timeout: options.timeout.toString() }),
    });

    return {
      [Symbol.asyncIterator]: () => {
        const eventSource = new EventSource(`${url}?${params}`);
        let done = false;
        const events: ProcessEvent[] = [];
        let resolveNext: ((value: IteratorResult<ProcessEvent>) => void) | null = null;

        eventSource.onmessage = (event) => {
          const data = JSON.parse(event.data);
          const processEvent = this.parseEvent(data);
          if (processEvent) {
            if (resolveNext) {
              resolveNext({ value: processEvent, done: false });
              resolveNext = null;
            } else {
              events.push(processEvent);
            }
          }
        };

        eventSource.onerror = () => {
          done = true;
          eventSource.close();
          if (resolveNext) {
            resolveNext({ value: undefined as any, done: true });
            resolveNext = null;
          }
        };

        eventSource.addEventListener('exit', (event: any) => {
          done = true;
          eventSource.close();
        });

        return {
          next: () => {
            return new Promise<IteratorResult<ProcessEvent>>((resolve) => {
              if (events.length > 0) {
                resolve({ value: events.shift()!, done: false });
              } else if (done) {
                resolve({ value: undefined as any, done: true });
              } else {
                resolveNext = resolve;
              }
            });
          },
        };
      },
    };
  }

  /**
   * Kill a running process
   */
  async kill(sandboxId: string, pid: number, signal?: number): Promise<void> {
    await this.httpClient.post(`/sandboxes/${sandboxId}/process/${pid}/kill`, {
      signal: signal || 15,
    });
  }

  private parseEvent(data: any): ProcessEvent | null {
    if (data.type === 'stdout') {
      return { type: 'stdout', data: data.data };
    } else if (data.type === 'stderr') {
      return { type: 'stderr', data: data.data };
    } else if (data.type === 'exit') {
      return { type: 'exit', code: data.code };
    } else if (data.type === 'error') {
      return { type: 'error', message: data.message };
    }
    return null;
  }
}
