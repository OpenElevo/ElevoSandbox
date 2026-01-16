// Package workspace provides a Go SDK for the Elevo Workspace service.
//
// The SDK provides access to workspace management, sandbox management, process execution,
// PTY terminals, and filesystem operations through a simple, idiomatic Go API.
//
// Basic usage:
//
//	client := workspace.NewClient("http://localhost:8080")
//
//	// Create a workspace
//	ws, err := client.Workspace.Create(context.Background(), &workspace.CreateWorkspaceParams{
//	    Name: "my-workspace",
//	})
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer client.Workspace.Delete(context.Background(), ws.ID)
//
//	// Create a sandbox bound to the workspace
//	sandbox, err := client.Sandbox.Create(context.Background(), &workspace.CreateSandboxParams{
//	    WorkspaceID: ws.ID,
//	    Template:    "workspace-test:latest",
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
