package workspace

import (
	"fmt"
	"net/http"
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
