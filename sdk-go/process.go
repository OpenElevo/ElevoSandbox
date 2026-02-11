package workspace

import (
	"context"
	"fmt"
	"io"

	pb "github.com/OpenElevo/ElevoSandbox/sdk-go/proto/workspace/v1"
)

// ProcessService provides operations for executing commands in sandboxes
type ProcessService struct {
	client *Client
}

// Run executes a command and waits for it to complete
func (p *ProcessService) Run(ctx context.Context, sandboxID string, command string, opts *RunCommandOptions) (*CommandResult, error) {
	if opts == nil {
		opts = &RunCommandOptions{}
	}

	var timeoutMs *uint64
	if opts.Timeout > 0 {
		t := uint64(opts.Timeout)
		timeoutMs = &t
	}

	req := &pb.RunCommandRequest{
		SandboxId: sandboxID,
		Command:   command,
		Args:      opts.Args,
		Env:       opts.Env,
		TimeoutMs: timeoutMs,
	}

	if opts.Cwd != "" {
		req.Cwd = &opts.Cwd
	}

	if req.Args == nil {
		req.Args = []string{}
	}
	if req.Env == nil {
		req.Env = map[string]string{}
	}

	resp, err := p.client.processClient.RunCommand(p.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	if resp.Result == nil {
		return nil, fmt.Errorf("no result in response")
	}

	return &CommandResult{
		ExitCode: int(resp.Result.ExitCode),
		Stdout:   resp.Result.Stdout,
		Stderr:   resp.Result.Stderr,
	}, nil
}

// RunStream executes a command and returns a channel of events
func (p *ProcessService) RunStream(ctx context.Context, sandboxID string, command string, opts *RunCommandOptions) (<-chan ProcessEvent, <-chan error) {
	eventCh := make(chan ProcessEvent, 100)
	errCh := make(chan error, 1)

	go func() {
		defer close(eventCh)
		defer close(errCh)

		if opts == nil {
			opts = &RunCommandOptions{}
		}

		var timeoutMs *uint64
		if opts.Timeout > 0 {
			t := uint64(opts.Timeout)
			timeoutMs = &t
		}

		req := &pb.RunCommandRequest{
			SandboxId: sandboxID,
			Command:   command,
			Args:      opts.Args,
			Env:       opts.Env,
			TimeoutMs: timeoutMs,
		}

		if opts.Cwd != "" {
			req.Cwd = &opts.Cwd
		}

		if req.Args == nil {
			req.Args = []string{}
		}
		if req.Env == nil {
			req.Env = map[string]string{}
		}

		stream, err := p.client.processClient.RunCommandStream(p.client.withAuth(ctx), req)
		if err != nil {
			errCh <- convertGrpcError(err)
			return
		}

		for {
			event, err := stream.Recv()
			if err == io.EOF {
				return
			}
			if err != nil {
				errCh <- convertGrpcError(err)
				return
			}

			processEvent := protoToProcessEvent(event)

			select {
			case eventCh <- processEvent:
			case <-ctx.Done():
				return
			}

			// Exit or error event signals end of stream
			if processEvent.Type == ProcessEventTypeExit || processEvent.Type == ProcessEventTypeError {
				return
			}
		}
	}()

	return eventCh, errCh
}

// Kill terminates a running process
func (p *ProcessService) Kill(ctx context.Context, sandboxID string, pid int, signal int) error {
	if signal == 0 {
		signal = 15 // SIGTERM
	}

	sig := int32(signal)
	req := &pb.KillProcessRequest{
		SandboxId: sandboxID,
		Pid:       uint32(pid),
		Signal:    &sig,
	}

	_, err := p.client.processClient.KillProcess(p.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// Exec is a convenience method that runs a command and returns stdout
// It returns an error if the exit code is non-zero
func (p *ProcessService) Exec(ctx context.Context, sandboxID string, command string, args ...string) (string, error) {
	result, err := p.Run(ctx, sandboxID, command, &RunCommandOptions{Args: args})
	if err != nil {
		return "", err
	}

	if result.ExitCode != 0 {
		return "", &ProcessError{
			SandboxID: sandboxID,
			Command:   command,
			Message:   fmt.Sprintf("exit code %d: %s", result.ExitCode, result.Stderr),
		}
	}

	return result.Stdout, nil
}

// Shell runs a shell command using bash -c
func (p *ProcessService) Shell(ctx context.Context, sandboxID string, script string, env map[string]string) (*CommandResult, error) {
	return p.Run(ctx, sandboxID, "bash", &RunCommandOptions{
		Args: []string{"-c", script},
		Env:  env,
	})
}

// ==================== Helper functions ====================

func protoToProcessEvent(event *pb.ProcessEvent) ProcessEvent {
	if event == nil || event.Event == nil {
		return ProcessEvent{}
	}

	switch e := event.Event.(type) {
	case *pb.ProcessEvent_Stdout:
		return ProcessEvent{
			Type: ProcessEventTypeStdout,
			Data: e.Stdout.Data,
		}
	case *pb.ProcessEvent_Stderr:
		return ProcessEvent{
			Type: ProcessEventTypeStderr,
			Data: e.Stderr.Data,
		}
	case *pb.ProcessEvent_Exit:
		code := int(e.Exit.Code)
		return ProcessEvent{
			Type: ProcessEventTypeExit,
			Code: &code,
		}
	case *pb.ProcessEvent_Error:
		return ProcessEvent{
			Type:    ProcessEventTypeError,
			Message: e.Error.Message,
		}
	default:
		return ProcessEvent{}
	}
}
