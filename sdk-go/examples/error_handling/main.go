// Example: Error Handling
//
// This example demonstrates proper error handling with the SDK.
//
// Run: go run error_handling/main.go

package main

import (
	"context"
	"fmt"
	"log"
	"time"

	workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

func main() {
	client := workspace.NewClient("http://localhost:8080", workspace.ClientOptions{
		Timeout: 30 * time.Second,
	})

	ctx := context.Background()

	fmt.Println("=== Error Handling Example ===")

	// 1. Handle not found error
	fmt.Println("1. Handling non-existent sandbox error...")
	_, err := client.Sandbox.Get(ctx, "non-existent-sandbox-id")
	if err != nil {
		if workspace.IsNotFound(err) {
			fmt.Println("   ✓ Correctly identified as NotFound error")
		} else if e, ok := err.(*workspace.Error); ok {
			fmt.Printf("   API Error [%d]: %s\n", e.StatusCode, e.Message)
		} else {
			fmt.Printf("   Unknown error: %v\n", err)
		}
	}
	fmt.Println()

	// 2. Create a sandbox for more tests
	fmt.Println("2. Creating sandbox for error tests...")
	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: "workspace-test:latest",
	})
	if err != nil {
		log.Fatalf("Failed to create sandbox: %v", err)
	}
	fmt.Printf("   Created: %s\n\n", sandbox.ID)

	defer func() {
		fmt.Println("\n6. Cleaning up...")
		client.Sandbox.Delete(ctx, sandbox.ID, true)
		fmt.Println("   Done!")
	}()

	// 3. Handle command failure
	fmt.Println("3. Handling command that returns non-zero exit code...")
	result, err := client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
		Args: []string{"-c", "exit 42"},
	})
	if err != nil {
		fmt.Printf("   Error: %v\n", err)
	} else {
		fmt.Printf("   Exit code: %d (expected: 42)\n", result.ExitCode)
		if result.ExitCode == 42 {
			fmt.Println("   ✓ Correctly captured non-zero exit code")
		}
	}
	fmt.Println()

	// 4. Handle process error with Exec helper
	fmt.Println("4. Using Exec helper (errors on non-zero exit)...")
	_, err = client.Process.Exec(ctx, sandbox.ID, "cat", "/nonexistent/file.txt")
	if err != nil {
		if e, ok := err.(*workspace.ProcessError); ok {
			fmt.Printf("   ✓ ProcessError caught: %s\n", e.Message)
		} else {
			fmt.Printf("   Error: %v\n", err)
		}
	}
	fmt.Println()

	// 5. Handle timeout with context
	fmt.Println("5. Handling timeout with context...")
	timeoutCtx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()

	_, err = client.Process.Run(timeoutCtx, sandbox.ID, "sleep", &workspace.RunCommandOptions{
		Args: []string{"10"},
	})
	if err != nil {
		if timeoutCtx.Err() == context.DeadlineExceeded {
			fmt.Println("   ✓ Context deadline exceeded (as expected)")
		} else {
			fmt.Printf("   Error: %v\n", err)
		}
	}
}
