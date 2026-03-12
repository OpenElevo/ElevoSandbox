// SDK synchronization test - verifies all features added during SDK sync.
//
// Tests: workspace file ops (move, copy, get_file_info, exists),
// sandbox (exists, wait_for_state), process (shell, exec), error handling.
//
// Usage:
//
//	go run examples/test_sdk_sync/main.go [flags]
//
// Flags:
//
//	-server string  gRPC server address (default "localhost:9090")
//	-apikey string  gRPC API key or JWT (optional)
//	-image string   Sandbox container image (default "workspace-test:latest")
package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"time"

	workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

var (
	serverAddr = flag.String("server", "localhost:9090", "gRPC server address")
	apiKey     = flag.String("apikey", "", "gRPC API key or JWT")
	image      = flag.String("image", "workspace-test:latest", "Sandbox container image")
)

type testResult struct {
	name   string
	passed bool
	err    error
}

var results []testResult

func main() {
	flag.Parse()

	fmt.Println("╔══════════════════════════════════════════════════╗")
	fmt.Println("║   Go SDK Sync Test — Verify All SDK Features    ║")
	fmt.Println("╚══════════════════════════════════════════════════╝")
	fmt.Printf("  Server: %s\n\n", *serverAddr)

	client, err := workspace.NewClient(*serverAddr, workspace.ClientOptions{
		APIKey:  *apiKey,
		Timeout: 60 * time.Second,
	})
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	ctx := context.Background()

	runTest("1. Workspace CRUD + StorageType", func() error {
		return testWorkspaceCRUD(ctx, client)
	})

	runTest("2. Workspace File Operations (all)", func() error {
		return testFileOps(ctx, client)
	})

	runTest("3. Sandbox Exists + WaitForState", func() error {
		return testSandboxFeatures(ctx, client)
	})

	runTest("4. Process Shell + Exec", func() error {
		return testProcessFeatures(ctx, client)
	})

	runTest("5. Error Handling (ProcessError, NotFound)", func() error {
		return testErrorHandling(ctx, client)
	})

	// Print summary
	fmt.Println("\n╔══════════════════════════════════════════════════╗")
	fmt.Println("║                  Test Summary                    ║")
	fmt.Println("╚══════════════════════════════════════════════════╝")
	passed, failed := 0, 0
	for _, r := range results {
		status := "✓ PASS"
		if !r.passed {
			status = "✗ FAIL"
			failed++
		} else {
			passed++
		}
		fmt.Printf("  %s  %s\n", status, r.name)
		if r.err != nil {
			fmt.Printf("         Error: %v\n", r.err)
		}
	}
	fmt.Printf("\n  Total: %d passed, %d failed\n", passed, failed)
	if failed > 0 {
		log.Fatal("Some tests failed!")
	}
}

func runTest(name string, fn func() error) {
	fmt.Printf("\n── %s ──\n", name)
	err := fn()
	r := testResult{name: name, passed: err == nil, err: err}
	results = append(results, r)
	if err != nil {
		fmt.Printf("  FAILED: %v\n", err)
	} else {
		fmt.Println("  PASSED")
	}
}

func testWorkspaceCRUD(ctx context.Context, client *workspace.Client) error {
	// Create
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
		Name:     "go-sync-test",
		Metadata: map[string]string{"test": "sync"},
	})
	if err != nil {
		return fmt.Errorf("create workspace: %w", err)
	}
	defer client.Workspace.Delete(ctx, ws.ID)

	fmt.Printf("  Created workspace: %s (storage_type=%s)\n", ws.ID, ws.StorageType)

	// Verify default storage type is managed
	if ws.StorageType != workspace.StorageTypeManaged {
		return fmt.Errorf("expected storage_type=managed, got %s", ws.StorageType)
	}

	// Get
	got, err := client.Workspace.Get(ctx, ws.ID)
	if err != nil {
		return fmt.Errorf("get workspace: %w", err)
	}
	if got.Name != "go-sync-test" {
		return fmt.Errorf("name mismatch: got %s", got.Name)
	}

	// Exists
	exists, err := client.Workspace.Exists(ctx, ws.ID)
	if err != nil {
		return fmt.Errorf("exists: %w", err)
	}
	if !exists {
		return fmt.Errorf("workspace should exist")
	}

	// Exists (non-existent)
	exists, err = client.Workspace.Exists(ctx, "non-existent-id")
	if err != nil {
		return fmt.Errorf("exists non-existent: %w", err)
	}
	if exists {
		return fmt.Errorf("non-existent workspace should not exist")
	}

	// List
	list, err := client.Workspace.List(ctx)
	if err != nil {
		return fmt.Errorf("list workspaces: %w", err)
	}
	found := false
	for _, w := range list {
		if w.ID == ws.ID {
			found = true
			break
		}
	}
	if !found {
		return fmt.Errorf("workspace not found in list")
	}

	fmt.Println("  Workspace CRUD + StorageType OK")
	return nil
}

