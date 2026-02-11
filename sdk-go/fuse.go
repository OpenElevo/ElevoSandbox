package workspace

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sync"
	"time"
)

// Default version and download URL template
const (
	DefaultVersion    = "latest"
	GitHubReleaseURL  = "https://github.com/OpenElevo/ElevoSandbox/releases/download/%s/workspace-fuse-%s-%s"
	GitHubLatestURL   = "https://github.com/OpenElevo/ElevoSandbox/releases/latest/download/workspace-fuse-%s-%s"
)

// getPlatformInfo returns the current platform and architecture
func getPlatformInfo() (string, string, error) {
	// Normalize platform
	var plat string
	switch runtime.GOOS {
	case "darwin":
		plat = "darwin"
	case "linux":
		plat = "linux"
	default:
		return "", "", fmt.Errorf("unsupported platform: %s", runtime.GOOS)
	}

	// Normalize architecture
	var arch string
	switch runtime.GOARCH {
	case "amd64":
		arch = "amd64"
	case "arm64":
		arch = "arm64"
	default:
		return "", "", fmt.Errorf("unsupported architecture: %s", runtime.GOARCH)
	}

	return plat, arch, nil
}

// getBinDir returns the directory for storing workspace-fuse binary
func getBinDir() (string, error) {
	// Try ~/.elevo/bin first
	home, err := os.UserHomeDir()
	if err == nil {
		binDir := filepath.Join(home, ".elevo", "bin")
		if err := os.MkdirAll(binDir, 0755); err == nil {
			return binDir, nil
		}
	}

	// Fall back to /usr/local/bin
	usrLocal := "/usr/local/bin"
	if info, err := os.Stat(usrLocal); err == nil && info.IsDir() {
		// Check write access
		testFile := filepath.Join(usrLocal, ".write_test")
		if f, err := os.Create(testFile); err == nil {
			f.Close()
			os.Remove(testFile)
			return usrLocal, nil
		}
	}

	return "", fmt.Errorf("cannot find writable directory for workspace-fuse binary")
}

// downloadFromURL downloads a file from URL to destPath, returns true if successful
func downloadFromURL(downloadURL string, destPath string, proxy string) bool {
	client := &http.Client{
		Timeout: 60 * time.Second,
	}
	if proxy != "" {
		client.Transport = &http.Transport{
			Proxy: http.ProxyURL(mustParseURL(proxy)),
		}
	}

	resp, err := client.Get(downloadURL)
	if err != nil {
		return false
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return false
	}

	f, err := os.Create(destPath)
	if err != nil {
		return false
	}
	defer f.Close()

	_, err = io.Copy(f, resp.Body)
	return err == nil
}

// tryDownloadFromServer tries to download workspace-fuse binary from workspace server
// serverURL should be the HTTP API URL (e.g., http://localhost:8081), not the gRPC URL
func tryDownloadFromServer(serverURL string, destPath string, proxy string) bool {
	plat, arch, err := getPlatformInfo()
	if err != nil {
		return false
	}

	downloadURL := fmt.Sprintf("%s/api/v1/downloads/workspace-fuse/%s/%s", serverURL, plat, arch)
	return downloadFromURL(downloadURL, destPath, proxy)
}

// downloadBinary downloads workspace-fuse binary for current platform
//
// Download priority:
// 1. From workspace server (if serverURL provided and binary available)
// 2. From GitHub Releases (fallback)
func downloadBinary(version string, proxy string, serverURL string) (string, error) {
	plat, arch, err := getPlatformInfo()
	if err != nil {
		return "", err
	}

	binDir, err := getBinDir()
	if err != nil {
		return "", err
	}

	binPath := filepath.Join(binDir, "workspace-fuse")
	tempPath := binPath + ".tmp"

	downloaded := false

	// Try server first if URL provided
	if serverURL != "" {
		downloaded = tryDownloadFromServer(serverURL, tempPath, proxy)
	}

	// Fallback to GitHub
	if !downloaded {
		var downloadURL string
		if version == "latest" || version == "" {
			downloadURL = fmt.Sprintf(GitHubLatestURL, plat, arch)
		} else {
			downloadURL = fmt.Sprintf(GitHubReleaseURL, version, plat, arch)
		}

		if !downloadFromURL(downloadURL, tempPath, proxy) {
			return "", fmt.Errorf("failed to download workspace-fuse from both server and GitHub")
		}
	}

	// Make executable
	if err := os.Chmod(tempPath, 0755); err != nil {
		os.Remove(tempPath)
		return "", fmt.Errorf("failed to chmod: %w", err)
	}

	// Verify it's a valid executable
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, tempPath, "--version")
	if err := cmd.Run(); err != nil {
		os.Remove(tempPath)
		return "", fmt.Errorf("downloaded binary is not valid: %w", err)
	}

	// Move to final location
	if err := os.Rename(tempPath, binPath); err != nil {
		os.Remove(tempPath)
		return "", fmt.Errorf("failed to move binary: %w", err)
	}

	return binPath, nil
}

