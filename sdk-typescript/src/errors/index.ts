/**
 * Error types for the Workspace SDK
 */

import * as grpc from '@grpc/grpc-js';

// HTTP status codes used for error mapping (matching Go SDK)
const HTTP_STATUS = {
  BAD_REQUEST: 400,
  UNAUTHORIZED: 401,
  FORBIDDEN: 403,
  NOT_FOUND: 404,
  PRECONDITION_FAILED: 412,
  CLIENT_CLOSED: 499,
  INTERNAL_SERVER_ERROR: 500,
  SERVICE_UNAVAILABLE: 503,
} as const;

/**
 * Base error class for Workspace SDK errors
 */
export class WorkspaceError extends Error {
  /** HTTP-style status code (matching Go SDK) */
  public readonly statusCode: number;
  /** Additional details */
  public readonly details?: string;

  constructor(message: string, statusCode: number, details?: string) {
    super(message);
    this.name = 'WorkspaceError';
    this.statusCode = statusCode;
    this.details = details;
  }
}

/**
 * Workspace not found error
 */
export class WorkspaceNotFoundError extends WorkspaceError {
  constructor(workspaceId: string) {
    super(`Workspace not found: ${workspaceId}`, HTTP_STATUS.NOT_FOUND);
    this.name = 'WorkspaceNotFoundError';
  }
}

/**
 * Sandbox not found error
 */
export class SandboxNotFoundError extends WorkspaceError {
  public readonly sandboxId: string;

  constructor(sandboxId: string) {
    super(`Sandbox not found: ${sandboxId}`, HTTP_STATUS.NOT_FOUND);
    this.name = 'SandboxNotFoundError';
    this.sandboxId = sandboxId;
  }
}

/**
 * Template not found error
 */
export class TemplateNotFoundError extends WorkspaceError {
  constructor(template: string) {
    super(`Template not found: ${template}`, HTTP_STATUS.NOT_FOUND);
    this.name = 'TemplateNotFoundError';
  }
}

/**
 * File not found error
 */
export class FileNotFoundError extends WorkspaceError {
  constructor(path: string) {
    super(`File not found: ${path}`, HTTP_STATUS.NOT_FOUND);
    this.name = 'FileNotFoundError';
  }
}

/**
 * Permission denied error
 */
export class PermissionDeniedError extends WorkspaceError {
  constructor(message: string) {
    super(message, HTTP_STATUS.FORBIDDEN);
    this.name = 'PermissionDeniedError';
  }
}

/**
 * Process execution error
 */
export class ProcessError extends WorkspaceError {
  public readonly sandboxId: string;
  public readonly command: string;

  constructor(sandboxId: string, command: string, message: string) {
    super(`Process error in sandbox ${sandboxId} running '${command}': ${message}`, HTTP_STATUS.INTERNAL_SERVER_ERROR);
    this.name = 'ProcessError';
    this.sandboxId = sandboxId;
    this.command = command;
  }
}

/**
 * Timeout error
 */
export class TimeoutError extends WorkspaceError {
  public readonly operation: string;
  public readonly duration: string;

  constructor(operation: string, duration: string = 'unknown') {
    super(`Timeout after ${duration} during ${operation}`, HTTP_STATUS.INTERNAL_SERVER_ERROR);
    this.name = 'TimeoutError';
    this.operation = operation;
    this.duration = duration;
  }
}

/**
 * Connection error
 */
export class ConnectionError extends WorkspaceError {
  public readonly url: string;

  constructor(url: string, message: string) {
    super(`Connection error to ${url}: ${message}`, HTTP_STATUS.SERVICE_UNAVAILABLE);
    this.name = 'ConnectionError';
    this.url = url;
  }
}

/**
 * PTY not found error
 */
export class PtyNotFoundError extends WorkspaceError {
  constructor(ptyId: string) {
    super(`PTY not found: ${ptyId}`, HTTP_STATUS.NOT_FOUND);
    this.name = 'PtyNotFoundError';
  }
}

/**
 * Agent not connected error
 */
export class AgentNotConnectedError extends WorkspaceError {
  constructor(sandboxId: string) {
    super(`Agent not connected for sandbox: ${sandboxId}`, HTTP_STATUS.SERVICE_UNAVAILABLE);
    this.name = 'AgentNotConnectedError';
  }
}

/**
 * Check if an error is a not-found error
 */
export function isNotFound(err: unknown): boolean {
  if (err == null) return false;
  if (err instanceof WorkspaceError) {
    return err.statusCode === HTTP_STATUS.NOT_FOUND;
  }
  if (err instanceof SandboxNotFoundError) {
    return true;
  }
  return false;
}

/**
 * Check if an error is a timeout error
 */
export function isTimeout(err: unknown): boolean {
  if (err == null) return false;
  return err instanceof TimeoutError;
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
      return new WorkspaceError(message, HTTP_STATUS.NOT_FOUND);

    case grpc.status.INVALID_ARGUMENT:
      return new WorkspaceError(message, HTTP_STATUS.BAD_REQUEST);

    case grpc.status.UNAUTHENTICATED:
      return new WorkspaceError(message, HTTP_STATUS.UNAUTHORIZED);

    case grpc.status.PERMISSION_DENIED:
      return new PermissionDeniedError(message);

    case grpc.status.FAILED_PRECONDITION:
      return new WorkspaceError(message, HTTP_STATUS.PRECONDITION_FAILED);

    case grpc.status.UNAVAILABLE:
      if (message.toLowerCase().includes('agent')) {
        return new AgentNotConnectedError(message);
      }
      return new WorkspaceError(message, HTTP_STATUS.SERVICE_UNAVAILABLE);

    case grpc.status.DEADLINE_EXCEEDED:
      return new TimeoutError('gRPC call');

    case grpc.status.CANCELLED:
      return new WorkspaceError(message, HTTP_STATUS.CLIENT_CLOSED);

    default:
      return new WorkspaceError(message, HTTP_STATUS.INTERNAL_SERVER_ERROR);
  }
}
