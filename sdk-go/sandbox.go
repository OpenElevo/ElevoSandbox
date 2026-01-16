package workspace

import (
	"context"
	"fmt"
	"net/http"
)

// SandboxService provides operations for managing sandboxes
type SandboxService struct {
	client *Client
}

// Create creates a new sandbox bound to a workspace
func (s *SandboxService) Create(ctx context.Context, params *CreateSandboxParams) (*Sandbox, error) {
	if params == nil {
		return nil, fmt.Errorf("params cannot be nil, workspace_id is required")
	}
	if params.WorkspaceID == "" {
		return nil, fmt.Errorf("workspace_id is required")
	}

	var sandbox Sandbox
	err := s.client.doRequest(ctx, http.MethodPost, "/sandboxes", params, &sandbox)
	if err != nil {
		return nil, err
	}

	return &sandbox, nil
}

// Get retrieves a sandbox by ID
func (s *SandboxService) Get(ctx context.Context, id string) (*Sandbox, error) {
	var sandbox Sandbox
	err := s.client.doRequest(ctx, http.MethodGet, fmt.Sprintf("/sandboxes/%s", id), nil, &sandbox)
	if err != nil {
		if IsNotFound(err) {
			return nil, &SandboxNotFoundError{SandboxID: id}
		}
		return nil, err
	}

	return &sandbox, nil
}

// List returns all sandboxes
func (s *SandboxService) List(ctx context.Context) ([]Sandbox, error) {
	var response ListSandboxesResponse
	err := s.client.doRequest(ctx, http.MethodGet, "/sandboxes", nil, &response)
	if err != nil {
		return nil, err
	}

	return response.Sandboxes, nil
}

// ListWithFilter returns sandboxes matching the given state
func (s *SandboxService) ListWithFilter(ctx context.Context, state SandboxState) ([]Sandbox, error) {
	var response ListSandboxesResponse
	path := fmt.Sprintf("/sandboxes?state=%s", state)
	err := s.client.doRequest(ctx, http.MethodGet, path, nil, &response)
	if err != nil {
		return nil, err
	}

	return response.Sandboxes, nil
}

// Delete deletes a sandbox
func (s *SandboxService) Delete(ctx context.Context, id string, force bool) error {
	path := fmt.Sprintf("/sandboxes/%s", id)
	if force {
		path += "?force=true"
	}

	var response DeleteResponse
	err := s.client.doRequest(ctx, http.MethodDelete, path, nil, &response)
	if err != nil {
		return err
	}

	return nil
}

// Exists checks if a sandbox exists
func (s *SandboxService) Exists(ctx context.Context, id string) (bool, error) {
	_, err := s.Get(ctx, id)
	if err != nil {
		if IsNotFound(err) {
			return false, nil
		}
		return false, err
	}
	return true, nil
}

// WaitForState waits for a sandbox to reach a specific state
func (s *SandboxService) WaitForState(ctx context.Context, id string, targetState SandboxState) (*Sandbox, error) {
	for {
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		default:
			sandbox, err := s.Get(ctx, id)
			if err != nil {
				return nil, err
			}

			if sandbox.State == targetState {
				return sandbox, nil
			}

			if sandbox.State == SandboxStateFailed {
				msg := "unknown error"
				if sandbox.ErrorMessage != nil {
					msg = *sandbox.ErrorMessage
				}
				return nil, &Error{
					StatusCode: http.StatusInternalServerError,
					Message:    fmt.Sprintf("sandbox failed: %s", msg),
				}
			}
		}
	}
}
