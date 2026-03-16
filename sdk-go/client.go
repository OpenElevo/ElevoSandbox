package workspace

import (
	"context"
	"crypto/tls"
	"fmt"
	"time"

	pb "github.com/OpenElevo/ElevoWorkspace/sdk-go/proto/workspace/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"
	"google.golang.org/grpc/credentials/insecure"
	"google.golang.org/grpc/metadata"
)

// ClientOptions contains options for creating a client
type ClientOptions struct {
	// APIKey is the optional API key for authentication
	APIKey string
	// Timeout is the request timeout (default: 30s)
	Timeout time.Duration
	// TLS enables TLS for the gRPC connection
	TLS bool
	// TLSConfig is an optional custom TLS configuration
	TLSConfig *tls.Config
	// DialOptions are additional gRPC dial options
	DialOptions []grpc.DialOption
}

// Client is the main workspace client
type Client struct {
	conn   *grpc.ClientConn
	apiKey string

	// gRPC clients
	workspaceClient pb.WorkspaceServiceClient
	sandboxClient   pb.SandboxServiceClient
	processClient   pb.ProcessServiceClient
	ptyClient       pb.PtyServiceClient
	fsClient        pb.FileSystemServiceClient

	// Services (high-level wrappers)
	Workspace *WorkspaceService
	Sandbox   *SandboxService
	Process   *ProcessService
	Pty       *PtyService
	Fs        *FileSystemService
}

// NewClient creates a new workspace client
func NewClient(serverAddr string, opts ...ClientOptions) (*Client, error) {
	var opt ClientOptions
	if len(opts) > 0 {
		opt = opts[0]
	}

	// Build dial options
	dialOpts := []grpc.DialOption{}

	// Add TLS or insecure credentials
	if opt.TLS {
		tlsConfig := opt.TLSConfig
		if tlsConfig == nil {
			tlsConfig = &tls.Config{}
		}
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(credentials.NewTLS(tlsConfig)))
	} else {
		dialOpts = append(dialOpts, grpc.WithTransportCredentials(insecure.NewCredentials()))
	}

	// Add custom dial options
	dialOpts = append(dialOpts, opt.DialOptions...)

	// Connect to gRPC server
	conn, err := grpc.NewClient(serverAddr, dialOpts...)
	if err != nil {
		return nil, fmt.Errorf("failed to connect to server: %w", err)
	}

	c := &Client{
		conn:            conn,
		apiKey:          opt.APIKey,
		workspaceClient: pb.NewWorkspaceServiceClient(conn),
		sandboxClient:   pb.NewSandboxServiceClient(conn),
		processClient:   pb.NewProcessServiceClient(conn),
		ptyClient:       pb.NewPtyServiceClient(conn),
		fsClient:        pb.NewFileSystemServiceClient(conn),
	}

	// Initialize high-level services
	c.Workspace = &WorkspaceService{client: c}
	c.Sandbox = &SandboxService{client: c}
	c.Process = &ProcessService{client: c}
	c.Pty = &PtyService{client: c}
	c.Fs = &FileSystemService{client: c}

	return c, nil
}

// NewStorageProvider creates a new StorageProvider that shares a local directory
// through the existing gRPC connection.
func (c *Client) NewStorageProvider(config StorageProviderConfig) *StorageProvider {
	return NewStorageProvider(c.conn, config)
}

// Close closes the gRPC connection
func (c *Client) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

// withAuth adds authentication metadata to the context
func (c *Client) withAuth(ctx context.Context) context.Context {
	if c.apiKey != "" {
		return metadata.AppendToOutgoingContext(ctx, "authorization", "Bearer "+c.apiKey)
	}
	return ctx
}