// EnsureBinary ensures workspace-fuse binary is available
func EnsureBinary(version string, forceDownload bool, proxy string, serverURL string) (string, error) {
	binDir, err := getBinDir()
	if err != nil && !forceDownload {
		return "", err
	}

	binPath := filepath.Join(binDir, "workspace-fuse")

	if !forceDownload {
		if _, err := os.Stat(binPath); err == nil {
			// Verify it works
			ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
			defer cancel()
			cmd := exec.CommandContext(ctx, binPath, "--version")
			if err := cmd.Run(); err == nil {
				return binPath, nil
			}
		}
	}

	return downloadBinary(version, proxy, serverURL)
}

func mustParseURL(rawURL string) *url.URL {
	u, _ := url.Parse(rawURL)
	return u
}

// FuseMountOptions contains options for FUSE mount
type FuseMountOptions struct {
	// Server is the gRPC server URL
	Server string
	// WorkspaceID is the workspace to mount
	WorkspaceID string
	// Token is the authentication token
	Token string
	// MountPoint is the local mount point (auto-created if not specified)
	MountPoint string
	// BinaryPath is the path to workspace-fuse binary
	BinaryPath string
	// CacheTTL is the metadata cache TTL in seconds (default: 5)
	CacheTTL int
	// ReadCacheSize is the read cache size in MB (default: 256)
	ReadCacheSize int
	// BlockSize is the block size for reads (default: 128KB)
	BlockSize int
	// Debug enables debug logging
	Debug bool
}

// FuseMount represents an active FUSE mount for a workspace
type FuseMount struct {
	server        string
	workspaceID   string
	token         string
	mountPoint    string
	binaryPath    string
	cacheTTL      int
	readCacheSize int
	blockSize     int
	debug         bool

	tempDir string
	cmd     *exec.Cmd
	mounted bool
	mu      sync.Mutex
}

// NewFuseMount creates a new FUSE mount
func NewFuseMount(opts FuseMountOptions) *FuseMount {
	cacheTTL := opts.CacheTTL
	if cacheTTL == 0 {
		cacheTTL = 5
	}

	readCacheSize := opts.ReadCacheSize
	if readCacheSize == 0 {
		readCacheSize = 256
	}

	blockSize := opts.BlockSize
	if blockSize == 0 {
		blockSize = 131072
	}

	return &FuseMount{
		server:        opts.Server,
		workspaceID:   opts.WorkspaceID,
		token:         opts.Token,
		mountPoint:    opts.MountPoint,
		binaryPath:    opts.BinaryPath,
		cacheTTL:      cacheTTL,
		readCacheSize: readCacheSize,
		blockSize:     blockSize,
		debug:         opts.Debug,
	}
}

// MountPoint returns the mount point path
func (m *FuseMount) MountPoint() string {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.mountPoint
}

// Path is an alias for MountPoint
func (m *FuseMount) Path() string {
	return m.MountPoint()
}

// IsMounted returns whether the mount is active
func (m *FuseMount) IsMounted() bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.mounted && m.cmd != nil && m.cmd.ProcessState == nil
}

