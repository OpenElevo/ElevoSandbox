package workspace_test

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	workspace "git.easyops.local/elevo/elevo-workspace/sdk-go"
)

const (
	testAPIURL = "http://localhost:8080"
	testImage  = "workspace-test:latest"
)

func newTestClient() *workspace.Client {
	return workspace.NewClient(testAPIURL, workspace.ClientOptions{
		Timeout: 60 * time.Second,
	})
}

// Test 1: Sandbox Lifecycle
func TestSandboxLifecycle(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	// Create
	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox: %v", err)
	}
	t.Logf("Created sandbox: %s (state: %s)", sandbox.ID, sandbox.State)

	if sandbox.State != workspace.SandboxStateRunning {
		t.Errorf("expected state running, got %s", sandbox.State)
	}

	// Get
	fetched, err := client.Sandbox.Get(ctx, sandbox.ID)
	if err != nil {
		t.Fatalf("failed to get sandbox: %v", err)
	}
	if fetched.ID != sandbox.ID {
		t.Errorf("expected ID %s, got %s", sandbox.ID, fetched.ID)
	}
	t.Logf("Got sandbox info: state=%s", fetched.State)

	// List
	sandboxes, err := client.Sandbox.List(ctx)
	if err != nil {
		t.Fatalf("failed to list sandboxes: %v", err)
	}
	found := false
	for _, s := range sandboxes {
		if s.ID == sandbox.ID {
			found = true
			break
		}
	}
	if !found {
		t.Error("sandbox not found in list")
	}
	t.Logf("Listed sandboxes: found %d total", len(sandboxes))

	// Delete
	err = client.Sandbox.Delete(ctx, sandbox.ID, true)
	if err != nil {
		t.Fatalf("failed to delete sandbox: %v", err)
	}
	t.Logf("Deleted sandbox: %s", sandbox.ID)
}

// Test 2: Process Execution
func TestProcessExecution(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandbox.ID, true)

	t.Run("Echo", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
			Args: []string{"Hello", "Go"},
		})
		if err != nil {
			t.Fatalf("failed to run echo: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		if !strings.Contains(result.Stdout, "Hello Go") {
			t.Errorf("expected stdout to contain 'Hello Go', got %s", result.Stdout)
		}
		t.Logf("Echo: stdout='%s'", strings.TrimSpace(result.Stdout))
	})

	t.Run("ListDir", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "ls", &workspace.RunCommandOptions{
			Args: []string{"-la", "/workspace"},
		})
		if err != nil {
			t.Fatalf("failed to run ls: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		t.Log("ls -la executed successfully")
	})

	t.Run("FailingCommand", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
			Args: []string{"-c", "exit 42"},
		})
		if err != nil {
			t.Fatalf("failed to run command: %v", err)
		}
		if result.ExitCode != 42 {
			t.Errorf("expected exit code 42, got %d", result.ExitCode)
		}
		t.Logf("Failing command returned correct exit code: %d", result.ExitCode)
	})

	t.Run("EnvVar", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
			Args: []string{"-c", "echo $GO_VAR"},
			Env:  map[string]string{"GO_VAR": "go_value"},
		})
		if err != nil {
			t.Fatalf("failed to run command: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		if !strings.Contains(result.Stdout, "go_value") {
			t.Errorf("expected stdout to contain 'go_value', got %s", result.Stdout)
		}
		t.Logf("Env var: stdout='%s'", strings.TrimSpace(result.Stdout))
	})

	t.Run("FileWriteRead", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
			Args: []string{"-c", "echo 'go content' > /workspace/go_test.txt && cat /workspace/go_test.txt"},
		})
		if err != nil {
			t.Fatalf("failed to run command: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		if !strings.Contains(result.Stdout, "go content") {
			t.Errorf("expected stdout to contain 'go content', got %s", result.Stdout)
		}
		t.Log("File write/read successful")
	})
}

// Test 3: Sandbox Isolation
func TestSandboxIsolation(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	sandboxA, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox A: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandboxA.ID, true)

	sandboxB, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox B: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandboxB.ID, true)

	// Write file in A
	_, err = client.Process.Run(ctx, sandboxA.ID, "bash", &workspace.RunCommandOptions{
		Args: []string{"-c", "echo 'secret_go' > /workspace/secret.txt"},
	})
	if err != nil {
		t.Fatalf("failed to write file in A: %v", err)
	}
	t.Logf("Created file in sandbox A: %s", sandboxA.ID)

	// Try to read from B
	result, err := client.Process.Run(ctx, sandboxB.ID, "cat", &workspace.RunCommandOptions{
		Args: []string{"/workspace/secret.txt"},
	})
	if err != nil {
		t.Fatalf("failed to run cat: %v", err)
	}

	if result.ExitCode == 0 {
		t.Error("Isolation broken: B can read A's files!")
	} else {
		t.Log("Sandbox isolation verified: B cannot read A's files")
	}
}

