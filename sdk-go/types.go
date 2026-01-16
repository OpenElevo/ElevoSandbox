package workspace

import (
	"time"
)

// Workspace represents a workspace instance
type Workspace struct {
	ID        string            `json:"id"`
	Name      *string           `json:"name,omitempty"`
	NfsURL    *string           `json:"nfs_url,omitempty"`
	Metadata  map[string]string `json:"metadata,omitempty"`
	CreatedAt time.Time         `json:"created_at"`
	UpdatedAt time.Time         `json:"updated_at"`
}

// CreateWorkspaceParams contains parameters for creating a workspace
type CreateWorkspaceParams struct {
	Name     string            `json:"name,omitempty"`
	Metadata map[string]string `json:"metadata,omitempty"`
}

// ListWorkspacesResponse represents the response from listing workspaces
type ListWorkspacesResponse struct {
	Workspaces []Workspace `json:"workspaces"`
	Total      int         `json:"total"`
}

// SandboxState represents the state of a sandbox
type SandboxState string

const (
	SandboxStateStarting SandboxState = "starting"
	SandboxStateRunning  SandboxState = "running"
	SandboxStateStopped  SandboxState = "stopped"
	SandboxStateFailed   SandboxState = "failed"
)

// Sandbox represents a sandbox instance
type Sandbox struct {
	ID           string            `json:"id"`
	WorkspaceID  string            `json:"workspace_id"`
	Name         *string           `json:"name,omitempty"`
	Template     string            `json:"template"`
	State        SandboxState      `json:"state"`
	Env          map[string]string `json:"env,omitempty"`
	Metadata     map[string]string `json:"metadata,omitempty"`
	CreatedAt    time.Time         `json:"created_at"`
	UpdatedAt    time.Time         `json:"updated_at"`
	Timeout      int               `json:"timeout,omitempty"`
	ErrorMessage *string           `json:"error_message,omitempty"`
}

// CreateSandboxParams contains parameters for creating a sandbox
type CreateSandboxParams struct {
	WorkspaceID string            `json:"workspace_id"`
	Template    string            `json:"template,omitempty"`
	Name        string            `json:"name,omitempty"`
	Env         map[string]string `json:"env,omitempty"`
	Metadata    map[string]string `json:"metadata,omitempty"`
	Timeout     int               `json:"timeout,omitempty"`
}

// CommandResult contains the result of a command execution
type CommandResult struct {
	ExitCode int    `json:"exit_code"`
	Stdout   string `json:"stdout"`
	Stderr   string `json:"stderr"`
}

// RunCommandOptions contains options for running a command
type RunCommandOptions struct {
	Args    []string          `json:"args,omitempty"`
	Env     map[string]string `json:"env,omitempty"`
	Cwd     string            `json:"cwd,omitempty"`
	Timeout int               `json:"timeout,omitempty"`
}

// ProcessEventType represents the type of process event
type ProcessEventType string

const (
	ProcessEventStdout ProcessEventType = "stdout"
	ProcessEventStderr ProcessEventType = "stderr"
	ProcessEventExit   ProcessEventType = "exit"
	ProcessEventError  ProcessEventType = "error"
)

// ProcessEvent represents an event from a streaming process
type ProcessEvent struct {
	Type    ProcessEventType `json:"type"`
	Data    string           `json:"data,omitempty"`
	Code    *int             `json:"code,omitempty"`
	Message string           `json:"message,omitempty"`
}

// PtyHandle represents a PTY session
type PtyHandle struct {
	ID        string `json:"id"`
	SandboxID string `json:"sandbox_id"`
	Cols      int    `json:"cols"`
	Rows      int    `json:"rows"`
}

// PtyOptions contains options for creating a PTY
type PtyOptions struct {
	Cols    int               `json:"cols,omitempty"`
	Rows    int               `json:"rows,omitempty"`
	Env     map[string]string `json:"env,omitempty"`
	Command string            `json:"command,omitempty"`
}

// FileInfo represents information about a file
type FileInfo struct {
	Name    string    `json:"name"`
	Path    string    `json:"path"`
	Size    int64     `json:"size"`
	Mode    string    `json:"mode"`
	ModTime time.Time `json:"mod_time"`
	IsDir   bool      `json:"is_dir"`
}

// ListSandboxesResponse represents the response from listing sandboxes
type ListSandboxesResponse struct {
	Sandboxes []Sandbox `json:"sandboxes"`
	Total     int       `json:"total"`
}

// DeleteResponse represents a delete operation response
type DeleteResponse struct {
	Success bool `json:"success"`
}
