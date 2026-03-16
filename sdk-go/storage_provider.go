package workspace

import (
	"context"
	"fmt"
	"log"
	"sync"
	"sync/atomic"
	"time"

	pb "github.com/OpenElevo/ElevoWorkspace/sdk-go/proto/workspace/v1"
	"golang.org/x/sys/unix"
	"google.golang.org/grpc"
	"google.golang.org/grpc/metadata"
)

// StorageProviderConfig configures the StorageProvider.
type StorageProviderConfig struct {
	// LocalDir is the local directory path to share.
	LocalDir string
	// WorkspaceID is the workspace this provider serves.
	WorkspaceID string
	// Token is the authentication token (reuses existing API key mechanism).
	Token string
	// WorkerPoolSize is the number of goroutines handling file operations (default 64).
	WorkerPoolSize int
	// ResponseBufferSize is the capacity of the response channel (default 256).
	ResponseBufferSize int
	// MaxConcurrentDataStreams is the maximum number of concurrent data stream RPCs (default 8).
	MaxConcurrentDataStreams int
	// OperationTimeout is the timeout for individual file operations and lock acquisitions (default 10s).
	OperationTimeout time.Duration
}

func (c *StorageProviderConfig) applyDefaults() {
	if c.WorkerPoolSize <= 0 {
		c.WorkerPoolSize = 64
	}
	if c.ResponseBufferSize <= 0 {
		c.ResponseBufferSize = 256
	}
	if c.MaxConcurrentDataStreams <= 0 {
		c.MaxConcurrentDataStreams = 8
	}
	if c.OperationTimeout <= 0 {
		c.OperationTimeout = 10 * time.Second
	}
}

// StorageProvider shares a local directory with the Server via gRPC reverse stream.
// The Server sends file operation requests; the provider executes them on the local
// filesystem and returns the results through the same control stream.
type StorageProvider struct {
	config StorageProviderConfig
	conn   *grpc.ClientConn
	client pb.ClientStorageServiceClient

	// responseCh serializes all writes to the control stream via responseWriter.
	responseCh chan *pb.ClientMessage

	// File change watcher.
	watcher *fileWatcher

	// Path safety validator (openat-based).
	pathGuard *pathGuard

	// Per-file write locks (map[string]*chanMutex).
	fileLocks sync.Map

	// Data stream concurrency limiter.
	dataStreamSem chan struct{}

	// Lifecycle.
	ctx    context.Context
	cancel context.CancelFunc
	wg     sync.WaitGroup

	// Connection state for external inspection.
	connected atomic.Bool
}

// NewStorageProvider creates a new StorageProvider.
// Call Share() to start the connection loop.
func NewStorageProvider(conn *grpc.ClientConn, config StorageProviderConfig) *StorageProvider {
	config.applyDefaults()
	ctx, cancel := context.WithCancel(context.Background())
	return &StorageProvider{
		config:        config,
		conn:          conn,
		client:        pb.NewClientStorageServiceClient(conn),
		responseCh:    make(chan *pb.ClientMessage, config.ResponseBufferSize),
		dataStreamSem: make(chan struct{}, config.MaxConcurrentDataStreams),
		ctx:           ctx,
		cancel:        cancel,
	}
}

// IsConnected returns whether the provider is currently connected to the server.
func (sp *StorageProvider) IsConnected() bool {
	return sp.connected.Load()
}

// Stop cancels the provider context, causing Share() to return.
func (sp *StorageProvider) Stop() {
	sp.cancel()
}

// withAuth adds authorization metadata to the context using the configured token.
func (sp *StorageProvider) withAuth(ctx context.Context) context.Context {
	if sp.config.Token != "" {
		return metadata.AppendToOutgoingContext(ctx, "authorization", "Bearer "+sp.config.Token)
	}
	return ctx
}