// Test 4: Long Running Command
func TestLongRunningCommand(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandbox.ID, true)

	start := time.Now()
	result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
		Args: []string{"-c", "sleep 3 && echo 'done'"},
	})
	elapsed := time.Since(start)

	if err != nil {
		t.Fatalf("failed to run command: %v", err)
	}

	if result.ExitCode != 0 {
		t.Errorf("expected exit code 0, got %d", result.ExitCode)
	}
	if !strings.Contains(result.Stdout, "done") {
		t.Errorf("expected stdout to contain 'done', got %s", result.Stdout)
	}
	if elapsed < 3*time.Second {
		t.Errorf("expected elapsed time >= 3s, got %v", elapsed)
	}
	t.Logf("Long running command completed in %v", elapsed)
}

// Test 5: Script Execution
func TestScriptExecution(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandbox.ID, true)

	t.Run("BashLoop", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
			Args: []string{"-c", "for i in a b c; do echo item_$i; done"},
		})
		if err != nil {
			t.Fatalf("failed to run script: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		if !strings.Contains(result.Stdout, "item_a") || !strings.Contains(result.Stdout, "item_c") {
			t.Errorf("expected output to contain 'item_a' and 'item_c', got %s", result.Stdout)
		}
		t.Log("Bash script executed with loop output")
	})

	t.Run("Pipe", func(t *testing.T) {
		result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
			Args: []string{"-c", "echo 'go sdk' | tr 'a-z' 'A-Z'"},
		})
		if err != nil {
			t.Fatalf("failed to run pipe: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		if !strings.Contains(result.Stdout, "GO SDK") {
			t.Errorf("expected output to contain 'GO SDK', got %s", result.Stdout)
		}
		t.Logf("Pipe command success: %s", strings.TrimSpace(result.Stdout))
	})
}

// Test 6: Concurrent Operations
func TestConcurrentOperations(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	// Create sandboxes concurrently
	var sandboxes [3]*workspace.Sandbox
	var wg sync.WaitGroup
	var mu sync.Mutex
	var createErr error

	start := time.Now()
	for i := 0; i < 3; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			s, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
				Template: testImage,
			})
			mu.Lock()
			defer mu.Unlock()
			if err != nil {
				createErr = err
				return
			}
			sandboxes[idx] = s
		}(i)
	}
	wg.Wait()
	createTime := time.Since(start)

	if createErr != nil {
		t.Fatalf("failed to create sandboxes: %v", createErr)
	}

	for i, s := range sandboxes {
		if s == nil {
			t.Fatalf("sandbox %d is nil", i)
		}
		defer client.Sandbox.Delete(ctx, s.ID, true)
	}
	t.Logf("Created 3 sandboxes concurrently in %v", createTime)

	// Run commands concurrently
	var results [3]*workspace.CommandResult
	start = time.Now()
	for i := 0; i < 3; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			r, err := client.Process.Run(ctx, sandboxes[idx].ID, "echo", &workspace.RunCommandOptions{
				Args: []string{fmt.Sprintf("sandbox%d", idx+1)},
			})
			mu.Lock()
			defer mu.Unlock()
			if err != nil {
				createErr = err
				return
			}
			results[idx] = r
		}(i)
	}
	wg.Wait()
	cmdTime := time.Since(start)

	if createErr != nil {
		t.Fatalf("failed to run commands: %v", createErr)
	}

	for i, r := range results {
		if r == nil || r.ExitCode != 0 {
			t.Errorf("command %d failed", i)
		}
	}
	t.Logf("Ran 3 commands concurrently in %v", cmdTime)
}

