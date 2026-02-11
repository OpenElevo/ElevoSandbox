package workspace

import (
	"fmt"
	"net/http"

	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
)

// Error represents an API error
type Error struct {
	StatusCode int    `json:"status_code"`
	Message    string `json:"message"`
	Details    string `json:"details,omitempty"`
}

func (e *Error) Error() string {
	if e.Details != "" {
		return fmt.Sprintf("workspace error [%d]: %s - %s", e.StatusCode, e.Message, e.Details)
	}
	return fmt.Sprintf("workspace error [%d]: %s", e.StatusCode, e.Message)
}

// Common error types
var (
	ErrNotFound          = &Error{StatusCode: http.StatusNotFound, Message: "resource not found"}
	ErrBadRequest        = &Error{StatusCode: http.StatusBadRequest, Message: "bad request"}
	ErrUnauthorized      = &Error{StatusCode: http.StatusUnauthorized, Message: "unauthorized"}
	ErrForbidden         = &Error{StatusCode: http.StatusForbidden, Message: "forbidden"}
	ErrInternalServer    = &Error{StatusCode: http.StatusInternalServerError, Message: "internal server error"}
	ErrServiceUnavailable = &Error{StatusCode: http.StatusServiceUnavailable, Message: "service unavailable"}
)

// SandboxNotFoundError represents a sandbox not found error
type SandboxNotFoundError struct {
	SandboxID string
}

func (e *SandboxNotFoundError) Error() string {
	return fmt.Sprintf("sandbox not found: %s", e.SandboxID)
}

// ProcessError represents a process execution error
type ProcessError struct {
	SandboxID string
	Command   string
	Message   string
}

func (e *ProcessError) Error() string {
	return fmt.Sprintf("process error in sandbox %s running '%s': %s", e.SandboxID, e.Command, e.Message)
}

// ConnectionError represents a connection error
type ConnectionError struct {
	URL     string
	Message string
}

func (e *ConnectionError) Error() string {
	return fmt.Sprintf("connection error to %s: %s", e.URL, e.Message)
}

// TimeoutError represents a timeout error
type TimeoutError struct {
	Operation string
	Duration  string
}

func (e *TimeoutError) Error() string {
	return fmt.Sprintf("timeout after %s during %s", e.Duration, e.Operation)
}

// IsNotFound checks if an error is a not found error
func IsNotFound(err error) bool {
	if err == nil {
		return false
	}
	if e, ok := err.(*Error); ok {
		return e.StatusCode == http.StatusNotFound
	}
	if _, ok := err.(*SandboxNotFoundError); ok {
		return true
	}
	return false
}

// IsTimeout checks if an error is a timeout error
func IsTimeout(err error) bool {
	if err == nil {
		return false
	}
	_, ok := err.(*TimeoutError)
	return ok
}

// convertGrpcError converts a gRPC error to a workspace error
func convertGrpcError(err error) error {
	if err == nil {
		return nil
	}

	st, ok := status.FromError(err)
	if !ok {
		return &Error{
			StatusCode: http.StatusInternalServerError,
			Message:    err.Error(),
		}
	}

	var statusCode int
	switch st.Code() {
	case codes.OK:
		return nil
	case codes.NotFound:
		statusCode = http.StatusNotFound
	case codes.InvalidArgument:
		statusCode = http.StatusBadRequest
	case codes.Unauthenticated:
		statusCode = http.StatusUnauthorized
	case codes.PermissionDenied:
		statusCode = http.StatusForbidden
	case codes.FailedPrecondition:
		statusCode = http.StatusPreconditionFailed
	case codes.Unavailable:
		statusCode = http.StatusServiceUnavailable
	case codes.DeadlineExceeded:
		return &TimeoutError{
			Operation: "gRPC call",
			Duration:  "unknown",
		}
	case codes.Canceled:
		return &Error{
			StatusCode: 499, // Client Closed Request
			Message:    st.Message(),
		}
	default:
		statusCode = http.StatusInternalServerError
	}

	return &Error{
		StatusCode: statusCode,
		Message:    st.Message(),
	}
}