// Share starts the storage provider. It connects to the server, performs the
// handshake, and serves file operations until the context is cancelled.
// It reconnects automatically with exponential backoff on connection errors.
func (sp *StorageProvider) Share(ctx context.Context) error {
	// Initialize path safety validator.
	var err error
	sp.pathGuard, err = newPathGuard(sp.config.LocalDir)
	if err != nil {
		return fmt.Errorf("init path guard: %w", err)
	}
	defer sp.pathGuard.Close()

	// Start file change watcher.
	sp.watcher, err = newFileWatcher(sp.config.LocalDir, sp.responseCh)
	if err != nil {
		return fmt.Errorf("init file watcher: %w", err)
	}
	defer sp.watcher.Close()

	// Exponential backoff reconnection loop.
	backoff := time.Second
	const maxBackoff = 30 * time.Second

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-sp.ctx.Done():
			return sp.ctx.Err()
		default:
		}

		connectedAt, connErr := sp.connectAndServe(ctx)
		sp.connected.Store(false)

		if connErr != nil {
			log.Printf("[StorageProvider] connection error: %v, reconnecting in %v", connErr, backoff)
		}

		// Reset backoff if a connection was successfully established.
		if !connectedAt.IsZero() {
			backoff = time.Second
		}

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-sp.ctx.Done():
			return sp.ctx.Err()
		case <-time.After(backoff):
		}

		backoff = min(backoff*2, maxBackoff)
	}
}

// connectAndServe establishes one connection, performs the handshake, and
// processes requests until the stream ends. Returns the time of successful
// connection (zero if never connected).
func (sp *StorageProvider) connectAndServe(ctx context.Context) (time.Time, error) {
	// Establish control stream.
	stream, err := sp.client.Connect(sp.withAuth(ctx))
	if err != nil {
		return time.Time{}, fmt.Errorf("connect: %w", err)
	}

	// Send handshake.
	err = stream.Send(&pb.ClientMessage{
		Message: &pb.ClientMessage_Handshake{
			Handshake: &pb.StorageHandshake{
				WorkspaceId: sp.config.WorkspaceID,
				Token:       sp.config.Token,
			},
		},
	})
	if err != nil {
		return time.Time{}, fmt.Errorf("send handshake: %w", err)
	}

	// Wait for handshake ack.
	msg, err := stream.Recv()
	if err != nil {
		return time.Time{}, fmt.Errorf("recv handshake ack: %w", err)
	}
	ack := msg.GetHandshakeAck()
	if ack == nil || !ack.Success {
		errMsg := "unknown error"
		if ack != nil && ack.Error != nil {
			errMsg = *ack.Error
		}
		return time.Time{}, fmt.Errorf("handshake failed: %s", errMsg)
	}

	connectedAt := time.Now()
	sp.connected.Store(true)
	log.Printf("[StorageProvider] connected for workspace %s", sp.config.WorkspaceID)

	// Start the response writer goroutine (serializes stream.Send calls).
	sp.wg.Add(1)
	go sp.responseWriter(stream)

	// Start worker pool.
	requestCh := make(chan *pb.StorageOperationRequest, sp.config.WorkerPoolSize)
	for i := 0; i < sp.config.WorkerPoolSize; i++ {
		sp.wg.Add(1)
		go sp.worker(requestCh)
	}

	// Main loop: read requests from the server.
	var recvErr error
	for {
		msg, err := stream.Recv()
		if err != nil {
			recvErr = fmt.Errorf("recv: %w", err)
			break
		}

		switch m := msg.Message.(type) {
		case *pb.ServerStorageMessage_OperationRequest:
			select {
			case requestCh <- m.OperationRequest:
			case <-ctx.Done():
				recvErr = ctx.Err()
			}
		case *pb.ServerStorageMessage_Ping:
			sp.trySendResponse(&pb.ClientMessage{
				Message: &pb.ClientMessage_Pong{
					Pong: &pb.StoragePong{Timestamp: m.Ping.Timestamp},
				},
			})
		case *pb.ServerStorageMessage_StartDataTransfer:
			sp.wg.Add(1)
			go func() {
				defer sp.wg.Done()
				sp.handleDataTransfer(ctx, m.StartDataTransfer)
			}()
		default:
			log.Printf("[StorageProvider] unknown message type: %T", m)
		}

		if recvErr != nil {
			break
		}
	}

	// Signal workers to stop by closing the request channel.
	close(requestCh)

	// Wait for all goroutines (workers + responseWriter) from this connection
	// cycle to finish before returning, preventing goroutine leaks between
	// reconnect cycles.
	sp.wg.Wait()

	return connectedAt, recvErr
}

