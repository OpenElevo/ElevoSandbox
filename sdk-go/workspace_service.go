package workspace

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
)

// WorkspaceService provides operations for managing workspaces and file operations
type WorkspaceService struct {
	client *Client
}

// ==================== Workspace CRUD ====================

// Create creates a new workspace
func (w *WorkspaceService) Create(ctx context.Context, params *CreateWorkspaceParams) (*Workspace, error) {
	if params == nil {
		params = &CreateWorkspaceParams{}
	}

	var workspace Workspace
	err := w.client.doRequest(ctx, http.MethodPost, "/workspaces", params, &workspace)
	if err != nil {
		return nil, err
	}

	return &workspace, nil
}

// Get retrieves a workspace by ID
func (w *WorkspaceService) Get(ctx context.Context, id string) (*Workspace, error) {
	var workspace Workspace
	err := w.client.doRequest(ctx, http.MethodGet, fmt.Sprintf("/workspaces/%s", id), nil, &workspace)
	if err != nil {
		return nil, err
	}

	return &workspace, nil
}

// List returns all workspaces
func (w *WorkspaceService) List(ctx context.Context) ([]Workspace, error) {
	var response ListWorkspacesResponse
	err := w.client.doRequest(ctx, http.MethodGet, "/workspaces", nil, &response)
	if err != nil {
		return nil, err
	}

	return response.Workspaces, nil
}

// Delete deletes a workspace
func (w *WorkspaceService) Delete(ctx context.Context, id string) error {
	apiPath := fmt.Sprintf("/workspaces/%s", id)

	var response DeleteResponse
	err := w.client.doRequest(ctx, http.MethodDelete, apiPath, nil, &response)
	if err != nil {
		return err
	}

	return nil
}

// Exists checks if a workspace exists
func (w *WorkspaceService) Exists(ctx context.Context, id string) (bool, error) {
	_, err := w.Get(ctx, id)
	if err != nil {
		if IsNotFound(err) {
			return false, nil
		}
		return false, err
	}
	return true, nil
}

// ==================== File Operations ====================

// ReadFile reads the content of a file from a workspace
func (w *WorkspaceService) ReadFile(ctx context.Context, workspaceID, filePath string) ([]byte, error) {
	params := url.Values{}
	params.Set("path", filePath)

	var result struct {
		Content string `json:"content"`
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files?%s", workspaceID, params.Encode())
	err := w.client.doRequest(ctx, http.MethodGet, apiPath, nil, &result)
	if err != nil {
		return nil, err
	}

	return []byte(result.Content), nil
}

// ReadFileString reads the content of a file as a string from a workspace
func (w *WorkspaceService) ReadFileString(ctx context.Context, workspaceID, filePath string) (string, error) {
	content, err := w.ReadFile(ctx, workspaceID, filePath)
	if err != nil {
		return "", err
	}
	return string(content), nil
}

// WriteFile writes content to a file in a workspace
func (w *WorkspaceService) WriteFile(ctx context.Context, workspaceID, filePath string, content []byte) error {
	params := url.Values{}
	params.Set("path", filePath)

	req := map[string]string{
		"content": string(content),
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files?%s", workspaceID, params.Encode())
	return w.client.doRequest(ctx, http.MethodPut, apiPath, req, nil)
}

// WriteFileString writes a string to a file in a workspace
func (w *WorkspaceService) WriteFileString(ctx context.Context, workspaceID, filePath, content string) error {
	return w.WriteFile(ctx, workspaceID, filePath, []byte(content))
}

// Mkdir creates a directory in a workspace
func (w *WorkspaceService) Mkdir(ctx context.Context, workspaceID, dirPath string) error {
	req := map[string]string{
		"path": dirPath,
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files/mkdir", workspaceID)
	return w.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// ListFiles lists files in a directory in a workspace
func (w *WorkspaceService) ListFiles(ctx context.Context, workspaceID, dirPath string) ([]FileInfo, error) {
	params := url.Values{}
	params.Set("path", dirPath)

	var result struct {
		Files []FileInfo `json:"files"`
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files/list?%s", workspaceID, params.Encode())
	err := w.client.doRequest(ctx, http.MethodGet, apiPath, nil, &result)
	if err != nil {
		return nil, err
	}

	return result.Files, nil
}

// DeleteFile removes a file or directory from a workspace
func (w *WorkspaceService) DeleteFile(ctx context.Context, workspaceID, targetPath string, recursive bool) error {
	params := url.Values{}
	params.Set("path", targetPath)
	if recursive {
		params.Set("recursive", "true")
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files?%s", workspaceID, params.Encode())
	return w.client.doRequest(ctx, http.MethodDelete, apiPath, nil, nil)
}

// MoveFile moves/renames a file or directory in a workspace
func (w *WorkspaceService) MoveFile(ctx context.Context, workspaceID, srcPath, dstPath string) error {
	req := map[string]string{
		"source":      srcPath,
		"destination": dstPath,
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files/move", workspaceID)
	return w.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// CopyFile copies a file or directory in a workspace
func (w *WorkspaceService) CopyFile(ctx context.Context, workspaceID, srcPath, dstPath string) error {
	req := map[string]string{
		"source":      srcPath,
		"destination": dstPath,
	}

	apiPath := fmt.Sprintf("/workspaces/%s/files/copy", workspaceID)
	return w.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// GetFileInfo returns information about a file in a workspace
func (w *WorkspaceService) GetFileInfo(ctx context.Context, workspaceID, filePath string) (*FileInfo, error) {
	params := url.Values{}
	params.Set("path", filePath)

	var info FileInfo
	apiPath := fmt.Sprintf("/workspaces/%s/files/info?%s", workspaceID, params.Encode())
	err := w.client.doRequest(ctx, http.MethodGet, apiPath, nil, &info)
	if err != nil {
		return nil, err
	}

	return &info, nil
}

// FileExists checks if a file or directory exists in a workspace
func (w *WorkspaceService) FileExists(ctx context.Context, workspaceID, targetPath string) (bool, error) {
	_, err := w.GetFileInfo(ctx, workspaceID, targetPath)
	if err != nil {
		if IsNotFound(err) {
			return false, nil
		}
		return false, err
	}
	return true, nil
}
