package workspace

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"
)

// ClientOptions contains options for creating a client
type ClientOptions struct {
	// APIKey is the optional API key for authentication
	APIKey string
	// Timeout is the request timeout (default: 30s)
	Timeout time.Duration
	// HTTPClient is an optional custom HTTP client
	HTTPClient *http.Client
}

// Client is the main workspace client
type Client struct {
	apiURL     string
	apiKey     string
	httpClient *http.Client

	// Services
	Sandbox    *SandboxService
	Process    *ProcessService
	Pty        *PtyService
	FileSystem *FileSystemService
}

// NewClient creates a new workspace client
func NewClient(apiURL string, opts ...ClientOptions) *Client {
	var opt ClientOptions
	if len(opts) > 0 {
		opt = opts[0]
	}

	timeout := opt.Timeout
	if timeout == 0 {
		timeout = 30 * time.Second
	}

	httpClient := opt.HTTPClient
	if httpClient == nil {
		httpClient = &http.Client{
			Timeout: timeout,
		}
	}

	c := &Client{
		apiURL:     apiURL,
		apiKey:     opt.APIKey,
		httpClient: httpClient,
	}

	// Initialize services
	c.Sandbox = &SandboxService{client: c}
	c.Process = &ProcessService{client: c}
	c.Pty = &PtyService{client: c}
	c.FileSystem = &FileSystemService{client: c}

	return c
}

// doRequest performs an HTTP request
func (c *Client) doRequest(ctx context.Context, method, path string, body interface{}, result interface{}) error {
	url := fmt.Sprintf("%s/api/v1%s", c.apiURL, path)

	var bodyReader io.Reader
	if body != nil {
		jsonBody, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("failed to marshal request body: %w", err)
		}
		bodyReader = bytes.NewReader(jsonBody)
	}

	req, err := http.NewRequestWithContext(ctx, method, url, bodyReader)
	if err != nil {
		return fmt.Errorf("failed to create request: %w", err)
	}

	req.Header.Set("Content-Type", "application/json")
	if c.apiKey != "" {
		req.Header.Set("Authorization", fmt.Sprintf("Bearer %s", c.apiKey))
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return &ConnectionError{URL: url, Message: err.Error()}
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to read response body: %w", err)
	}

	if resp.StatusCode >= 400 {
		return c.parseErrorResponse(resp.StatusCode, respBody)
	}

	if result != nil && len(respBody) > 0 {
		if err := json.Unmarshal(respBody, result); err != nil {
			return fmt.Errorf("failed to unmarshal response: %w", err)
		}
	}

	return nil
}

// parseErrorResponse parses an error response from the API
func (c *Client) parseErrorResponse(statusCode int, body []byte) error {
	var errResp struct {
		Error   string `json:"error"`
		Message string `json:"message"`
		Details string `json:"details"`
	}

	if err := json.Unmarshal(body, &errResp); err != nil {
		return &Error{
			StatusCode: statusCode,
			Message:    string(body),
		}
	}

	msg := errResp.Error
	if msg == "" {
		msg = errResp.Message
	}
	if msg == "" {
		msg = http.StatusText(statusCode)
	}

	return &Error{
		StatusCode: statusCode,
		Message:    msg,
		Details:    errResp.Details,
	}
}

// Health checks if the server is healthy
func (c *Client) Health(ctx context.Context) error {
	var result map[string]interface{}
	return c.doRequest(ctx, http.MethodGet, "/health", nil, &result)
}
