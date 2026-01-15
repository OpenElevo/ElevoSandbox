package workspace

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strings"
)

// ProcessService provides operations for executing commands in sandboxes
type ProcessService struct {
	client *Client
}

// runRequest is the request body for running a command
type runRequest struct {
	Command string            `json:"command"`
	Args    []string          `json:"args,omitempty"`
	Env     map[string]string `json:"env,omitempty"`
	Cwd     string            `json:"cwd,omitempty"`
	Timeout int               `json:"timeout,omitempty"`
}

// Run executes a command and waits for it to complete
func (p *ProcessService) Run(ctx context.Context, sandboxID string, command string, opts *RunCommandOptions) (*CommandResult, error) {
	if opts == nil {
		opts = &RunCommandOptions{}
	}

	req := runRequest{
		Command: command,
		Args:    opts.Args,
		Env:     opts.Env,
		Cwd:     opts.Cwd,
		Timeout: opts.Timeout,
	}

	if req.Args == nil {
		req.Args = []string{}
	}
	if req.Env == nil {
		req.Env = map[string]string{}
	}

	var result struct {
		ExitCode int    `json:"exit_code"`
		Stdout   string `json:"stdout"`
		Stderr   string `json:"stderr"`
	}

	path := fmt.Sprintf("/sandboxes/%s/process/run", sandboxID)
	err := p.client.doRequest(ctx, http.MethodPost, path, req, &result)
	if err != nil {
		return nil, err
	}

	return &CommandResult{
		ExitCode: result.ExitCode,
		Stdout:   result.Stdout,
		Stderr:   result.Stderr,
	}, nil
}

// RunStream executes a command and returns a channel of events
func (p *ProcessService) RunStream(ctx context.Context, sandboxID string, command string, opts *RunCommandOptions) (<-chan ProcessEvent, <-chan error) {
	eventCh := make(chan ProcessEvent, 100)
	errCh := make(chan error, 1)

	go func() {
		defer close(eventCh)
		defer close(errCh)

		if opts == nil {
			opts = &RunCommandOptions{}
		}

		// Build URL with query parameters
		params := url.Values{}
		params.Set("command", command)
		if opts.Args != nil {
			argsJSON, _ := json.Marshal(opts.Args)
			params.Set("args", string(argsJSON))
		} else {
			params.Set("args", "[]")
		}
		if opts.Env != nil {
			envJSON, _ := json.Marshal(opts.Env)
			params.Set("env", string(envJSON))
		} else {
			params.Set("env", "{}")
		}
		if opts.Cwd != "" {
			params.Set("cwd", opts.Cwd)
		}
		if opts.Timeout > 0 {
			params.Set("timeout", fmt.Sprintf("%d", opts.Timeout))
		}

		streamURL := fmt.Sprintf("%s/api/v1/sandboxes/%s/process/run/stream?%s",
			p.client.apiURL, sandboxID, params.Encode())

		req, err := http.NewRequestWithContext(ctx, http.MethodGet, streamURL, nil)
		if err != nil {
			errCh <- fmt.Errorf("failed to create request: %w", err)
			return
		}

		req.Header.Set("Accept", "text/event-stream")
		if p.client.apiKey != "" {
			req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", p.client.apiKey))
		}

		resp, err := p.client.httpClient.Do(req)
		if err != nil {
			errCh <- &ConnectionError{URL: streamURL, Message: err.Error()}
			return
		}
		defer resp.Body.Close()

		if resp.StatusCode >= 400 {
			errCh <- &Error{
				StatusCode: resp.StatusCode,
				Message:    fmt.Sprintf("stream request failed: %s", resp.Status),
			}
			return
		}

		scanner := bufio.NewScanner(resp.Body)
		for scanner.Scan() {
			select {
			case <-ctx.Done():
				return
			default:
			}

			line := scanner.Text()
			if !strings.HasPrefix(line, "data: ") {
				continue
			}

			data := strings.TrimPrefix(line, "data: ")
			var event struct {
				Type    string `json:"type"`
				Data    string `json:"data,omitempty"`
				Code    *int   `json:"code,omitempty"`
				Message string `json:"message,omitempty"`
			}

			if err := json.Unmarshal([]byte(data), &event); err != nil {
				continue
			}

			processEvent := ProcessEvent{
				Type:    ProcessEventType(event.Type),
				Data:    event.Data,
				Code:    event.Code,
				Message: event.Message,
			}

			select {
			case eventCh <- processEvent:
			case <-ctx.Done():
				return
			}

			// Exit event signals end of stream
			if event.Type == "exit" || event.Type == "error" {
				return
			}
		}

		if err := scanner.Err(); err != nil {
			errCh <- fmt.Errorf("scanner error: %w", err)
		}
	}()

	return eventCh, errCh
}

// Kill terminates a running process
func (p *ProcessService) Kill(ctx context.Context, sandboxID string, pid int, signal int) error {
	if signal == 0 {
		signal = 15 // SIGTERM
	}

	req := map[string]int{"signal": signal}
	path := fmt.Sprintf("/sandboxes/%s/process/%d/kill", sandboxID, pid)

	return p.client.doRequest(ctx, http.MethodPost, path, req, nil)
}

// Exec is a convenience method that runs a command and returns stdout
// It returns an error if the exit code is non-zero
func (p *ProcessService) Exec(ctx context.Context, sandboxID string, command string, args ...string) (string, error) {
	result, err := p.Run(ctx, sandboxID, command, &RunCommandOptions{Args: args})
	if err != nil {
		return "", err
	}

	if result.ExitCode != 0 {
		return "", &ProcessError{
			SandboxID: sandboxID,
			Command:   command,
			Message:   fmt.Sprintf("exit code %d: %s", result.ExitCode, result.Stderr),
		}
	}

	return result.Stdout, nil
}

// Shell runs a shell command using bash -c
func (p *ProcessService) Shell(ctx context.Context, sandboxID string, script string, env map[string]string) (*CommandResult, error) {
	return p.Run(ctx, sandboxID, "bash", &RunCommandOptions{
		Args: []string{"-c", script},
		Env:  env,
	})
}
