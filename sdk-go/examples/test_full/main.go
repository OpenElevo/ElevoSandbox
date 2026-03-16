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
//	-token string   FUSE API token (optional, empty means no auth)
//	-apikey string  gRPC API key or JWT for authentication (optional)
//
// Example:
//
//	go run examples/test_full/main.go -server localhost:9090 -apikey "$JWT"
package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"time"

	workspace "github.com/OpenElevo/ElevoWorkspace/sdk-go"
)

var (
	serverAddr = flag.String("server", "localhost:9090", "gRPC server address")
	fuseToken  = flag.String("token", "", "FUSE API token (optional, empty means no auth)")
	apiKey     = flag.String("apikey", "", "gRPC API key or JWT for authentication (optional)")
)

func main() {
	flag.Parse()

	fmt.Println("=== Go SDK Test ===")
	fmt.Printf("Server: %s\n\n", *serverAddr)

	client, err := workspace.NewClient(*serverAddr, workspace.ClientOptions{
		APIKey:  *apiKey,
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
	testWorkspaceFiles(ctx, client, workspaceID)

	// Sandbox tests require Docker and a valid namespace (tenant).
	// Skip gracefully if sandbox creation fails.
	sandboxID = testSandbox(ctx, client, workspaceID)
	if sandboxID != "" {
		testCommand(ctx, client, sandboxID)
		testShell(ctx, client, sandboxID)
		testDirectoryListing(ctx, client, sandboxID)
	}

	// Use API key as FUSE token if no explicit FUSE token provided
	fuseTokenVal := *fuseToken
	if fuseTokenVal == "" {
		fuseTokenVal = *apiKey
	}
	testFuse(ctx, *serverAddr, fuseTokenVal, workspaceID)

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

func testWorkspaceFiles(ctx context.Context, client *workspace.Client, workspaceID string) {
	fmt.Println("2. Testing workspace file operations via gRPC...")

	// Write file
	fmt.Println("   Writing file...")
	err := client.Workspace.WriteFileString(ctx, workspaceID, "grpc_test.txt", "Hello from gRPC file API")
	if err != nil {
		log.Fatalf("Failed to write file: %v", err)
	}
	fmt.Println("   Write OK")

	// Read file
	fmt.Println("   Reading file...")
	content, err := client.Workspace.ReadFileString(ctx, workspaceID, "grpc_test.txt")
	if err != nil {
		log.Fatalf("Failed to read file: %v", err)
	}
	if content != "Hello from gRPC file API" {
		log.Fatalf("Content mismatch: expected %q, got %q", "Hello from gRPC file API", content)
	}
	fmt.Printf("   Content: %s\n", content)
	fmt.Println("   Content verified OK")

	// Mkdir
	fmt.Println("   Creating directory...")
	if err := client.Workspace.Mkdir(ctx, workspaceID, "test_dir"); err != nil {
		log.Fatalf("Failed to mkdir: %v", err)
	}
	fmt.Println("   Mkdir OK")

	// Write file in subdirectory
	if err := client.Workspace.WriteFileString(ctx, workspaceID, "test_dir/nested.txt", "nested content"); err != nil {
		log.Fatalf("Failed to write nested file: %v", err)
	}

	// List files
	fmt.Println("   Listing files...")
	files, err := client.Workspace.ListFiles(ctx, workspaceID, ".")
	if err != nil {
		log.Fatalf("Failed to list files: %v", err)
	}
	fmt.Printf("   Files (%d): ", len(files))
	for i, f := range files {
		if i > 0 {
			fmt.Print(", ")
		}
		fmt.Printf("%s(%s)", f.Name, f.Type)
	}
	fmt.Println()

	// File exists
	exists, err := client.Workspace.FileExists(ctx, workspaceID, "grpc_test.txt")
	if err != nil {
		log.Fatalf("Failed to check file exists: %v", err)
	}
	if !exists {
		log.Fatalf("File should exist but doesn't")
	}
	fmt.Println("   FileExists OK")

	// Delete file
	fmt.Println("   Deleting file...")
	if err := client.Workspace.DeleteFile(ctx, workspaceID, "test_dir/nested.txt", false); err != nil {
		log.Fatalf("Failed to delete file: %v", err)
	}
	exists, _ = client.Workspace.FileExists(ctx, workspaceID, "test_dir/nested.txt")
	if exists {
		log.Fatalf("File should be deleted but still exists")
	}
	fmt.Println("   Delete OK")

	fmt.Println("   File operations OK")
}

func testSandbox(ctx context.Context, client *workspace.Client, workspaceID string) string {
	fmt.Println("3. Creating sandbox...")
	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		WorkspaceID: workspaceID,
		Name:        "go-sdk-test-sandbox",
		Template:    "workspace-test:latest",
	})
	if err != nil {
		fmt.Printf("   Sandbox creation failed (expected if Docker not available): %v\n", err)
		fmt.Println("   Skipping sandbox-dependent tests (command, shell, directory listing)")
		return ""
	}
	fmt.Printf("   Created sandbox: %s (state: %s)\n\n", sandbox.ID, sandbox.State)
	return sandbox.ID
}

func testCommand(ctx context.Context, client *workspace.Client, sandboxID string) {
	fmt.Println("4. Running command...")
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
	fmt.Println("5. File operations via shell...")
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
	fmt.Println("6. Listing workspace directory...")
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
	fmt.Println("7. Testing FUSE mount...")

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
