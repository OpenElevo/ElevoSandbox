/**
 * Error types for the Workspace SDK
 */

import { AxiosError } from 'axios';
import * as grpc from '@grpc/grpc-js';

/**
 * Base error class for Workspace SDK errors
 */
export class WorkspaceError extends Error {
  /** Error code */
  public readonly code: number;
  /** Additional details */
  public readonly details?: string;

  constructor(message: string, code: number, details?: string) {
    super(message);
    this.name = 'WorkspaceError';
    this.code = code;
    this.details = details;
  }
}

/**
 * Workspace not found error
 */
export class WorkspaceNotFoundError extends WorkspaceError {
  constructor(workspaceId: string) {
    super(`Workspace not found: ${workspaceId}`, 1001);
    this.name = 'WorkspaceNotFoundError';
  }
}

/**
 * Sandbox not found error
 */
export class SandboxNotFoundError extends WorkspaceError {
  constructor(sandboxId: string) {
    super(`Sandbox not found: ${sandboxId}`, 2001);
    this.name = 'SandboxNotFoundError';
  }
}

/**
 * Template not found error
 */
export class TemplateNotFoundError extends WorkspaceError {
  constructor(template: string) {
    super(`Template not found: ${template}`, 2003);
    this.name = 'TemplateNotFoundError';
  }
}

/**
 * File not found error
 */
export class FileNotFoundError extends WorkspaceError {
  constructor(path: string) {
    super(`File not found: ${path}`, 3001);
    this.name = 'FileNotFoundError';
  }
}

/**
 * Permission denied error
 */
export class PermissionDeniedError extends WorkspaceError {
  constructor(path: string) {
    super(`Permission denied: ${path}`, 3003);
    this.name = 'PermissionDeniedError';
  }
}

/**
 * Process timeout error
 */
export class ProcessTimeoutError extends WorkspaceError {
  constructor() {
    super('Process timeout', 4002);
    this.name = 'ProcessTimeoutError';
  }
}

/**
 * PTY not found error
 */
export class PtyNotFoundError extends WorkspaceError {
  constructor(ptyId: string) {
    super(`PTY not found: ${ptyId}`, 4101);
    this.name = 'PtyNotFoundError';
  }
}

/**
 * Agent not connected error
 */
export class AgentNotConnectedError extends WorkspaceError {
  constructor(sandboxId: string) {
    super(`Agent not connected for sandbox: ${sandboxId}`, 5001);
    this.name = 'AgentNotConnectedError';
  }
}

/**
 * Convert gRPC error to SDK error
 */
export function convertGrpcError(error: grpc.ServiceError): WorkspaceError {
  const message = error.details || error.message || 'Unknown error';

  switch (error.code) {
    case grpc.status.NOT_FOUND:
      // Try to determine specific not found type from message
      if (message.toLowerCase().includes('workspace')) {
        return new WorkspaceNotFoundError(message);
      } else if (message.toLowerCase().includes('sandbox')) {
        return new SandboxNotFoundError(message);
      } else if (message.toLowerCase().includes('template')) {
        return new TemplateNotFoundError(message);
      } else if (message.toLowerCase().includes('pty')) {
        return new PtyNotFoundError(message);
      } else if (message.toLowerCase().includes('file')) {
        return new FileNotFoundError(message);
      }
      return new WorkspaceError(message, 1000);

    case grpc.status.PERMISSION_DENIED:
      return new PermissionDeniedError(message);

    case grpc.status.DEADLINE_EXCEEDED:
      return new ProcessTimeoutError();

    case grpc.status.UNAVAILABLE:
      if (message.toLowerCase().includes('agent')) {
        return new AgentNotConnectedError(message);
      }
      return new WorkspaceError(message, 5000);

    case grpc.status.INVALID_ARGUMENT:
      return new WorkspaceError(message, 1001);

    case grpc.status.FAILED_PRECONDITION:
      return new WorkspaceError(message, 1002);

    default:
      return new WorkspaceError(message, 1000);
  }
}

/**
 * Parse error response from HTTP API (legacy)
 */
export function parseErrorResponse(error: AxiosError): WorkspaceError {
  if (error.response?.data) {
    const data = error.response.data as { code?: number; message?: string; details?: string };
    const code = data.code || 1000;
    const message = data.message || 'Unknown error';

    // Map error codes to specific error types
    switch (code) {
      case 1001:
        return new WorkspaceNotFoundError(message.replace('Workspace not found: ', ''));
      case 2001:
        return new SandboxNotFoundError(message.replace('Sandbox not found: ', ''));
      case 2003:
        return new TemplateNotFoundError(message.replace('Template not found: ', ''));
      case 3001:
        return new FileNotFoundError(message.replace('File not found: ', ''));
      case 3003:
        return new PermissionDeniedError(message.replace('Permission denied: ', ''));
      case 4002:
        return new ProcessTimeoutError();
      case 4101:
        return new PtyNotFoundError(message.replace('PTY not found: ', ''));
      case 5001:
        return new AgentNotConnectedError(message.replace('Agent not connected for sandbox: ', ''));
      default:
        return new WorkspaceError(message, code, data.details);
    }
  }

  // Network or other error
  return new WorkspaceError(error.message || 'Network error', 1000);
}