// trySendResponse attempts to send a message to the response channel.
// Returns false if the provider context is cancelled (avoiding deadlock when
// the responseWriter has exited and the channel is full).
func (sp *StorageProvider) trySendResponse(msg *pb.ClientMessage) bool {
	select {
	case sp.responseCh <- msg:
		return true
	case <-sp.ctx.Done():
		return false
	}
}

// responseWriter serializes all writes to the gRPC stream from a single goroutine.
// gRPC streams are not safe for concurrent Send() calls.
func (sp *StorageProvider) responseWriter(stream pb.ClientStorageService_ConnectClient) {
	defer sp.wg.Done()
	for {
		select {
		case <-sp.ctx.Done():
			return
		case msg, ok := <-sp.responseCh:
			if !ok {
				return
			}
			if err := stream.Send(msg); err != nil {
				log.Printf("[StorageProvider] send error: %v", err)
				return
			}
		}
	}
}

// worker processes file operation requests from the request channel.
func (sp *StorageProvider) worker(requestCh <-chan *pb.StorageOperationRequest) {
	defer sp.wg.Done()
	for {
		select {
		case <-sp.ctx.Done():
			return
		case req, ok := <-requestCh:
			if !ok {
				return
			}
			resp := sp.executeOperation(req)
			if resp != nil {
				sp.trySendResponse(resp)
			}
		}
	}
}

// handleDataTransfer handles a data stream request (read or write) from the server.
// Each call runs in its own goroutine, bounded by dataStreamSem.
func (sp *StorageProvider) handleDataTransfer(ctx context.Context, req *pb.StartDataTransfer) {
	// Acquire semaphore.
	select {
	case sp.dataStreamSem <- struct{}{}:
		defer func() { <-sp.dataStreamSem }()
	case <-ctx.Done():
		return
	case <-time.After(sp.config.OperationTimeout):
		sp.sendDataTransferFailed(req.TransferId, "data stream semaphore timeout")
		return
	}

	switch req.Operation {
	case pb.DataTransferOperation_DATA_TRANSFER_OPERATION_READ_FILE:
		sp.handleReadFileTransfer(ctx, req)
	case pb.DataTransferOperation_DATA_TRANSFER_OPERATION_WRITE_FILE:
		sp.handleWriteFileTransfer(ctx, req)
	default:
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("unknown data transfer operation: %v", req.Operation))
	}
}

// handleReadFileTransfer opens a local file and streams its content to the server
// via the ReadFileStream client-streaming RPC.
func (sp *StorageProvider) handleReadFileTransfer(ctx context.Context, req *pb.StartDataTransfer) {
	const chunkSize = 64 * 1024 // 64KB per chunk

	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("path error: %v", err))
		return
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	fd, err := openatReadOnly(dirFd, name)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open file: %v", err))
		return
	}
	f := fdToFile(fd, req.Path)
	defer f.Close()

	stream, err := sp.client.ReadFileStream(sp.withAuth(ctx))
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open read stream: %v", err))
		return
	}

	// Send header.
	if err := stream.Send(&pb.ReadFileStreamRequest{
		Payload: &pb.ReadFileStreamRequest_Header{
			Header: &pb.ReadFileStreamHeader{
				TransferId:  req.TransferId,
				WorkspaceId: sp.config.WorkspaceID,
			},
		},
	}); err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("send header: %v", err))
		return
	}

	// Stream file data.
	buf := make([]byte, chunkSize)
	for {
		n, readErr := f.Read(buf)
		if n > 0 {
			if err := stream.Send(&pb.ReadFileStreamRequest{
				Payload: &pb.ReadFileStreamRequest_Data{Data: buf[:n]},
			}); err != nil {
				sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("send chunk: %v", err))
				return
			}
		}
		if readErr != nil {
			break // EOF or error
		}
	}

	if _, err := stream.CloseAndRecv(); err != nil {
		log.Printf("[StorageProvider] read stream close error: %v", err)
	}
}

