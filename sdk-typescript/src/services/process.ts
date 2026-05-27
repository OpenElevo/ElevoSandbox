/**
 * Process service for executing commands via gRPC
 */

import * as grpc from '@grpc/grpc-js';
import { CommandResult, RunCommandOptions, ProcessEvent } from '../types/index.js';
import { ProcessServiceClient, createMetadata, promisifyUnary } from '../grpc.js';
import { convertGrpcError, ProcessError } from '../errors/index.js';

/**
 * Service for executing commands in sandboxes
 */
export class ProcessService {
  constructor(
    private readonly client: ProcessServiceClient,
    private readonly apiKey?: string
  ) {}

  private metadata(): grpc.Metadata {
    return createMetadata(this.apiKey);
  }

  /**
   * Run a command and wait for completion
   */
  async run(sandboxId: string, command: string, options: RunCommandOptions = {}): Promise<CommandResult> {
    try {
      const request: any = {
        sandboxId,
        command,
        args: options.args || [],
        env: options.env || {},
      };
      if (options.cwd) request.cwd = options.cwd;
      if (options.timeout) request.timeoutMs = options.timeout;

      const response = await promisifyUnary(
        this.client,
        this.client.runCommand,
        request,
        this.metadata()
      );

      return {
        exitCode: response.result.exitCode,
        stdout: response.result.stdout,
        stderr: response.result.stderr,
      };
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Run a command with streaming output
   */
  runStream(
    sandboxId: string,
    command: string,
    options: RunCommandOptions = {}
  ): AsyncIterable<ProcessEvent> {
    const client = this.client;
    const metadata = this.metadata();

    const request: any = {
      sandboxId,
      command,
      args: options.args || [],
      env: options.env || {},
    };
    if (options.cwd) request.cwd = options.cwd;
    if (options.timeout) request.timeoutMs = options.timeout;

    return {
      [Symbol.asyncIterator]: () => {
        const stream = client.runCommandStream(request, metadata);
        let done = false;
        let error: Error | null = null;

        stream.on('error', (err: grpc.ServiceError) => {
          error = convertGrpcError(err);
          done = true;
        });

        stream.on('end', () => {
          done = true;
        });

        return {
          next: (): Promise<IteratorResult<ProcessEvent>> => {
            return new Promise((resolve, reject) => {
              if (error) {
                reject(error);
                return;
              }

              if (done) {
                resolve({ value: undefined as any, done: true });
                return;
              }

              const onData = (data: any) => {
                stream.removeListener('data', onData);
                stream.removeListener('end', onEnd);
                stream.removeListener('error', onError);

                const event = this.parseEvent(data);
                if (event) {
                  resolve({ value: event, done: false });
                } else {
                  // Skip unknown events, get next
                  this.getNextEvent(stream, resolve, reject);
                }
              };

              const onEnd = () => {
                stream.removeListener('data', onData);
                stream.removeListener('end', onEnd);
                stream.removeListener('error', onError);
                done = true;
                resolve({ value: undefined as any, done: true });
              };

              const onError = (err: grpc.ServiceError) => {
                stream.removeListener('data', onData);
                stream.removeListener('end', onEnd);
                stream.removeListener('error', onError);
                reject(convertGrpcError(err));
              };

              stream.once('data', onData);
              stream.once('end', onEnd);
              stream.once('error', onError);
            });
          },
        };
      },
    };
  }

  private getNextEvent(
    stream: grpc.ClientReadableStream<any>,
    resolve: (value: IteratorResult<ProcessEvent>) => void,
    reject: (error: Error) => void
  ): void {
    const onData = (data: any) => {
      stream.removeListener('data', onData);
      stream.removeListener('end', onEnd);
      stream.removeListener('error', onError);

      const event = this.parseEvent(data);
      if (event) {
        resolve({ value: event, done: false });
      } else {
        this.getNextEvent(stream, resolve, reject);
      }
    };

    const onEnd = () => {
      stream.removeListener('data', onData);
      stream.removeListener('end', onEnd);
      stream.removeListener('error', onError);
      resolve({ value: undefined as any, done: true });
    };

    const onError = (err: grpc.ServiceError) => {
      stream.removeListener('data', onData);
      stream.removeListener('end', onEnd);
      stream.removeListener('error', onError);
      reject(convertGrpcError(err));
    };

    stream.once('data', onData);
    stream.once('end', onEnd);
    stream.once('error', onError);
  }

  /**
   * Kill a running process
   */
  async kill(sandboxId: string, pid: number, signal?: number): Promise<void> {
    try {
      const request: any = {
        sandboxId,
        pid,
      };
      if (signal !== undefined) request.signal = signal;

      await promisifyUnary(
        this.client,
        this.client.killProcess,
        request,
        this.metadata()
      );
    } catch (error) {
      throw convertGrpcError(error as grpc.ServiceError);
    }
  }

  /**
   * Convenience method: run a command and return stdout.
   * Throws ProcessError if the exit code is non-zero.
   */
  async exec(sandboxId: string, command: string, ...args: string[]): Promise<string> {
    const result = await this.run(sandboxId, command, { args });

    if (result.exitCode !== 0) {
      throw new ProcessError(
        sandboxId,
        command,
        `exit code ${result.exitCode}: ${result.stderr}`
      );
    }

    return result.stdout;
  }

  /**
   * Convenience method: run a shell command using bash -c
   */
  async shell(
    sandboxId: string,
    script: string,
    env?: Record<string, string>
  ): Promise<CommandResult> {
    return this.run(sandboxId, 'bash', {
      args: ['-c', script],
      env,
    });
  }

  private parseEvent(data: any): ProcessEvent | null {
    // Handle oneof field - proto-loader uses the field name directly
    if (data.stdout) {
      return { type: 'stdout', data: data.stdout.data };
    } else if (data.stderr) {
      return { type: 'stderr', data: data.stderr.data };
    } else if (data.exit) {
      return { type: 'exit', code: data.exit.code };
    } else if (data.error) {
      return { type: 'error', message: data.error.message };
    }
    return null;
  }
}