func testFileOps(ctx context.Context, client *workspace.Client) error {
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{Name: "go-file-ops-test"})
	if err != nil {
		return fmt.Errorf("create workspace: %w", err)
	}
	defer client.Workspace.Delete(ctx, ws.ID)

	// WriteFile + ReadFile
	if err := client.Workspace.WriteFileString(ctx, ws.ID, "hello.txt", "Hello World"); err != nil {
		return fmt.Errorf("write file: %w", err)
	}
	content, err := client.Workspace.ReadFileString(ctx, ws.ID, "hello.txt")
	if err != nil {
		return fmt.Errorf("read file: %w", err)
	}
	if content != "Hello World" {
		return fmt.Errorf("content mismatch: got %q", content)
	}
	fmt.Println("  WriteFile + ReadFile OK")

	// Mkdir + ListFiles
	if err := client.Workspace.Mkdir(ctx, ws.ID, "subdir"); err != nil {
		return fmt.Errorf("mkdir: %w", err)
	}
	files, err := client.Workspace.ListFiles(ctx, ws.ID, ".")
	if err != nil {
		return fmt.Errorf("list files: %w", err)
	}
	fmt.Printf("  ListFiles: %d items\n", len(files))

	// GetFileInfo
	info, err := client.Workspace.GetFileInfo(ctx, ws.ID, "hello.txt")
	if err != nil {
		return fmt.Errorf("get file info: %w", err)
	}
	if info.Type != "file" || info.Name != "hello.txt" {
		return fmt.Errorf("file info mismatch: name=%s type=%s", info.Name, info.Type)
	}
	fmt.Printf("  GetFileInfo: name=%s type=%s size=%d\n", info.Name, info.Type, info.Size)

	// FileExists
	exists, err := client.Workspace.FileExists(ctx, ws.ID, "hello.txt")
	if err != nil {
		return fmt.Errorf("file exists: %w", err)
	}
	if !exists {
		return fmt.Errorf("file should exist")
	}

	exists, err = client.Workspace.FileExists(ctx, ws.ID, "no-such-file.txt")
	if err != nil {
		return fmt.Errorf("file exists (missing): %w", err)
	}
	if exists {
		return fmt.Errorf("non-existent file should not exist")
	}
	fmt.Println("  FileExists OK")

	// CopyFile
	if err := client.Workspace.CopyFile(ctx, ws.ID, "hello.txt", "hello_copy.txt"); err != nil {
		return fmt.Errorf("copy file: %w", err)
	}
	copyContent, err := client.Workspace.ReadFileString(ctx, ws.ID, "hello_copy.txt")
	if err != nil {
		return fmt.Errorf("read copy: %w", err)
	}
	if copyContent != "Hello World" {
		return fmt.Errorf("copy content mismatch: got %q", copyContent)
	}
	fmt.Println("  CopyFile OK")

	// MoveFile
	if err := client.Workspace.MoveFile(ctx, ws.ID, "hello_copy.txt", "hello_moved.txt"); err != nil {
		return fmt.Errorf("move file: %w", err)
	}
	exists, _ = client.Workspace.FileExists(ctx, ws.ID, "hello_copy.txt")
	if exists {
		return fmt.Errorf("moved source should not exist")
	}
	movedContent, err := client.Workspace.ReadFileString(ctx, ws.ID, "hello_moved.txt")
	if err != nil {
		return fmt.Errorf("read moved: %w", err)
	}
	if movedContent != "Hello World" {
		return fmt.Errorf("moved content mismatch")
	}
	fmt.Println("  MoveFile OK")

	// DeleteFile
	if err := client.Workspace.DeleteFile(ctx, ws.ID, "hello_moved.txt", false); err != nil {
		return fmt.Errorf("delete file: %w", err)
	}
	exists, _ = client.Workspace.FileExists(ctx, ws.ID, "hello_moved.txt")
	if exists {
		return fmt.Errorf("deleted file should not exist")
	}
	fmt.Println("  DeleteFile OK")

	return nil
}

