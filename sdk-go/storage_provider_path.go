package workspace

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"golang.org/x/sys/unix"
)

// pathGuard provides openat-based path safety. It ensures all file operations
// stay within the shared root directory, preventing symlink-based path traversal.
//
// Security is enforced in two layers:
//  1. ValidatePath: fast string-level check to reject ".." components.
//  2. OpenParentDir: fd-based traversal using O_NOFOLLOW|O_DIRECTORY on every
//     path component. ELOOP from symlink resolution becomes a traversal denial.
type pathGuard struct {
	rootPath string
	rootFd   int
}

func newPathGuard(rootPath string) (*pathGuard, error) {
	absRoot, err := filepath.Abs(rootPath)
	if err != nil {
		return nil, fmt.Errorf("resolve root path: %w", err)
	}

	// Verify the root is a directory.
	info, err := os.Stat(absRoot)
	if err != nil {
		return nil, fmt.Errorf("stat root: %w", err)
	}
	if !info.IsDir() {
		return nil, fmt.Errorf("root path is not a directory: %s", absRoot)
	}

	// Open root with O_NOFOLLOW | O_DIRECTORY.
	fd, err := unix.Open(absRoot, unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
	if err != nil {
		return nil, fmt.Errorf("open root dir: %w", err)
	}

	return &pathGuard{rootPath: absRoot, rootFd: fd}, nil
}

// Close releases the root directory fd.
func (pg *pathGuard) Close() {
	if pg.rootFd >= 0 {
		unix.Close(pg.rootFd)
		pg.rootFd = -1
	}
}

// ValidatePath performs a fast string-level check to reject obvious path
// traversal attempts. This is Layer 1; OpenParentDir provides Layer 2.
func (pg *pathGuard) ValidatePath(relPath string) error {
	if relPath == "" {
		return nil // root of workspace
	}
	cleaned := filepath.Clean(relPath)
	if cleaned == ".." || strings.HasPrefix(cleaned, "../") || strings.Contains(cleaned, "/../") || strings.HasSuffix(cleaned, "/..") {
		return fmt.Errorf("path traversal denied: %s", relPath)
	}
	if filepath.IsAbs(cleaned) {
		return fmt.Errorf("absolute paths not allowed: %s", relPath)
	}
	return nil
}

// OpenParentDir walks the relative path component by component using openat
// with O_NOFOLLOW, returning the parent directory fd and the leaf file name.
//
// The caller MUST close the returned dirFd using closeFdIfNotRoot when done,
// unless dirFd equals pg.rootFd (file is directly in the root).
func (pg *pathGuard) OpenParentDir(relPath string) (dirFd int, fileName string, err error) {
	if err := pg.ValidatePath(relPath); err != nil {
		return -1, "", err
	}

	cleaned := filepath.Clean(relPath)

	// Handle empty/root path: "." → root directory itself.
	if cleaned == "." {
		return pg.rootFd, ".", nil
	}

	parts := strings.Split(cleaned, "/")

	// File directly in root directory.
	if len(parts) == 1 {
		return pg.rootFd, parts[0], nil
	}

	// Walk each intermediate directory with openat.
	currentFd := pg.rootFd
	for i := 0; i < len(parts)-1; i++ {
		nextFd, openErr := unix.Openat(
			currentFd,
			parts[i],
			unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_DIRECTORY|unix.O_CLOEXEC,
			0,
		)
		if openErr != nil {
			if currentFd != pg.rootFd {
				unix.Close(currentFd)
			}
			if openErr == unix.ELOOP {
				return -1, "", fmt.Errorf("path traversal denied (symlink): %s", relPath)
			}
			return -1, "", openErr
		}
		if currentFd != pg.rootFd {
			unix.Close(currentFd)
		}
		currentFd = nextFd
	}

	return currentFd, parts[len(parts)-1], nil
}

// closeFdIfNotRoot closes the fd only if it differs from rootFd.
// The root fd is shared across all operations and must never be closed mid-session.
func closeFdIfNotRoot(fd int, rootFd int) {
	if fd != rootFd && fd >= 0 {
		unix.Close(fd)
	}
}

// openatReadOnly opens a file relative to dirFd in read-only mode with O_NOFOLLOW.
func openatReadOnly(dirFd int, name string) (int, error) {
	return unix.Openat(dirFd, name, unix.O_RDONLY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
}

// openatCreateTrunc opens/creates a file relative to dirFd, truncating if it exists.
func openatCreateTrunc(dirFd int, name string) (int, error) {
	return unix.Openat(dirFd, name, unix.O_WRONLY|unix.O_CREAT|unix.O_TRUNC|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0644)
}

// fdToFile wraps a raw fd into an *os.File for higher-level I/O.
func fdToFile(fd int, name string) *os.File {
	return os.NewFile(uintptr(fd), name)
}
