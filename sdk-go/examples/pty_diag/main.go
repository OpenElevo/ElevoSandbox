// PTY diagnostic test — keeps sandbox alive for manual log inspection.
package main

import (
	"context"
	"fmt"
	"io"
	"log"
	"os"
	"strings"
	"time"

	workspace "github.com/OpenElevo/ElevoSandbox/sdk-go"
	pb "github.com/OpenElevo/ElevoSandbox/sdk-go/proto/workspace/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

func main() {
	grpcAddr := "172.30.0.188:3201"
	image := "docker.easyops.local/elevo/workspace-base:latest"

	fmt.Println("=== PTY Diagnostic Test ===")

	client, err := workspace.NewClient(grpcAddr, workspace.ClientOptions{
		Timeout: 120 * time.Second,
	})
	if err != nil {
		log.Fatalf("Failed to create client: %v", err)
	}
	defer client.Close()

	ctx := context.Background()

	fmt.Println("[1] Creating workspace + sandbox...")
	ws, err := client.Workspace.Create(ctx, &workspace.CreateWorkspaceParams{Name: "pty-diag"})
	if err != nil {
		log.Fatalf("Create workspace: %v", err)
	}
	defer client.Workspace.Delete(ctx, ws.ID)

	sb, err := client.Sandbox.Create(ctx, &workspace.CreateSandboxParams{
		WorkspaceID: ws.ID, Template: image, Name: "pty-diag-sb",
	})
	if err != nil {
		log.Fatalf("Create sandbox: %v", err)
	}
	defer client.Sandbox.Delete(ctx, sb.ID, true)

	waitCtx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	sb, err = client.Sandbox.WaitForState(waitCtx, sb.ID, workspace.SandboxStateRunning)
	if err != nil {
		log.Fatalf("Wait for sandbox: %v", err)
	}
	fmt.Printf("  Sandbox: %s\n", sb.ID)

	// Print the container name pattern for log inspection
	fmt.Printf("  >> Check agent logs: ssh root@172.30.0.188 \"docker logs workspace-%s 2>&1\"\n",
		sb.ID[:8])

	fmt.Println("[2] Verify agent via Process.Shell...")
	result, err := client.Process.Shell(ctx, sb.ID, "echo agent_ok", nil)
	if err != nil {
		log.Fatalf("Process.Shell: %v", err)
	}
	fmt.Printf("  Agent works: %s", result.Stdout)

	fmt.Println("[3] Creating PTY...")
	handle, err := client.Pty.Create(ctx, sb.ID, &workspace.PtyOptions{
		Cols: 80, Rows: 24, Shell: "/bin/bash",
	})
	if err != nil {
		log.Fatalf("Create PTY: %v", err)
	}
	fmt.Printf("  PTY: %s\n", handle.ID)

	fmt.Println("[4] Waiting 3s for PTY creation on agent...")
	time.Sleep(3 * time.Second)

	// Print marker for log searching
	fmt.Printf("  >> Now check agent logs for PTY creation\n")

	fmt.Println("[5] Opening raw gRPC PtyStream...")
	conn, err := grpc.NewClient(grpcAddr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		log.Fatalf("gRPC dial: %v", err)
	}
	defer conn.Close()

	ptyClient := pb.NewPtyServiceClient(conn)
	stream, err := ptyClient.PtyStream(ctx)
	if err != nil {
		log.Fatalf("PtyStream: %v", err)
	}

	fmt.Printf("[6] Sending init...\n")
	err = stream.Send(&pb.PtyStreamRequest{
		Request: &pb.PtyStreamRequest_Init{
			Init: &pb.PtyStreamInit{SandboxId: sb.ID, PtyId: handle.ID},
		},
	})
	if err != nil {
		log.Fatalf("Send init: %v", err)
	}

	// Start receiver
	var output strings.Builder
	recvDone := make(chan bool, 1)
	go func() {
		for {
			resp, err := stream.Recv()
			if err == io.EOF {
				fmt.Println("  [recv] EOF")
				recvDone <- false
				return
			}
			if err != nil {
				fmt.Printf("  [recv] error: %v\n", err)
				recvDone <- false
				return
			}
			switch r := resp.Response.(type) {
			case *pb.PtyStreamResponse_Output:
				chunk := string(r.Output)
				output.WriteString(chunk)
				fmt.Printf("  [recv] output (%d bytes): %q\n", len(r.Output), chunk)
				if strings.Contains(output.String(), "DIAG_42") {
					recvDone <- true
					return
				}
			case *pb.PtyStreamResponse_ExitCode:
				fmt.Printf("  [recv] exit_code: %d\n", r.ExitCode)
				recvDone <- false
				return
			case *pb.PtyStreamResponse_Error:
				fmt.Printf("  [recv] error: %s\n", r.Error)
				recvDone <- false
				return
			}
		}
	}()

	time.Sleep(2 * time.Second)
	fmt.Println("[7] Sending input: echo DIAG_42")
	err = stream.Send(&pb.PtyStreamRequest{
		Request: &pb.PtyStreamRequest_Input{Input: []byte("echo DIAG_42\n")},
	})
	if err != nil {
		log.Fatalf("Send input: %v", err)
	}

	fmt.Println("[8] Waiting 15s for output...")
	select {
	case found := <-recvDone:
		if found {
			fmt.Println("\n[RESULT] PTY streaming WORKS!")
			stream.CloseSend()
			os.Exit(0)
		} else {
			fmt.Printf("\n[RESULT] Stream ended. Output: %q\n", output.String())
		}
	case <-time.After(15 * time.Second):
		fmt.Printf("\n[RESULT] TIMEOUT. Output: %q\n", output.String())
	}

	stream.CloseSend()

	// Don't kill PTY yet - keep sandbox alive for log inspection
	fmt.Println("\n[9] Checking agent container logs now...")
	// We'll check logs via SSH
	os.Exit(1)
}
