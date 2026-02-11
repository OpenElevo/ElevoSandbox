package workspace

import (
	"context"
	"fmt"
	"io"
	"sync"

	pb "github.com/OpenElevo/ElevoSandbox/sdk-go/proto/workspace/v1"
)

// PtyService provides operations for PTY terminals
type PtyService struct {
	client *Client
}

// PtySession represents an active PTY session with gRPC bidirectional stream
type PtySession struct {
	Handle    *PtyHandle
	stream    pb.PtyService_PtyStreamClient
	client    *Client
	sandboxID string

	// Channels for communication
	incoming chan []byte
	outgoing chan []byte
	errors   chan error
	done     chan struct{}

	mu       sync.Mutex
	closed   bool
	closeOnce sync.Once
}

// Create creates a new PTY session
func (p *PtyService) Create(ctx context.Context, sandboxID string, opts *PtyOptions) (*PtyHandle, error) {
	if opts == nil {
		opts = &PtyOptions{
			Cols: 80,
			Rows: 24,
		}
	}

	if opts.Cols == 0 {
		opts.Cols = 80
	}
	if opts.Rows == 0 {
		opts.Rows = 24
	}

	var colsPtr, rowsPtr *uint32
	if opts.Cols > 0 {
		c := uint32(opts.Cols)
		colsPtr = &c
	}
	if opts.Rows > 0 {
		r := uint32(opts.Rows)
		rowsPtr = &r
	}

	req := &pb.CreatePtyRequest{
		SandboxId: sandboxID,
		Cols:      colsPtr,
		Rows:      rowsPtr,
		Env:       opts.Env,
	}

	if opts.Shell != "" {
		req.Shell = &opts.Shell
	}

	resp, err := p.client.ptyClient.CreatePty(p.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	if resp.Pty == nil {
		return nil, fmt.Errorf("no pty in response")
	}

	return &PtyHandle{
		ID:        resp.Pty.Id,
		SandboxID: resp.Pty.SandboxId,
		Cols:      int(resp.Pty.Cols),
		Rows:      int(resp.Pty.Rows),
	}, nil
}

// Connect creates a PTY and establishes a gRPC bidirectional stream
func (p *PtyService) Connect(ctx context.Context, sandboxID string, opts *PtyOptions) (*PtySession, error) {
	handle, err := p.Create(ctx, sandboxID, opts)
	if err != nil {
		return nil, err
	}

	// Establish bidirectional stream
	stream, err := p.client.ptyClient.PtyStream(p.client.withAuth(ctx))
	if err != nil {
		return nil, convertGrpcError(err)
	}

	// Send init message
	initReq := &pb.PtyStreamRequest{
		Request: &pb.PtyStreamRequest_Init{
			Init: &pb.PtyStreamInit{
				SandboxId: sandboxID,
				PtyId:     handle.ID,
			},
		},
	}

	if err := stream.Send(initReq); err != nil {
		// Clean up stream on error
		stream.CloseSend()
		return nil, convertGrpcError(err)
	}

	session := &PtySession{
		Handle:    handle,
		stream:    stream,
		client:    p.client,
		sandboxID: sandboxID,
		incoming:  make(chan []byte, 100),
		outgoing:  make(chan []byte, 100),
		errors:    make(chan error, 1),
		done:      make(chan struct{}),
	}

	// Start read/write goroutines
	go session.readLoop()
	go session.writeLoop()

	return session, nil
}

// Resize resizes a PTY
func (p *PtyService) Resize(ctx context.Context, sandboxID, ptyID string, cols, rows int) error {
	req := &pb.ResizePtyRequest{
		SandboxId: sandboxID,
		PtyId:     ptyID,
		Cols:      uint32(cols),
		Rows:      uint32(rows),
	}

	_, err := p.client.ptyClient.ResizePty(p.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// Kill terminates a PTY
func (p *PtyService) Kill(ctx context.Context, sandboxID, ptyID string) error {
	req := &pb.KillPtyRequest{
		SandboxId: sandboxID,
		PtyId:     ptyID,
	}

	_, err := p.client.ptyClient.KillPty(p.client.withAuth(ctx), req)
	if err != nil {
		return convertGrpcError(err)
	}

	return nil
}

// Read returns the channel for reading data from the PTY
func (s *PtySession) Read() <-chan []byte {
	return s.incoming
}

// Write sends data to the PTY
func (s *PtySession) Write(data []byte) error {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return fmt.Errorf("session is closed")
	}
	s.mu.Unlock()

	select {
	case s.outgoing <- data:
		return nil
	case <-s.done:
		return fmt.Errorf("session is closed")
	}
}

// WriteString sends a string to the PTY
func (s *PtySession) WriteString(data string) error {
	return s.Write([]byte(data))
}

// Resize resizes the PTY via the stream
func (s *PtySession) Resize(cols, rows int) error {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return fmt.Errorf("session is closed")
	}
	s.mu.Unlock()

	req := &pb.PtyStreamRequest{
		Request: &pb.PtyStreamRequest_Resize{
			Resize: &pb.PtyResizeEvent{
				Cols: uint32(cols),
				Rows: uint32(rows),
			},
		},
	}

	return s.stream.Send(req)
}

// Errors returns the channel for errors
func (s *PtySession) Errors() <-chan error {
	return s.errors
}

// Done returns a channel that is closed when the session ends
func (s *PtySession) Done() <-chan struct{} {
	return s.done
}

// Close closes the PTY session
func (s *PtySession) Close() error {
	var err error
	s.closeOnce.Do(func() {
		s.mu.Lock()
		s.closed = true
		s.mu.Unlock()

		close(s.done)

		// Close the gRPC stream
		err = s.stream.CloseSend()
	})
	return err
}

// readLoop reads messages from gRPC stream
func (s *PtySession) readLoop() {
	defer func() {
		// Use closeOnce to safely close done channel
		s.closeOnce.Do(func() {
			s.mu.Lock()
			s.closed = true
			s.mu.Unlock()
			close(s.done)
		})
	}()

	for {
		resp, err := s.stream.Recv()
		if err == io.EOF {
			return
		}
		if err != nil {
			s.mu.Lock()
			closed := s.closed
			s.mu.Unlock()

			if !closed {
				select {
				case s.errors <- convertGrpcError(err):
				default:
				}
			}
			return
		}

		switch r := resp.Response.(type) {
		case *pb.PtyStreamResponse_Output:
			select {
			case s.incoming <- r.Output:
			case <-s.done:
				return
			}
		case *pb.PtyStreamResponse_ExitCode:
			// PTY exited
			s.mu.Lock()
			closed := s.closed
			s.mu.Unlock()

			if !closed {
				select {
				case s.errors <- fmt.Errorf("PTY exited with code %d", r.ExitCode):
				default:
				}
			}
			return
		case *pb.PtyStreamResponse_Error:
			s.mu.Lock()
			closed := s.closed
			s.mu.Unlock()

			if !closed {
				select {
				case s.errors <- fmt.Errorf("PTY error: %s", r.Error):
				default:
				}
			}
			return
		}
	}
}

// writeLoop writes messages to gRPC stream
func (s *PtySession) writeLoop() {
	for {
		select {
		case data := <-s.outgoing:
			req := &pb.PtyStreamRequest{
				Request: &pb.PtyStreamRequest_Input{
					Input: data,
				},
			}

			if err := s.stream.Send(req); err != nil {
				s.mu.Lock()
				closed := s.closed
				s.mu.Unlock()

				if !closed {
					select {
					case s.errors <- convertGrpcError(err):
					default:
					}
				}
				return
			}
		case <-s.done:
			return
		}
	}
}