// Mount mounts the workspace
func (m *FuseMount) Mount(ctx context.Context) (string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if m.mounted {
		return m.mountPoint, nil
	}

	// Ensure binary is available
	if m.binaryPath == "" {
		binPath, err := EnsureBinary(DefaultVersion, false, "", "")
		if err != nil {
			return "", fmt.Errorf("failed to ensure binary: %w", err)
		}
		m.binaryPath = binPath
	}

	// Create mount point if not specified
	if m.mountPoint == "" {
		tempDir, err := os.MkdirTemp("", "workspace_fuse_")
		if err != nil {
			return "", fmt.Errorf("failed to create temp directory: %w", err)
		}
		m.tempDir = tempDir
		m.mountPoint = tempDir
	} else {
		if err := os.MkdirAll(m.mountPoint, 0755); err != nil {
			return "", fmt.Errorf("failed to create mount point: %w", err)
		}
	}

	// Build command
	args := []string{
		"mount",
		"--server", m.server,
		"--workspace", m.workspaceID,
		"--target", m.mountPoint,
		"--foreground",
		"--cache-ttl", fmt.Sprintf("%d", m.cacheTTL),
		"--read-cache-size", fmt.Sprintf("%d", m.readCacheSize),
		"--block-size", fmt.Sprintf("%d", m.blockSize),
	}

	// Token is optional
	if m.token != "" {
		args = append(args, "--token", m.token)
	}

	if m.debug {
		args = append(args, "--debug")
	}

	// Start the FUSE process
	m.cmd = exec.Command(m.binaryPath, args...)
	m.cmd.Stdout = nil
	m.cmd.Stderr = nil

	if err := m.cmd.Start(); err != nil {
		m.cleanup()
		return "", fmt.Errorf("failed to start workspace-fuse: %w", err)
	}

	// Wait for mount to be ready
	timeout := 30 * time.Second
	if deadline, ok := ctx.Deadline(); ok {
		timeout = time.Until(deadline)
	}

	startTime := time.Now()
	for time.Since(startTime) < timeout {
		// Check if process died
		if m.cmd.ProcessState != nil {
			m.cleanup()
			return "", fmt.Errorf("workspace-fuse exited unexpectedly")
		}

		// Check if mount is ready
		if _, err := os.ReadDir(m.mountPoint); err == nil {
			m.mounted = true
			return m.mountPoint, nil
		}

		select {
		case <-ctx.Done():
			m.cleanup()
			return "", ctx.Err()
		case <-time.After(100 * time.Millisecond):
		}
	}

	// Timeout
	m.cleanup()
	return "", fmt.Errorf("timeout waiting for mount to be ready")
}

// Unmount unmounts the workspace
func (m *FuseMount) Unmount() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	if !m.mounted {
		return nil
	}

	m.cleanup()
	return nil
}

func (m *FuseMount) cleanup() {
	// Terminate the FUSE process
	if m.cmd != nil && m.cmd.Process != nil {
		m.cmd.Process.Signal(os.Interrupt)
		done := make(chan error, 1)
		go func() {
			done <- m.cmd.Wait()
		}()

		select {
		case <-done:
		case <-time.After(5 * time.Second):
			m.cmd.Process.Kill()
		}
		m.cmd = nil
	}

	// Try fusermount as fallback
	if m.mountPoint != "" {
		exec.Command("fusermount", "-u", m.mountPoint).Run()
	}

	m.mounted = false

	// Clean up temp directory
	if m.tempDir != "" {
		os.RemoveAll(m.tempDir)
		m.tempDir = ""
	}
}

// WithMount is a helper that mounts, executes a function, and unmounts
func (m *FuseMount) WithMount(ctx context.Context, fn func(mountPoint string) error) error {
	mountPoint, err := m.Mount(ctx)
	if err != nil {
		return err
	}
	defer m.Unmount()

	return fn(mountPoint)
}

// WriteFile writes content to a file in the mounted workspace
func (m *FuseMount) WriteFile(relativePath string, content []byte) error {
	if !m.IsMounted() {
		return fmt.Errorf("not mounted")
	}
	fullPath := filepath.Join(m.MountPoint(), relativePath)
	dir := filepath.Dir(fullPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return err
	}
	return os.WriteFile(fullPath, content, 0644)
}