// handleWriteFileTransfer receives data from the server's WriteFileStream
// server-streaming RPC and writes it to a local file.
func (sp *StorageProvider) handleWriteFileTransfer(ctx context.Context, req *pb.StartDataTransfer) {
	dirFd, name, err := sp.pathGuard.OpenParentDir(req.Path)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("path error: %v", err))
		return
	}
	defer closeFdIfNotRoot(dirFd, sp.pathGuard.rootFd)

	fd, err := openatCreateTrunc(dirFd, name)
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("create file: %v", err))
		return
	}
	f := fdToFile(fd, req.Path)

	// Track whether the transfer completed successfully to clean up partial files.
	completed := false
	defer func() {
		f.Close()
		if !completed {
			// Remove the partial file on failure to avoid leaving corrupt data.
			// Use Unlinkat relative to dirFd for safety (dirFd may already be closed
			// if it's rootFd, but Unlinkat on rootFd is fine since it stays open).
			_ = unix.Unlinkat(dirFd, name, 0)
		}
	}()

	stream, err := sp.client.WriteFileStream(sp.withAuth(ctx), &pb.WriteFileStreamRequest{
		TransferId:  req.TransferId,
		WorkspaceId: sp.config.WorkspaceID,
	})
	if err != nil {
		sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("open write stream: %v", err))
		return
	}

	for {
		resp, err := stream.Recv()
		if err != nil {
			sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("recv write data: %v", err))
			return
		}
		switch p := resp.Payload.(type) {
		case *pb.WriteFileStreamResponse_Data:
			if _, err := f.Write(p.Data); err != nil {
				sp.sendDataTransferFailed(req.TransferId, fmt.Sprintf("write data: %v", err))
				return
			}
		case *pb.WriteFileStreamResponse_Done:
			completed = true
			return // file closed via defer
		}
	}
}

// sendDataTransferFailed notifies the server that a data transfer failed.
func (sp *StorageProvider) sendDataTransferFailed(transferID string, reason string) {
	sp.trySendResponse(&pb.ClientMessage{
		Message: &pb.ClientMessage_DataTransferFailed{
			DataTransferFailed: &pb.DataTransferFailed{
				TransferId: transferID,
				Reason:     reason,
			},
		},
	})
}

// chanMutex implements a channel-based mutex with timeout support.
// A capacity-1 buffered channel acts as the lock token.
type chanMutex struct {
	ch chan struct{}
}

func newChanMutex() *chanMutex {
	ch := make(chan struct{}, 1)
	ch <- struct{}{} // initial state: unlocked
	return &chanMutex{ch: ch}
}

// acquireFileLock acquires a per-file lock with a 10-second timeout.
// Returns nil if the lock could not be acquired.
func (sp *StorageProvider) acquireFileLock(path string) *chanMutex {
	actual, _ := sp.fileLocks.LoadOrStore(path, newChanMutex())
	mu := actual.(*chanMutex)
	select {
	case <-mu.ch: // acquired
		return mu
	case <-time.After(sp.config.OperationTimeout):
		return nil // timeout
	}
}

// releaseFileLock releases a previously acquired per-file lock.
func (sp *StorageProvider) releaseFileLock(mu *chanMutex) {
	if mu != nil {
		mu.ch <- struct{}{} // release: put token back
	}
}
