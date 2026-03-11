/**
 * FileSystem service for low-level filesystem operations via gRPC
 * Primarily used for FUSE mounting and binary downloads.
 */

import * as grpc from '@grpc/grpc-js';
import { FileSystemServiceClient, createMetadata } from '../grpc';
import { convertGrpcError } from '../errors';

/**
 * Service for low-level filesystem operations (FUSE support)
 */
export class FileSystemService {
  constructor(
    private readonly client: FileSystemServiceClient,
    private readonly apiKey?: string
  ) {}

  private metadata(): grpc.Metadata {
    return createMetadata(this.apiKey);
  }

  /**
   * Download a binary file from the server (e.g., workspace-fuse)
   */
  async downloadBinary(name: string, platform: string, arch: string): Promise<Uint8Array> {
    const request = { name, platform, arch };

    return new Promise((resolve, reject) => {
      const stream = this.client.downloadBinary(request, this.metadata());
      const chunks: Buffer[] = [];

      stream.on('data', (response: any) => {
        if (response.chunk) {
          chunks.push(Buffer.from(response.chunk));
        }
      });

      stream.on('end', () => {
        resolve(new Uint8Array(Buffer.concat(chunks)));
      });

      stream.on('error', (err: grpc.ServiceError) => {
        reject(convertGrpcError(err));
      });
    });
  }
}