// Test 7: Error Handling
func TestErrorHandling(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	t.Run("NonExistentSandbox", func(t *testing.T) {
		_, err := client.Sandbox.Get(ctx, "non-existent-id")
		if err == nil {
			t.Error("expected error for non-existent sandbox")
		}
		if !workspace.IsNotFound(err) {
			t.Logf("Got error (expected not found): %v", err)
		} else {
			t.Log("Correct error for non-existent sandbox")
		}
	})

	t.Run("MissingFile", func(t *testing.T) {
		sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
			Template: testImage,
		})
		if err != nil {
			t.Fatalf("failed to create sandbox: %v", err)
		}
		defer client.Sandbox.Delete(ctx, sandbox.ID, true)

		result, err := client.Process.Run(ctx, sandbox.ID, "cat", &workspace.RunCommandOptions{
			Args: []string{"/nonexistent/file.txt"},
		})
		if err != nil {
			t.Fatalf("failed to run cat: %v", err)
		}

		if result.ExitCode == 0 {
			t.Error("expected non-zero exit code for missing file")
		} else {
			t.Logf("Correct error for missing file: exit_code=%d", result.ExitCode)
		}
	})
}

// Test 8: Sandbox Metadata
func TestSandboxMetadata(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	name := "test-sandbox-go"
	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
		Name:     name,
		Metadata: map[string]string{"purpose": "testing"},
	})
	if err != nil {
		t.Fatalf("failed to create sandbox: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandbox.ID, true)

	if sandbox.Name == nil || *sandbox.Name != name {
		t.Errorf("expected name %s, got %v", name, sandbox.Name)
	} else {
		t.Logf("Sandbox created with custom name: %s", *sandbox.Name)
	}

	// Test command-level env
	result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
		Args: []string{"-c", "echo $CMD_ENV"},
		Env:  map[string]string{"CMD_ENV": "go-cmd-value"},
	})
	if err != nil {
		t.Fatalf("failed to run command: %v", err)
	}

	if !strings.Contains(result.Stdout, "go-cmd-value") {
		t.Errorf("expected 'go-cmd-value' in output, got %s", result.Stdout)
	} else {
		t.Log("Command-level env works correctly")
	}
}

// Test 9: Rapid Operations
func TestRapidOperations(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	success := 0
	for i := 0; i < 5; i++ {
		sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
			Template: testImage,
		})
		if err != nil {
			t.Errorf("failed to create sandbox %d: %v", i, err)
			continue
		}

		if sandbox.State == workspace.SandboxStateRunning {
			err = client.Sandbox.Delete(ctx, sandbox.ID, true)
			if err != nil {
				t.Errorf("failed to delete sandbox %d: %v", i, err)
				continue
			}
			success++
		}
	}

	if success != 5 {
		t.Errorf("rapid create/delete failed: %d/5", success)
	} else {
		t.Logf("Rapid create/delete test passed (%d/5)", success)
	}
}

// Test helper functions
func TestHelperFunctions(t *testing.T) {
	client := newTestClient()
	ctx := context.Background()

	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: testImage,
	})
	if err != nil {
		t.Fatalf("failed to create sandbox: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sandbox.ID, true)

	t.Run("Exec", func(t *testing.T) {
		output, err := client.Process.Exec(ctx, sandbox.ID, "echo", "test", "output")
		if err != nil {
			t.Fatalf("Exec failed: %v", err)
		}
		if !strings.Contains(output, "test output") {
			t.Errorf("expected 'test output', got %s", output)
		}
		t.Logf("Exec output: %s", strings.TrimSpace(output))
	})

	t.Run("Shell", func(t *testing.T) {
		result, err := client.Process.Shell(ctx, sandbox.ID, "echo $SHELL_VAR | tr 'a-z' 'A-Z'", map[string]string{
			"SHELL_VAR": "hello",
		})
		if err != nil {
			t.Fatalf("Shell failed: %v", err)
		}
		if result.ExitCode != 0 {
			t.Errorf("expected exit code 0, got %d", result.ExitCode)
		}
		if !strings.Contains(result.Stdout, "HELLO") {
			t.Errorf("expected 'HELLO', got %s", result.Stdout)
		}
		t.Logf("Shell output: %s", strings.TrimSpace(result.Stdout))
	})

	t.Run("Exists", func(t *testing.T) {
		exists, err := client.Sandbox.Exists(ctx, sandbox.ID)
		if err != nil {
			t.Fatalf("Exists failed: %v", err)
		}
		if !exists {
			t.Error("expected sandbox to exist")
		}

		exists, err = client.Sandbox.Exists(ctx, "non-existent-id")
		if err != nil {
			t.Fatalf("Exists failed: %v", err)
		}
		if exists {
			t.Error("expected sandbox to not exist")
		}
		t.Log("Exists helper works correctly")
	})
}
