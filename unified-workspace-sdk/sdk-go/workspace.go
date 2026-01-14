// Package workspace provides a Go SDK for the Unified Workspace service.
//
// The SDK provides access to sandbox management, process execution, PTY terminals,
// and filesystem operations through a simple, idiomatic Go API.
//
// Basic usage:
//
//	client := workspace.NewClient("http://localhost:8080")
//
//	// Create a sandbox
//	sandbox, err := client.Sandbox.Create(context.Background(), &workspace.CreateSandboxParams{
//	    Template: "workspace-test:latest",
//	})
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer client.Sandbox.Delete(context.Background(), sandbox.ID, true)
//
//	// Run a command
//	result, err := client.Process.Run(context.Background(), sandbox.ID, "echo", &workspace.RunCommandOptions{
//	    Args: []string{"Hello", "World"},
//	})
//	if err != nil {
//	    log.Fatal(err)
//	}
//	fmt.Println(result.Stdout)
package workspace

// Version is the SDK version
const Version = "0.1.0"
