package workspace

import (
	"context"
	"io"

	pb "github.com/OpenElevo/ElevoSandbox/sdk-go/proto/workspace/v1"
)

// FileSystemService provides low-level filesystem operations via gRPC
// This is primarily used for FUSE mounting
type FileSystemService struct {
	client *Client
}

// DownloadBinary downloads a binary file from the server
func (f *FileSystemService) DownloadBinary(ctx context.Context, name, platform, arch string) ([]byte, error) {
	req := &pb.DownloadBinaryRequest{
		Name:     name,
		Platform: platform,
		Arch:     arch,
	}

	stream, err := f.client.fsClient.DownloadBinary(f.client.withAuth(ctx), req)
	if err != nil {
		return nil, convertGrpcError(err)
	}

	var data []byte
	for {
		resp, err := stream.Recv()
		if err == io.EOF {
			break
		}
		if err != nil {
			return nil, convertGrpcError(err)
		}
		data = append(data, resp.Chunk...)
	}

	return data, nil
}
