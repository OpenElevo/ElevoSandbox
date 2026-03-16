// grpc_helper is a CLI tool that uses the Go SDK to perform workspace operations
// via gRPC. It's designed to be called from shell test scripts.
//
// Usage:
//
//	go run ./tests/grpc_helper/main.go -server localhost:9090 <command> [args...]
//
// Commands:
//
//	create-workspace [name]            Create a workspace, prints workspace ID
//	delete-workspace <id>              Delete a workspace
//	get-workspace <id>                 Get workspace info (JSON)
//	list-workspaces                    List all workspaces (JSON)
//	write-file <ws_id> <path> <data>   Write data to a file in workspace
//	read-file <ws_id> <path>           Read a file from workspace, prints content
//	list-files <ws_id> <path>          List files in a directory (JSON)
//	mkdir <ws_id> <path>               Create a directory
//	delete-file <ws_id> <path>         Delete a file or directory
//	move-file <ws_id> <src> <dst>      Move/rename a file
//	copy-file <ws_id> <src> <dst>      Copy a file
//	file-exists <ws_id> <path>         Check if file exists (prints "true"/"false")
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"time"

	workspace "github.com/OpenElevo/ElevoWorkspace/sdk-go"
)

func main() {
	if len(os.Args) < 2 {
		usage()
		os.Exit(1)
	}

	// Parse flags manually to support: -server <addr> <command> [args...]
	server := "localhost:9090"
	apiKey := ""
	args := os.Args[1:]

	for len(args) > 0 {
		switch args[0] {
		case "-server":
			if len(args) < 2 {
				fatal("missing value for -server")
			}
			server = args[1]
			args = args[2:]
		case "-apikey":
			if len(args) < 2 {
				fatal("missing value for -apikey")
			}
			apiKey = args[1]
			args = args[2:]
		default:
			goto done
		}
	}
done:

	if len(args) == 0 {
		usage()
		os.Exit(1)
	}

	command := args[0]
	cmdArgs := args[1:]

	client, err := workspace.NewClient(server, workspace.ClientOptions{
		APIKey:  apiKey,
		Timeout: 30 * time.Second,
	})
	if err != nil {
		fatal("failed to connect: %v", err)
	}
	defer client.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	switch command {
	case "create-workspace":
		name := "test-workspace"
		if len(cmdArgs) > 0 {
			name = cmdArgs[0]
		}
		ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{
			Name: name,
		})
		if err != nil {
			fatal("create workspace: %v", err)
		}
		fmt.Println(ws.ID)

	case "delete-workspace":
		requireArgs(cmdArgs, 1, "delete-workspace <id>")
		if err := client.Workspace.Delete(ctx, cmdArgs[0]); err != nil {
			fatal("delete workspace: %v", err)
		}
		fmt.Println("ok")

	case "get-workspace":
		requireArgs(cmdArgs, 1, "get-workspace <id>")
		ws, err := client.Workspace.Get(ctx, cmdArgs[0])
		if err != nil {
			fatal("get workspace: %v", err)
		}
		printJSON(ws)

	case "list-workspaces":
		workspaces, err := client.Workspace.List(ctx)
		if err != nil {
			fatal("list workspaces: %v", err)
		}
		printJSON(workspaces)

	case "write-file":
		requireArgs(cmdArgs, 3, "write-file <ws_id> <path> <data>")
		data := cmdArgs[2]
		// Support reading from stdin with "-"
		if data == "-" {
			buf, err := readStdin()
			if err != nil {
				fatal("read stdin: %v", err)
			}
			data = string(buf)
		}
		if err := client.Workspace.WriteFile(ctx, cmdArgs[0], cmdArgs[1], []byte(data)); err != nil {
			fatal("write file: %v", err)
		}
		fmt.Println("ok")

	case "read-file":
		requireArgs(cmdArgs, 2, "read-file <ws_id> <path>")
		content, err := client.Workspace.ReadFile(ctx, cmdArgs[0], cmdArgs[1])
		if err != nil {
			fatal("read file: %v", err)
		}
		os.Stdout.Write(content)

	case "list-files":
		requireArgs(cmdArgs, 2, "list-files <ws_id> <path>")
		files, err := client.Workspace.ListFiles(ctx, cmdArgs[0], cmdArgs[1])
		if err != nil {
			fatal("list files: %v", err)
		}
		printJSON(files)

	case "mkdir":
		requireArgs(cmdArgs, 2, "mkdir <ws_id> <path>")
		if err := client.Workspace.Mkdir(ctx, cmdArgs[0], cmdArgs[1]); err != nil {
			fatal("mkdir: %v", err)
		}
		fmt.Println("ok")

	case "delete-file":
		requireArgs(cmdArgs, 2, "delete-file <ws_id> <path>")
		if err := client.Workspace.DeleteFile(ctx, cmdArgs[0], cmdArgs[1], true); err != nil {
			fatal("delete file: %v", err)
		}
		fmt.Println("ok")

	case "move-file":
		requireArgs(cmdArgs, 3, "move-file <ws_id> <src> <dst>")
		if err := client.Workspace.MoveFile(ctx, cmdArgs[0], cmdArgs[1], cmdArgs[2]); err != nil {
			fatal("move file: %v", err)
		}
		fmt.Println("ok")

	case "copy-file":
		requireArgs(cmdArgs, 3, "copy-file <ws_id> <src> <dst>")
		if err := client.Workspace.CopyFile(ctx, cmdArgs[0], cmdArgs[1], cmdArgs[2]); err != nil {
			fatal("copy file: %v", err)
		}
		fmt.Println("ok")

	case "file-exists":
		requireArgs(cmdArgs, 2, "file-exists <ws_id> <path>")
		exists, err := client.Workspace.FileExists(ctx, cmdArgs[0], cmdArgs[1])
		if err != nil {
			fatal("file exists: %v", err)
		}
		if exists {
			fmt.Println("true")
		} else {
			fmt.Println("false")
		}

	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", command)
		usage()
		os.Exit(1)
	}
}

func usage() {
	cmds := []string{
		"create-workspace [name]",
		"delete-workspace <id>",
		"get-workspace <id>",
		"list-workspaces",
		"write-file <ws_id> <path> <data>",
		"read-file <ws_id> <path>",
		"list-files <ws_id> <path>",
		"mkdir <ws_id> <path>",
		"delete-file <ws_id> <path>",
		"move-file <ws_id> <src> <dst>",
		"copy-file <ws_id> <src> <dst>",
		"file-exists <ws_id> <path>",
	}
	fmt.Fprintf(os.Stderr, "Usage: grpc_helper [-server addr] [-apikey key] <command> [args...]\n\nCommands:\n")
	for _, c := range cmds {
		fmt.Fprintf(os.Stderr, "  %s\n", c)
	}
}

func requireArgs(args []string, n int, usage string) {
	if len(args) < n {
		fatal("usage: %s", usage)
	}
}

func fatal(format string, args ...interface{}) {
	fmt.Fprintf(os.Stderr, "ERROR: "+format+"\n", args...)
	os.Exit(1)
}

func printJSON(v interface{}) {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		fatal("json marshal: %v", err)
	}
	fmt.Println(string(data))
}

func readStdin() ([]byte, error) {
	var buf strings.Builder
	tmp := make([]byte, 4096)
	for {
		n, err := os.Stdin.Read(tmp)
		if n > 0 {
			buf.Write(tmp[:n])
		}
		if err != nil {
			break
		}
	}
	return []byte(buf.String()), nil
}
