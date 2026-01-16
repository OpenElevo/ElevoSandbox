// Example: Concurrent Operations
//
// This example demonstrates running multiple sandboxes and commands concurrently.
//
// Run: go run concurrent/main.go

package main

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
)

func main() {
	client := workspace.NewClient("http://localhost:8080", workspace.ClientOptions{
		Timeout: 120 * time.Second,
	})

	ctx := context.Background()

	fmt.Println("=== Concurrent Operations Example ===\n")

	// Create multiple sandboxes concurrently
	numSandboxes := 3
	sandboxes := make([]*workspace.Sandbox, numSandboxes)
	var wg sync.WaitGroup
	var mu sync.Mutex

	fmt.Printf("1. Creating %d sandboxes concurrently...\n", numSandboxes)
	start := time.Now()

	for i := 0; i < numSandboxes; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()

			sandbox, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
				Template: "workspace-test:latest",
				Name:     fmt.Sprintf("concurrent-sandbox-%d", idx),
			})
			if err != nil {
				log.Printf("Failed to create sandbox %d: %v", idx, err)
				return
			}

			mu.Lock()
			sandboxes[idx] = sandbox
			mu.Unlock()

			fmt.Printf("   Sandbox %d created: %s\n", idx, sandbox.ID)
		}(i)
	}
	wg.Wait()

	elapsed := time.Since(start)
	fmt.Printf("   All sandboxes created in %v\n\n", elapsed)

	// Cleanup function
	defer func() {
		fmt.Println("\n4. Cleaning up sandboxes...")
		for i, s := range sandboxes {
			if s != nil {
				wg.Add(1)
				go func(idx int, sandbox *workspace.Sandbox) {
					defer wg.Done()
					client.Sandbox.Delete(ctx, sandbox.ID, true)
					fmt.Printf("   Deleted sandbox %d\n", idx)
				}(i, s)
			}
		}
		wg.Wait()
		fmt.Println("   Done!")
	}()

	// Run commands in all sandboxes concurrently
	fmt.Println("2. Running commands in all sandboxes concurrently...")
	start = time.Now()

	results := make([]*workspace.CommandResult, numSandboxes)
	for i, sandbox := range sandboxes {
		if sandbox == nil {
			continue
		}
		wg.Add(1)
		go func(idx int, s *workspace.Sandbox) {
			defer wg.Done()

			result, err := client.Process.Run(ctx, s.ID, "bash", &workspace.RunCommandOptions{
				Args: []string{"-c", fmt.Sprintf("echo 'Hello from sandbox %d' && sleep 1", idx)},
			})
			if err != nil {
				log.Printf("Failed to run command in sandbox %d: %v", idx, err)
				return
			}

			mu.Lock()
			results[idx] = result
			mu.Unlock()
		}(i, sandbox)
	}
	wg.Wait()

	elapsed = time.Since(start)
	fmt.Printf("   All commands completed in %v\n\n", elapsed)

	// Print results
	fmt.Println("3. Results:")
	for i, result := range results {
		if result != nil {
			fmt.Printf("   Sandbox %d: %s", i, result.Stdout)
		}
	}
}
