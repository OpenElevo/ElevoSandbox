package workspace

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"path"
)

// FileSystemService provides operations for filesystem access
type FileSystemService struct {
	client *Client
}

// Read reads the content of a file
func (f *FileSystemService) Read(ctx context.Context, sandboxID, filePath string) ([]byte, error) {
	params := url.Values{}
	params.Set("path", filePath)

	var result struct {
		Content string `json:"content"`
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/read?%s", sandboxID, params.Encode())
	err := f.client.doRequest(ctx, http.MethodGet, apiPath, nil, &result)
	if err != nil {
		return nil, err
	}

	return []byte(result.Content), nil
}

// ReadString reads the content of a file as a string
func (f *FileSystemService) ReadString(ctx context.Context, sandboxID, filePath string) (string, error) {
	content, err := f.Read(ctx, sandboxID, filePath)
	if err != nil {
		return "", err
	}
	return string(content), nil
}

// Write writes content to a file
func (f *FileSystemService) Write(ctx context.Context, sandboxID, filePath string, content []byte) error {
	req := map[string]string{
		"path":    filePath,
		"content": string(content),
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/write", sandboxID)
	return f.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// WriteString writes a string to a file
func (f *FileSystemService) WriteString(ctx context.Context, sandboxID, filePath, content string) error {
	return f.Write(ctx, sandboxID, filePath, []byte(content))
}

// Mkdir creates a directory
func (f *FileSystemService) Mkdir(ctx context.Context, sandboxID, dirPath string, recursive bool) error {
	req := map[string]interface{}{
		"path":      dirPath,
		"recursive": recursive,
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/mkdir", sandboxID)
	return f.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// List lists files in a directory
func (f *FileSystemService) List(ctx context.Context, sandboxID, dirPath string) ([]FileInfo, error) {
	params := url.Values{}
	params.Set("path", dirPath)

	var result struct {
		Files []FileInfo `json:"files"`
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/list?%s", sandboxID, params.Encode())
	err := f.client.doRequest(ctx, http.MethodGet, apiPath, nil, &result)
	if err != nil {
		return nil, err
	}

	return result.Files, nil
}

// Remove removes a file or directory
func (f *FileSystemService) Remove(ctx context.Context, sandboxID, targetPath string, recursive bool) error {
	params := url.Values{}
	params.Set("path", targetPath)
	if recursive {
		params.Set("recursive", "true")
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/delete?%s", sandboxID, params.Encode())
	return f.client.doRequest(ctx, http.MethodDelete, apiPath, nil, nil)
}

// Move moves/renames a file or directory
func (f *FileSystemService) Move(ctx context.Context, sandboxID, srcPath, dstPath string) error {
	req := map[string]string{
		"src": srcPath,
		"dst": dstPath,
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/move", sandboxID)
	return f.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// Copy copies a file or directory
func (f *FileSystemService) Copy(ctx context.Context, sandboxID, srcPath, dstPath string) error {
	req := map[string]string{
		"src": srcPath,
		"dst": dstPath,
	}

	apiPath := fmt.Sprintf("/sandboxes/%s/fs/copy", sandboxID)
	return f.client.doRequest(ctx, http.MethodPost, apiPath, req, nil)
}

// Stat returns information about a file
func (f *FileSystemService) Stat(ctx context.Context, sandboxID, filePath string) (*FileInfo, error) {
	params := url.Values{}
	params.Set("path", filePath)

	var info FileInfo
	apiPath := fmt.Sprintf("/sandboxes/%s/fs/stat?%s", sandboxID, params.Encode())
	err := f.client.doRequest(ctx, http.MethodGet, apiPath, nil, &info)
	if err != nil {
		return nil, err
	}

	return &info, nil
}

// Exists checks if a file or directory exists
func (f *FileSystemService) Exists(ctx context.Context, sandboxID, targetPath string) (bool, error) {
	_, err := f.Stat(ctx, sandboxID, targetPath)
	if err != nil {
		if IsNotFound(err) {
			return false, nil
		}
		return false, err
	}
	return true, nil
}

// Join joins path elements
func (f *FileSystemService) Join(elem ...string) string {
	return path.Join(elem...)
}

// ReadDir is an alias for List
func (f *FileSystemService) ReadDir(ctx context.Context, sandboxID, dirPath string) ([]FileInfo, error) {
	return f.List(ctx, sandboxID, dirPath)
}

// MkdirAll creates a directory and all parent directories
func (f *FileSystemService) MkdirAll(ctx context.Context, sandboxID, dirPath string) error {
	return f.Mkdir(ctx, sandboxID, dirPath, true)
}

// RemoveAll removes a file or directory recursively
func (f *FileSystemService) RemoveAll(ctx context.Context, sandboxID, targetPath string) error {
	return f.Remove(ctx, sandboxID, targetPath, true)
}
