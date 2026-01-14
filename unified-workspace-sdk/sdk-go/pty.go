package workspace

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"sync"

	"github.com/gorilla/websocket"
)

// PtyService provides operations for PTY terminals
type PtyService struct {
	client *Client
}

// PtySession represents an active PTY session with WebSocket connection
type PtySession struct {
	Handle    *PtyHandle
	conn      *websocket.Conn
	client    *Client
	sandboxID string

	// Channels for communication
	incoming chan []byte
	outgoing chan []byte
	errors   chan error
	done     chan struct{}

	mu     sync.Mutex
	closed bool
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

	var handle PtyHandle
	path := fmt.Sprintf("/sandboxes/%s/pty", sandboxID)
	err := p.client.doRequest(ctx, http.MethodPost, path, opts, &handle)
	if err != nil {
		return nil, err
	}

	handle.SandboxID = sandboxID
	return &handle, nil
}

// Connect creates a PTY and establishes a WebSocket connection
func (p *PtyService) Connect(ctx context.Context, sandboxID string, opts *PtyOptions) (*PtySession, error) {
	handle, err := p.Create(ctx, sandboxID, opts)
	if err != nil {
		return nil, err
	}

	// Build WebSocket URL
	wsURL := fmt.Sprintf("ws%s/api/v1/sandboxes/%s/pty/%s",
		p.client.apiURL[4:], // Convert http(s) to ws(s)
		sandboxID,
		handle.ID,
	)

	// Connect WebSocket
	header := http.Header{}
	if p.client.apiKey != "" {
		header.Set("Authorization", fmt.Sprintf("Bearer %s", p.client.apiKey))
	}

	conn, _, err := websocket.DefaultDialer.DialContext(ctx, wsURL, header)
	if err != nil {
		return nil, &ConnectionError{URL: wsURL, Message: err.Error()}
	}

	session := &PtySession{
		Handle:    handle,
		conn:      conn,
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
	req := map[string]int{
		"cols": cols,
		"rows": rows,
	}

	path := fmt.Sprintf("/sandboxes/%s/pty/%s/resize", sandboxID, ptyID)
	return p.client.doRequest(ctx, http.MethodPost, path, req, nil)
}

// Kill terminates a PTY
func (p *PtyService) Kill(ctx context.Context, sandboxID, ptyID string) error {
	path := fmt.Sprintf("/sandboxes/%s/pty/%s", sandboxID, ptyID)
	return p.client.doRequest(ctx, http.MethodDelete, path, nil, nil)
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

// Resize resizes the PTY
func (s *PtySession) Resize(cols, rows int) error {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return fmt.Errorf("session is closed")
	}
	s.mu.Unlock()

	// Send resize message via WebSocket
	msg := map[string]interface{}{
		"type": "resize",
		"cols": cols,
		"rows": rows,
	}

	data, err := json.Marshal(msg)
	if err != nil {
		return err
	}

	return s.conn.WriteMessage(websocket.TextMessage, data)
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
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return nil
	}
	s.closed = true
	s.mu.Unlock()

	close(s.done)

	// Close WebSocket connection
	if err := s.conn.WriteMessage(websocket.CloseMessage,
		websocket.FormatCloseMessage(websocket.CloseNormalClosure, "")); err != nil {
		// Ignore error, connection might already be closed
	}

	return s.conn.Close()
}

// readLoop reads messages from WebSocket
func (s *PtySession) readLoop() {
	defer func() {
		s.mu.Lock()
		if !s.closed {
			close(s.done)
		}
		s.mu.Unlock()
	}()

	for {
		_, message, err := s.conn.ReadMessage()
		if err != nil {
			s.mu.Lock()
			closed := s.closed
			s.mu.Unlock()

			if !closed {
				select {
				case s.errors <- err:
				default:
				}
			}
			return
		}

		select {
		case s.incoming <- message:
		case <-s.done:
			return
		}
	}
}

// writeLoop writes messages to WebSocket
func (s *PtySession) writeLoop() {
	for {
		select {
		case data := <-s.outgoing:
			msg := map[string]interface{}{
				"type": "input",
				"data": string(data),
			}
			msgData, _ := json.Marshal(msg)

			if err := s.conn.WriteMessage(websocket.TextMessage, msgData); err != nil {
				s.mu.Lock()
				closed := s.closed
				s.mu.Unlock()

				if !closed {
					select {
					case s.errors <- err:
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
