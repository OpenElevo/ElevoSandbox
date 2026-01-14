// Example: Basic Usage
//
// This example demonstrates basic sandbox and process operations.
//
// Run: go run basic/main.go

package main

import (
	"context"
	"fmt"
	"log"
	"time"

	workspace "github.com/anthropic/workspace-sdk-go"
)

func main() {
	// Create client with custom timeout
	client := workspace.NewClient("http://localhost:8080", workspace.ClientOptions{
		Timeout: 60 * time.Second,
	})

	ctx := context.Background()

	fmt.Println("=== Workspace SDK Basic Example ===\n")

	// 1. Create a sandbox
	fmt.Println("1. Creating sandbox...")
	sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		Template: "workspace-test:latest",
		Name:     "example-sandbox",
		Metadata: map[string]string{
			"purpose": "demo",
		},
	})
	if err != nil {
		log.Fatalf("Failed to create sandbox: %v", err)
	}
	fmt.Printf("   Created: %s (state: %s)\n\n", sandbox.ID, sandbox.State)

	// Ensure cleanup
	defer func() {
		fmt.Println("\n6. Cleaning up...")
		if err := client.Sandbox.Delete(ctx, sandbox.ID, true); err != nil {
			log.Printf("Warning: failed to delete sandbox: %v", err)
		}
		fmt.Println("   Done!")
	}()

	// 2. Run a simple command
	fmt.Println("2. Running echo command...")
	result, err := client.Process.Run(ctx, sandbox.ID, "echo", &workspace.RunCommandOptions{
		Args: []string{"Hello", "from", "Go", "SDK!"},
	})
	if err != nil {
		log.Fatalf("Failed to run command: %v", err)
	}
	fmt.Printf("   Output: %s\n", result.Stdout)

	// 3. Run command with environment variables
	fmt.Println("3. Running command with environment variables...")
	result, err = client.Process.Run(ctx, sandbox.ID, "bash", &workspace.RunCommandOptions{
		Args: []string{"-c", "echo \"User: $USER, App: $APP_NAME\""},
		Env: map[string]string{
			"USER":     "developer",
			"APP_NAME": "MyApp",
		},
	})
	if err != nil {
		log.Fatalf("Failed to run command: %v", err)
	}
	fmt.Printf("   Output: %s\n", result.Stdout)

	// 4. Write and read a file
	fmt.Println("4. Writing and reading a file...")
	result, err = client.Process.Shell(ctx, sandbox.ID, `
		echo '{"name": "test", "version": "1.0.0"}' > /workspace/config.json
		cat /workspace/config.json
	`, nil)
	if err != nil {
		log.Fatalf("Failed to run shell: %v", err)
	}
	fmt.Printf("   File content: %s\n", result.Stdout)

	// 5. List workspace directory
	fmt.Println("5. Listing workspace directory...")
	result, err = client.Process.Run(ctx, sandbox.ID, "ls", &workspace.RunCommandOptions{
		Args: []string{"-la", "/workspace"},
	})
	if err != nil {
		log.Fatalf("Failed to list directory: %v", err)
	}
	fmt.Printf("   Directory listing:\n%s\n", result.Stdout)
}