// ReadFile reads content from a file in the mounted workspace
func (m *FuseMount) ReadFile(relativePath string) ([]byte, error) {
	if !m.IsMounted() {
		return nil, fmt.Errorf("not mounted")
	}
	fullPath := filepath.Join(m.MountPoint(), relativePath)
	return os.ReadFile(fullPath)
}

// FuseService provides FUSE mount functionality for workspaces
type FuseService struct {
	server        string
	defaultToken  string
	binaryVersion string
	proxy         string
	httpServer    string
	binaryPath    string
	mounts        map[string]*FuseMount
	mu            sync.Mutex
}

// NewFuseService creates a new FUSE service
// httpServer is optional - if empty, binary will be downloaded from GitHub
func NewFuseService(server string, defaultToken string, binaryVersion string, proxy string, httpServer string) *FuseService {
	if binaryVersion == "" {
		binaryVersion = DefaultVersion
	}
	return &FuseService{
		server:        server,
		defaultToken:  defaultToken,
		binaryVersion: binaryVersion,
		proxy:         proxy,
		httpServer:    httpServer,
		mounts:        make(map[string]*FuseMount),
	}
}

// FuseMountServiceOptions contains options for mounting via service
type FuseMountServiceOptions struct {
	// Token overrides the default token
	Token string
	// MountPoint is the local mount point
	MountPoint string
	// CacheTTL is the metadata cache TTL in seconds
	CacheTTL int
	// ReadCacheSize is the read cache size in MB
	ReadCacheSize int
	// BlockSize is the block size for reads
	BlockSize int
	// Debug enables debug logging
	Debug bool
}

// Mount creates a FUSE mount for a workspace
func (s *FuseService) Mount(workspaceID string, opts ...FuseMountServiceOptions) (*FuseMount, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	var opt FuseMountServiceOptions
	if len(opts) > 0 {
		opt = opts[0]
	}

	token := opt.Token
	if token == "" {
		token = s.defaultToken
	}
	// Token is now optional - server may not require authentication

	// Check if already mounted
	if existing, ok := s.mounts[workspaceID]; ok && existing.IsMounted() {
		return existing, nil
	}

	// Ensure binary
	if s.binaryPath == "" {
		binPath, err := EnsureBinary(s.binaryVersion, false, s.proxy, s.httpServer)
		if err != nil {
			return nil, fmt.Errorf("failed to ensure binary: %w", err)
		}
		s.binaryPath = binPath
	}

	mount := NewFuseMount(FuseMountOptions{
		Server:        s.server,
		WorkspaceID:   workspaceID,
		Token:         token,
		MountPoint:    opt.MountPoint,
		BinaryPath:    s.binaryPath,
		CacheTTL:      opt.CacheTTL,
		ReadCacheSize: opt.ReadCacheSize,
		BlockSize:     opt.BlockSize,
		Debug:         opt.Debug,
	})

	s.mounts[workspaceID] = mount
	return mount, nil
}

// Unmount unmounts a workspace
func (s *FuseService) Unmount(workspaceID string) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if mount, ok := s.mounts[workspaceID]; ok {
		mount.Unmount()
		delete(s.mounts, workspaceID)
	}
	return nil
}

// UnmountAll unmounts all workspaces
func (s *FuseService) UnmountAll() {
	s.mu.Lock()
	defer s.mu.Unlock()

	for _, mount := range s.mounts {
		mount.Unmount()
	}
	s.mounts = make(map[string]*FuseMount)
}

// ListMounts returns all active mounts
func (s *FuseService) ListMounts() map[string]string {
	s.mu.Lock()
	defer s.mu.Unlock()

	result := make(map[string]string)
	for wsID, mount := range s.mounts {
		if mount.IsMounted() {
			result[wsID] = mount.MountPoint()
		}
	}
	return result
}

// FuseIsAvailable checks if FUSE is available on this system
func FuseIsAvailable() bool {
	// Check for fusermount
	if _, err := exec.LookPath("fusermount"); err != nil {
		return false
	}

	// Check for /dev/fuse
	if _, err := os.Stat("/dev/fuse"); err != nil {
		return false
	}

	return true
}
