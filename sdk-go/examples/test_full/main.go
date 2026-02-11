// Full SDK test for Elevo Workspace Go SDK.
//
// This script tests all major SDK functionality including:
// - Workspace CRUD
// - Sandbox management
// - Command execution
// - File operations
// - FUSE mounting
//
// Usage:
//
//	go run examples/test_full/main.go [flags]
//
// Flags:
//
//	-server string  gRPC server address (default "localhost:9090")
//	-token string   FUSE API token (default "test-token")
//
// Example:
//
//	go run examples/test_full/main.go -server localhost:9090
package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"time"

	workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

var (
	serverAddr = flag.String("server", "localhost:9090", "gRPC server address")
	fuseToken  = flag.String("token", "", "FUSE API token (optional, empty means no auth)")
)

func main() {
	flag.Parse()

	fmt.Println("=== Go SDK Test ===")
	fmt.Printf("Server: %s\n\n", *serverAddr)

	client, err := workspace.NewClient(*serverAddr, workspace.ClientOptions{
		Timeout: 60 * time.Second,
	})
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	ctx := context.Background()

	var workspaceID, sandboxID string

	defer func() {
		cleanup(ctx, client, sandboxID, workspaceID)
	}()

	// Run tests
	workspaceID = testWorkspace(ctx, client)
	sandboxID = testSandbox(ctx, client, workspaceID)
	testCommand(ctx, client, sandboxID)
	testShell(ctx, client, sandboxID)
	testDirectoryListing(ctx, client, sandboxID)
	testFuse(ctx, *serverAddr, *fuseToken, workspaceID)

	fmt.Println("\n=== All tests passed! ===")
}

func testWorkspace(ctx context.Context, client *workspace.Client) string {
	fmt.Println("1. Creating workspace...")
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
		Name: "go-sdk-test",
	})
	if err != nil {
		log.Fatalf("Failed to create workspace: %v", err)
	}
	fmt.Printf("   Created workspace: %s\n\n", ws.ID)
	return ws.ID
}

func testSandbox(ctx context.Context, client *workspace.Client, workspaceID string) string {
	fmt.Println("2. Creating sandbox...")
	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		WorkspaceID: workspaceID,
		Name:        "go-sdk-test-sandbox",
		Template:    "workspace-test:latest",
	})
	if err != nil {
		log.Fatalf("Failed to create sandbox: %v", err)
	}
	fmt.Printf("   Created sandbox: %s (state: %s)\n\n", sandbox.ID, sandbox.State)
	return sandbox.ID
}

func testCommand(ctx context.Context, client *workspace.Client, sandboxID string) {
	fmt.Println("3. Running command...")
	result, err := client.Process.Run(ctx, sandboxID, "echo", &workspace.RunCommandOptions{
		Args: []string{"Hello", "from", "Go", "SDK!"},
	})
	if err != nil {
		log.Fatalf("Failed to run command: %v", err)
	}
	fmt.Printf("   Output: %s", result.Stdout)
	fmt.Println("   OK")
}

func testShell(ctx context.Context, client *workspace.Client, sandboxID string) {
	fmt.Println("4. File operations via shell...")
	result, err := client.Process.Shell(ctx, sandboxID, `
		echo "Hello from Go SDK" > /workspace/test.txt
		cat /workspace/test.txt
	`, nil)
	if err != nil {
		log.Fatalf("Failed to run shell: %v", err)
	}
	fmt.Printf("   File content: %s", result.Stdout)
	fmt.Println("   OK")
}

func testDirectoryListing(ctx context.Context, client *workspace.Client, sandboxID string) {
	fmt.Println("5. Listing workspace directory...")
	result, err := client.Process.Run(ctx, sandboxID, "ls", &workspace.RunCommandOptions{
		Args: []string{"-la", "/workspace"},
	})
	if err != nil {
		log.Fatalf("Failed to list directory: %v", err)
	}
	fmt.Printf("   Directory listing:\n%s", result.Stdout)
	fmt.Println("   OK")
}

func testFuse(ctx context.Context, grpcAddr, token, workspaceID string) {
	fmt.Println("6. Testing FUSE mount...")

	if !workspace.FuseIsAvailable() {
		fmt.Println("   FUSE not available on this system, skipping...")
		return
	}

	fmt.Println("   Creating FUSE service...")
	fuseService := workspace.NewFuseService(grpcAddr, token, "", "", "")

	fmt.Println("   Mounting workspace...")
	mount, err := fuseService.Mount(workspaceID, workspace.FuseMountServiceOptions{
		Debug: false,
	})
	if err != nil {
		log.Fatalf("Failed to create mount: %v", err)
	}

	mountPoint, err := mount.Mount(ctx)
	if err != nil {
		log.Fatalf("Failed to mount: %v", err)
	}
	fmt.Printf("   Mounted at: %s\n", mountPoint)

	defer func() {
		fmt.Println("   Unmounting...")
		mount.Unmount()
		fmt.Println("   Unmounted OK")
	}()

	// Write file via FUSE
	fmt.Println("   Writing file via FUSE...")
	testContent := []byte("Hello from Go SDK via FUSE!")
	testFile := filepath.Join(mountPoint, "fuse_test.txt")
	if err := os.WriteFile(testFile, testContent, 0644); err != nil {
		log.Fatalf("Failed to write file: %v", err)
	}
	fmt.Println("   Write OK")

	// Read file via FUSE
	fmt.Println("   Reading file via FUSE...")
	content, err := os.ReadFile(testFile)
	if err != nil {
		log.Fatalf("Failed to read file: %v", err)
	}
	fmt.Printf("   Content: %s\n", string(content))

	// Verify content
	if string(content) != string(testContent) {
		log.Fatalf("Content mismatch: expected %q, got %q", testContent, content)
	}
	fmt.Println("   Content verified OK")

	// List directory via FUSE
	fmt.Println("   Listing directory via FUSE...")
	entries, err := os.ReadDir(mountPoint)
	if err != nil {
		log.Fatalf("Failed to list directory: %v", err)
	}
	fmt.Printf("   Files: ")
	for i, entry := range entries {
		if i > 0 {
			fmt.Print(", ")
		}
		fmt.Print(entry.Name())
	}
	fmt.Println()

	fmt.Println("   FUSE test OK")
}

func cleanup(ctx context.Context, client *workspace.Client, sandboxID, workspaceID string) {
	fmt.Println("\n--- Cleanup ---")

	if sandboxID != "" {
		fmt.Println("Deleting sandbox...")
		if err := client.Sandbox.Delete(ctx, sandboxID, true); err != nil {
			fmt.Printf("   Warning: %v\n", err)
		} else {
			fmt.Println("   OK")
		}
	}

	if workspaceID != "" {
		fmt.Println("Deleting workspace...")
		if err := client.Workspace.Delete(ctx, workspaceID); err != nil {
			fmt.Printf("   Warning: %v\n", err)
		} else {
			fmt.Println("   OK")
		}
	}
}