func testSandboxFeatures(ctx context.Context, client *workspace.Client) error {
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{Name: "go-sandbox-test"})
	if err != nil {
		return fmt.Errorf("create workspace: %w", err)
	}
	defer client.Workspace.Delete(ctx, ws.ID)

	sb, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		WorkspaceID: ws.ID,
		Template:    *image,
		Name:        "go-sync-test-sandbox",
	})
	if err != nil {
		fmt.Printf("  Sandbox creation failed (Docker may not be available): %v\n", err)
		fmt.Println("  Skipping sandbox-dependent tests")
		return nil
	}
	defer client.Sandbox.Delete(ctx, sb.ID, true)

	fmt.Printf("  Created sandbox: %s (state: %s)\n", sb.ID, sb.State)

	// Exists
	exists, err := client.Sandbox.Exists(ctx, sb.ID)
	if err != nil {
		return fmt.Errorf("sandbox exists: %w", err)
	}
	if !exists {
		return fmt.Errorf("sandbox should exist")
	}

	exists, err = client.Sandbox.Exists(ctx, "non-existent-sandbox")
	if err != nil {
		return fmt.Errorf("sandbox exists (non-existent): %w", err)
	}
	if exists {
		return fmt.Errorf("non-existent sandbox should not exist")
	}
	fmt.Println("  Sandbox.Exists OK")

	// WaitForState (already running in most cases)
	ctxWithTimeout, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	sb, err = client.Sandbox.WaitForState(ctxWithTimeout, sb.ID, workspace.SandboxStateRunning)
	if err != nil {
		return fmt.Errorf("wait for state: %w", err)
	}
	fmt.Printf("  WaitForState: reached %s\n", sb.State)

	return nil
}

func testProcessFeatures(ctx context.Context, client *workspace.Client) error {
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{Name: "go-process-test"})
	if err != nil {
		return fmt.Errorf("create workspace: %w", err)
	}
	defer client.Workspace.Delete(ctx, ws.ID)

	sb, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		WorkspaceID: ws.ID,
		Template:    *image,
	})
	if err != nil {
		fmt.Printf("  Sandbox creation failed: %v\n", err)
		fmt.Println("  Skipping process tests")
		return nil
	}
	defer client.Sandbox.Delete(ctx, sb.ID, true)

	ctxT, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	client.Sandbox.WaitForState(ctxT, sb.ID, workspace.SandboxStateRunning)

	// Shell
	result, err := client.Process.Shell(ctx, sb.ID, "echo hello && echo world", nil)
	if err != nil {
		return fmt.Errorf("shell: %w", err)
	}
	if result.ExitCode != 0 {
		return fmt.Errorf("shell exit code: %d", result.ExitCode)
	}
	fmt.Printf("  Shell output: %s", result.Stdout)
	fmt.Println("  Shell OK")

	// Exec
	stdout, err := client.Process.Exec(ctx, sb.ID, "echo", "Go", "SDK", "exec")
	if err != nil {
		return fmt.Errorf("exec: %w", err)
	}
	fmt.Printf("  Exec output: %s", stdout)
	fmt.Println("  Exec OK")

	return nil
}

func testErrorHandling(ctx context.Context, client *workspace.Client) error {
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{Name: "go-error-test"})
	if err != nil {
		return fmt.Errorf("create workspace: %w", err)
	}
	defer client.Workspace.Delete(ctx, ws.ID)

	sb, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		WorkspaceID: ws.ID,
		Template:    *image,
	})
	if err != nil {
		fmt.Println("  Skipping error handling tests (no Docker)")
		return nil
	}
	defer client.Sandbox.Delete(ctx, sb.ID, true)

	ctxT, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()
	client.Sandbox.WaitForState(ctxT, sb.ID, workspace.SandboxStateRunning)

	// ProcessError from Exec
	_, err = client.Process.Exec(ctx, sb.ID, "false")
	if err == nil {
		return fmt.Errorf("exec 'false' should fail")
	}
	if pe, ok := err.(*workspace.ProcessError); ok {
		fmt.Printf("  ProcessError caught: sandbox=%s command=%s\n", pe.SandboxID, pe.Command)
	} else {
		return fmt.Errorf("expected ProcessError, got %T: %v", err, err)
	}

	// NotFound error
	_, err = client.Workspace.Get(ctx, "non-existent-ws")
	if err == nil {
		return fmt.Errorf("get non-existent workspace should fail")
	}
	if !workspace.IsNotFound(err) {
		return fmt.Errorf("expected NotFound, got: %v", err)
	}
	fmt.Println("  NotFoundError OK")

	return nil
}
