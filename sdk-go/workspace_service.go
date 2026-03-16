package workspace

import (
	"context"
	"time"

	pb "github.com/OpenElevo/ElevoWorkspace/sdk-go/proto/workspace/v1"
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

	req := &pb.CreateWorkspaceRequest{
		Metadata: params.Metadata,
	}

	if params.Name != "" {
		req.Name = &params.Name
	}

	if params.StorageType != "" {
		st := string(params.StorageType)
		req.StorageType = &st
	}

	resp, err := w.client.workspaceClient.CreateWorkspace(w.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	return protoToWorkspace(resp.Workspace), nil
}

// Get retrieves a workspace by ID
func (w *WorkspaceService) Get(ctx context.Context, id string) (*Workspace, error) {
	req := &pb.GetWorkspaceRequest{Id: id}

	resp, err := w.client.workspaceClient.GetWorkspace(w.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	return protoToWorkspace(resp.Workspace), nil
}

// List returns all workspaces
func (w *WorkspaceService) List(ctx context.Context) ([]Workspace, error) {
	req := &pb.ListWorkspacesRequest{}

	resp, err := w.client.workspaceClient.ListWorkspaces(w.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	workspaces := make([]Workspace, 0, len(resp.Workspaces))
	for _, ws := range resp.Workspaces {
		workspaces = append(workspaces, *protoToWorkspace(ws))
	}

	return workspaces, nil
}

// Delete deletes a workspace
func (w *WorkspaceService) Delete(ctx context.Context, id string) error {
	req := &pb.DeleteWorkspaceRequest{Id: id}

	_, err := w.client.workspaceClient.DeleteWorkspace(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
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
	req := &pb.ReadFileRequest{
		WorkspaceId: workspaceID,
		Path:        filePath,
	}

	resp, err := w.client.workspaceClient.ReadFile(w.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	return resp.Content, nil
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
	req := &pb.WriteFileRequest{
		WorkspaceId: workspaceID,
		Path:        filePath,
		Content:     content,
	}

	_, err := w.client.workspaceClient.WriteFile(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// WriteFileString writes a string to a file in a workspace
func (w *WorkspaceService) WriteFileString(ctx context.Context, workspaceID, filePath, content string) error {
	return w.WriteFile(ctx, workspaceID, filePath, []byte(content))
}

// Mkdir creates a directory in a workspace
func (w *WorkspaceService) Mkdir(ctx context.Context, workspaceID, dirPath string) error {
	req := &pb.MkdirRequest{
		WorkspaceId: workspaceID,
		Path:        dirPath,
	}

	_, err := w.client.workspaceClient.Mkdir(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// ListFiles lists files in a directory in a workspace
func (w *WorkspaceService) ListFiles(ctx context.Context, workspaceID, dirPath string) ([]FileInfo, error) {
	req := &pb.ListFilesRequest{
		WorkspaceId: workspaceID,
		Path:        dirPath,
	}

	resp, err := w.client.workspaceClient.ListFiles(w.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	files := make([]FileInfo, 0, len(resp.Files))
	for _, f := range resp.Files {
		files = append(files, protoToFileInfo(f))
	}

	return files, nil
}

// DeleteFile removes a file or directory from a workspace
func (w *WorkspaceService) DeleteFile(ctx context.Context, workspaceID, targetPath string, recursive bool) error {
	req := &pb.DeleteFileRequest{
		WorkspaceId: workspaceID,
		Path:        targetPath,
		Recursive:   recursive,
	}

	_, err := w.client.workspaceClient.DeleteFile(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// MoveFile moves/renames a file or directory in a workspace
func (w *WorkspaceService) MoveFile(ctx context.Context, workspaceID, srcPath, dstPath string) error {
	req := &pb.MoveFileRequest{
		WorkspaceId: workspaceID,
		Source:      srcPath,
		Destination: dstPath,
	}

	_, err := w.client.workspaceClient.MoveFile(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// CopyFile copies a file or directory in a workspace
func (w *WorkspaceService) CopyFile(ctx context.Context, workspaceID, srcPath, dstPath string) error {
	req := &pb.CopyFileRequest{
		WorkspaceId: workspaceID,
		Source:      srcPath,
		Destination: dstPath,
	}

	_, err := w.client.workspaceClient.CopyFile(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// GetFileInfo returns information about a file in a workspace
func (w *WorkspaceService) GetFileInfo(ctx context.Context, workspaceID, filePath string) (*FileInfo, error) {
	req := &pb.GetFileInfoRequest{
		WorkspaceId: workspaceID,
		Path:        filePath,
	}

	resp, err := w.client.workspaceClient.GetFileInfo(w.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	info := protoToFileInfo(resp.File)
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

// ==================== NFS Transport ====================

// RegisterNfsTransport switches a remote workspace from gRPC to NFS transport.
func (w *WorkspaceService) RegisterNfsTransport(ctx context.Context, workspaceID, nfsURL string) error {
	req := &pb.RegisterNfsTransportRequest{
		WorkspaceId: workspaceID,
		NfsUrl:      nfsURL,
	}

	_, err := w.client.workspaceClient.RegisterNfsTransport(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// UnregisterNfsTransport switches a remote workspace from NFS back to gRPC transport.
func (w *WorkspaceService) UnregisterNfsTransport(ctx context.Context, workspaceID string) error {
	req := &pb.UnregisterNfsTransportRequest{
		WorkspaceId: workspaceID,
	}

	_, err := w.client.workspaceClient.UnregisterNfsTransport(w.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// ==================== Helper functions ====================

func protoToWorkspace(ws *pb.Workspace) *Workspace {
	if ws == nil {
		return nil
	}

	var createdAt, updatedAt time.Time
	if ws.CreatedAt != nil {
		createdAt = ws.CreatedAt.AsTime()
	}
	if ws.UpdatedAt != nil {
		updatedAt = ws.UpdatedAt.AsTime()
	}

	var name, nfsURL string
	if ws.Name != nil {
		name = *ws.Name
	}
	if ws.NfsUrl != nil {
		nfsURL = *ws.NfsUrl
	}

	storageType := StorageTypeManaged
	if ws.StorageType != "" {
		storageType = StorageType(ws.StorageType)
	}

	return &Workspace{
		ID:            ws.Id,
		Name:          name,
		NfsURL:        nfsURL,
		StorageType:   storageType,
		StorageConfig: ws.StorageConfig,
		Metadata:      ws.Metadata,
		CreatedAt:     createdAt,
		UpdatedAt:     updatedAt,
	}
}

func protoToFileInfo(f *pb.FileInfo) FileInfo {
	if f == nil {
		return FileInfo{}
	}

	var modifiedAt *time.Time
	if f.ModifiedAt != nil {
		t := f.ModifiedAt.AsTime()
		modifiedAt = &t
	}

	return FileInfo{
		Name:       f.Name,
		Path:       f.Path,
		Type:       f.Type,
		Size:       int64(f.Size),
		ModifiedAt: modifiedAt,
	}
}
