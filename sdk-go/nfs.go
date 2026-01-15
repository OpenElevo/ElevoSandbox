package workspace

import (
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
)

// NfsMountOptions contains options for NFS mount
type NfsMountOptions struct {
	// Host is the NFS server host
	Host string
	// Port is the NFS server port (default: 2049)
	Port int
	// ExportPath is the export path on the server
	ExportPath string
	// MountPoint is the local mount point (auto-created if not specified)
	MountPoint string
	// Options are custom mount options (default: nfsvers=3,tcp,nolock,port={port},mountport={port})
	Options string
}

// NfsMount represents an active NFS mount
type NfsMount struct {
	host       string
	port       int
	exportPath string
	mountPoint string
	options    string
	mounted    bool
	tempDir    string
}

// NewNfsMount creates a new NFS mount
func NewNfsMount(opts NfsMountOptions) *NfsMount {
	port := opts.Port
	if port == 0 {
		port = 2049
	}

	options := opts.Options
	if options == "" {
		options = "nfsvers=3,tcp,nolock,port={port},mountport={port}"
	}

	return &NfsMount{
		host:       opts.Host,
		port:       port,
		exportPath: opts.ExportPath,
		mountPoint: opts.MountPoint,
		options:    options,
	}
}

// MountPoint returns the mount point path
func (m *NfsMount) MountPoint() string {
	return m.mountPoint
}

// IsMounted returns whether the mount is active
func (m *NfsMount) IsMounted() bool {
	return m.mounted
}

// Mount mounts the NFS share
func (m *NfsMount) Mount() (string, error) {
	if m.mounted {
		return m.mountPoint, nil
	}

	// Create mount point if not specified
	if m.mountPoint == "" {
		tempDir, err := os.MkdirTemp("", "workspace_nfs_")
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

	// Build mount options
	opts := strings.ReplaceAll(m.options, "{port}", strconv.Itoa(m.port))

	// Mount command
	cmd := exec.Command("mount", "-t", "nfs", "-o", opts,
		fmt.Sprintf("%s:%s", m.host, m.exportPath), m.mountPoint)

	output, err := cmd.CombinedOutput()
	if err != nil {
		if m.tempDir != "" {
			os.RemoveAll(m.tempDir)
			m.tempDir = ""
		}
		return "", fmt.Errorf("failed to mount NFS: %s", string(output))
	}

	m.mounted = true
	return m.mountPoint, nil
}

// Unmount unmounts the NFS share
func (m *NfsMount) Unmount() error {
	if !m.mounted {
		return nil
	}

	cmd := exec.Command("umount", m.mountPoint)
	if err := cmd.Run(); err != nil {
		// Try lazy unmount
		cmd = exec.Command("umount", "-l", m.mountPoint)
		cmd.Run() // Ignore errors
	}

	m.mounted = false

	if m.tempDir != "" {
		os.RemoveAll(m.tempDir)
		m.tempDir = ""
	}

	return nil
}

// NfsService provides NFS mount functionality for sandbox workspaces
type NfsService struct {
	defaultHost string
	defaultPort int
}

// NewNfsService creates a new NFS service
func NewNfsService(defaultHost string, defaultPort int) *NfsService {
	if defaultPort == 0 {
		defaultPort = 2049
	}
	return &NfsService{
		defaultHost: defaultHost,
		defaultPort: defaultPort,
	}
}

// MountOptions contains options for mounting a sandbox
type MountOptions struct {
	// MountPoint is the local mount point (auto-created if not specified)
	MountPoint string
	// Host overrides the default NFS host
	Host string
	// Port overrides the default NFS port
	Port int
}

// Mount creates an NFS mount for a sandbox
func (s *NfsService) Mount(sandboxID string, opts ...MountOptions) *NfsMount {
	var opt MountOptions
	if len(opts) > 0 {
		opt = opts[0]
	}

	host := opt.Host
	if host == "" {
		host = s.defaultHost
	}

	port := opt.Port
	if port == 0 {
		port = s.defaultPort
	}

	return NewNfsMount(NfsMountOptions{
		Host:       host,
		Port:       port,
		ExportPath: "/" + sandboxID,
		MountPoint: opt.MountPoint,
	})
}

// MountFromURL creates an NFS mount from a full NFS URL
func (s *NfsService) MountFromURL(nfsURL string, mountPoint string) (*NfsMount, error) {
	u, err := url.Parse(nfsURL)
	if err != nil {
		return nil, fmt.Errorf("invalid NFS URL: %w", err)
	}

	host := u.Hostname()
	port := s.defaultPort
	if u.Port() != "" {
		port, err = strconv.Atoi(u.Port())
		if err != nil {
			return nil, fmt.Errorf("invalid port in NFS URL: %w", err)
		}
	}

	return NewNfsMount(NfsMountOptions{
		Host:       host,
		Port:       port,
		ExportPath: u.Path,
		MountPoint: mountPoint,
	}), nil
}

// IsAvailable checks if NFS mount is available on this system
func NfsIsAvailable() bool {
	_, err := exec.LookPath("mount.nfs")
	return err == nil
}

// WithMount is a helper that mounts, executes a function, and unmounts
func (m *NfsMount) WithMount(fn func(mountPoint string) error) error {
	mountPoint, err := m.Mount()
	if err != nil {
		return err
	}
	defer m.Unmount()

	return fn(mountPoint)
}

// WriteFile writes content to a file in the mounted workspace
func (m *NfsMount) WriteFile(relativePath string, content []byte) error {
	if !m.mounted {
		return fmt.Errorf("not mounted")
	}
	fullPath := filepath.Join(m.mountPoint, relativePath)
	dir := filepath.Dir(fullPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return err
	}
	return os.WriteFile(fullPath, content, 0644)
}

// ReadFile reads content from a file in the mounted workspace
func (m *NfsMount) ReadFile(relativePath string) ([]byte, error) {
	if !m.mounted {
		return nil, fmt.Errorf("not mounted")
	}
	fullPath := filepath.Join(m.mountPoint, relativePath)
	return os.ReadFile(fullPath)
}
