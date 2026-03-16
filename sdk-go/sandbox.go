package workspace

import (
	"context"
	"fmt"
	"time"

	pb "github.com/OpenElevo/ElevoWorkspace/sdk-go/proto/workspace/v1"
)

// SandboxService provides operations for managing sandboxes
type SandboxService struct {
	client *Client
}

// Create creates a new sandbox bound to a namespace
func (s *SandboxService) Create(ctx context.Context, params *CreateSandboxParams) (*Sandbox, error) {
	if params == nil {
		return nil, fmt.Errorf("params cannot be nil")
	}

	nsID := params.NamespaceID
	if nsID == "" {
		nsID = params.WorkspaceID // backward compat
	}
	if nsID == "" {
		return nil, fmt.Errorf("namespace_id (or workspace_id) is required")
	}

	req := &pb.CreateSandboxRequest{
		WorkspaceId: nsID,
		Env:         params.Env,
		Metadata:    params.Metadata,
	}

	if params.Template != "" {
		req.Template = &params.Template
	}
	if params.Name != "" {
		req.Name = &params.Name
	}
	if params.Timeout > 0 {
		t := uint64(params.Timeout)
		req.Timeout = &t
	}

	resp, err := s.client.sandboxClient.CreateSandbox(s.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	return protoToSandbox(resp.Sandbox), nil
}

// Get retrieves a sandbox by ID
func (s *SandboxService) Get(ctx context.Context, id string) (*Sandbox, error) {
	req := &pb.GetSandboxRequest{Id: id}

	resp, err := s.client.sandboxClient.GetSandbox(s.client.withAuth(ctx), req)
	if err != nil {
		grpcErr := convertGrpcError(err)
		if IsNotFound(grpcErr) {
			return nil, &SandboxNotFoundError{SandboxID: id}
		}
		return nil, grpcErr
	}

	return protoToSandbox(resp.Sandbox), nil
}

// List returns all sandboxes
func (s *SandboxService) List(ctx context.Context) ([]Sandbox, error) {
	req := &pb.ListSandboxesRequest{}

	resp, err := s.client.sandboxClient.ListSandboxes(s.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	sandboxes := make([]Sandbox, 0, len(resp.Sandboxes))
	for _, sb := range resp.Sandboxes {
		sandboxes = append(sandboxes, *protoToSandbox(sb))
	}

	return sandboxes, nil
}

// ListWithFilter returns sandboxes matching the given state
func (s *SandboxService) ListWithFilter(ctx context.Context, state SandboxState) ([]Sandbox, error) {
	protoState := sandboxStateToProto(state)
	req := &pb.ListSandboxesRequest{
		State: (*pb.SandboxState)(&protoState),
	}

	resp, err := s.client.sandboxClient.ListSandboxes(s.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	sandboxes := make([]Sandbox, 0, len(resp.Sandboxes))
	for _, sb := range resp.Sandboxes {
		sandboxes = append(sandboxes, *protoToSandbox(sb))
	}

	return sandboxes, nil
}

// Delete deletes a sandbox
func (s *SandboxService) Delete(ctx context.Context, id string, force bool) error {
	req := &pb.DeleteSandboxRequest{
		Id:    id,
		Force: force,
	}

	_, err := s.client.sandboxClient.DeleteSandbox(s.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
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
					StatusCode: 500,
					Message:    fmt.Sprintf("sandbox failed: %s", msg),
				}
			}

			// Small delay before next poll
			time.Sleep(100 * time.Millisecond)
		}
	}
}

// ==================== Helper functions ====================

func protoToSandbox(sb *pb.Sandbox) *Sandbox {
	if sb == nil {
		return nil
	}

	var createdAt, updatedAt time.Time
	if sb.CreatedAt != nil {
		createdAt = sb.CreatedAt.AsTime()
	}
	if sb.UpdatedAt != nil {
		updatedAt = sb.UpdatedAt.AsTime()
	}

	var errorMessage *string
	if sb.ErrorMessage != nil && *sb.ErrorMessage != "" {
		errorMessage = sb.ErrorMessage
	}

	var name string
	if sb.Name != nil {
		name = *sb.Name
	}

	return &Sandbox{
		ID:           sb.Id,
		WorkspaceID:  sb.WorkspaceId,
		NamespaceID:  sb.WorkspaceId, // Server maps namespace_id to workspace_id in proto
		Name:         name,
		Template:     sb.Template,
		State:        protoToSandboxState(pb.SandboxState(sb.State)),
		Env:          sb.Env,
		Metadata:     sb.Metadata,
		Timeout:      int64(sb.Timeout),
		ErrorMessage: errorMessage,
		CreatedAt:    createdAt,
		UpdatedAt:    updatedAt,
	}
}

func protoToSandboxState(state pb.SandboxState) SandboxState {
	switch state {
	case pb.SandboxState_SANDBOX_STATE_STARTING:
		return SandboxStateStarting
	case pb.SandboxState_SANDBOX_STATE_RUNNING:
		return SandboxStateRunning
	case pb.SandboxState_SANDBOX_STATE_STOPPING:
		return SandboxStateStopping
	case pb.SandboxState_SANDBOX_STATE_STOPPED:
		return SandboxStateStopped
	case pb.SandboxState_SANDBOX_STATE_ERROR:
		return SandboxStateFailed
	default:
		return SandboxStateUnknown
	}
}

func sandboxStateToProto(state SandboxState) int32 {
	switch state {
	case SandboxStateStarting:
		return int32(pb.SandboxState_SANDBOX_STATE_STARTING)
	case SandboxStateRunning:
		return int32(pb.SandboxState_SANDBOX_STATE_RUNNING)
	case SandboxStateStopping:
		return int32(pb.SandboxState_SANDBOX_STATE_STOPPING)
	case SandboxStateStopped:
		return int32(pb.SandboxState_SANDBOX_STATE_STOPPED)
	case SandboxStateFailed:
		return int32(pb.SandboxState_SANDBOX_STATE_ERROR)
	default:
		return int32(pb.SandboxState_SANDBOX_STATE_UNSPECIFIED)
	}
}
