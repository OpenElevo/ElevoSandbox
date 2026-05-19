package workspace

import (
	"bufio"
	"log"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/fsnotify/fsnotify"

	pb "github.com/OpenElevo/ElevoWorkspace/sdk-go/proto/workspace/v1"
)

// Default directories to ignore when watching for file changes.
var defaultIgnoreDirs = map[string]bool{
	".git":         true,
	"node_modules": true,
	"__pycache__":  true,
	"target":       true,
	"build":        true,
	".elevo":       true,
}

// fileWatcher monitors the shared directory for file changes and sends
// FileChangedNotification messages through the response channel.
//
// Event coalescing strategy:
//   - First event starts a 50ms timer and records the window start time.
//   - Subsequent events within the window accumulate (per-path dedup, last event wins).
//   - Timer does NOT reset on new events (fixed window, not sliding).
//   - If 200ms have elapsed since window start, flush immediately (max-latency cap).
//   - In degraded mode (inotify limit exceeded), polls every 5s with full_purge=true.
type fileWatcher struct {
	watcher    *fsnotify.Watcher
	rootDir    string
	responseCh chan<- *pb.ClientMessage

	// connDone shares the StorageProvider's per-connection done channel.
	// Closed when the current gRPC connection ends, unblocking trySend
	// so the fileWatcher does not deadlock on a full responseCh.
	connDone *atomic.Value

	// .elevoignore rules.
	ignoreRules []string

	// Event coalescing state.
	pendingEvents map[string]*pb.FileChangeEvent
	mu            sync.Mutex
	timer         *time.Timer
	windowStart   time.Time

	// inotify degraded mode flag.
	degraded atomic.Bool

	// Lifecycle.
	done chan struct{}
}

func newFileWatcher(rootDir string, responseCh chan<- *pb.ClientMessage, connDone *atomic.Value) (*fileWatcher, error) {
	watcher, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}

	fw := &fileWatcher{
		watcher:       watcher,
		rootDir:       rootDir,
		responseCh:    responseCh,
		connDone:      connDone,
		pendingEvents: make(map[string]*pb.FileChangeEvent),
		done:          make(chan struct{}),
	}

	// Load .elevoignore rules if present.
	fw.ignoreRules = loadElevoIgnore(rootDir)

	// Start the event loop immediately.
	go fw.eventLoop()

	// Add watches recursively in the background.
	// Fire-and-forget: scanning a large directory tree can take a long time;
	// the gRPC main loop must start immediately to handle server requests.
	// If the inotify limit is hit during walk, addWatchesRecursive switches to
	// degraded poll mode.
	go func() {
		if err := fw.addWatchesRecursive(rootDir); err != nil {
			log.Printf("[fileWatcher] addWatchesRecursive error: %v", err)
		}
	}()

	return fw, nil
}

// Close stops the file watcher.
func (fw *fileWatcher) Close() {
	close(fw.done)
	fw.watcher.Close()
}

// addWatchesRecursive walks the directory tree and adds fsnotify watches,
// skipping ignored directories.
func (fw *fileWatcher) addWatchesRecursive(root string) error {
	return filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // skip inaccessible paths
		}
		if !info.IsDir() {
			return nil
		}

		name := info.Name()
		if defaultIgnoreDirs[name] && path != root {
			return filepath.SkipDir
		}
		if fw.isIgnored(path) {
			return filepath.SkipDir
		}

		if err := fw.watcher.Add(path); err != nil {
			log.Printf("[fileWatcher] failed to watch %s: %v", path, err)
			// Check if we've hit the inotify limit.
			if isInotifyLimitError(err) {
				fw.degraded.Store(true)
				log.Printf("[fileWatcher] inotify limit reached, switching to degraded mode")
				go fw.degradedPollLoop()
				return filepath.SkipAll
			}
		}
		return nil
	})
}

// eventLoop processes fsnotify events.
func (fw *fileWatcher) eventLoop() {
	for {
		select {
		case <-fw.done:
			return
		case event, ok := <-fw.watcher.Events:
			if !ok {
				return
			}
			fw.handleEvent(event)
		case err, ok := <-fw.watcher.Errors:
			if !ok {
				return
			}
			log.Printf("[fileWatcher] error: %v", err)
		}
	}
}

// handleEvent processes a single fsnotify event, coalescing with pending events.
func (fw *fileWatcher) handleEvent(event fsnotify.Event) {
	if fw.degraded.Load() {
		return // degraded mode handles notifications via polling
	}

	relPath, err := filepath.Rel(fw.rootDir, event.Name)
	if err != nil {
		return
	}

	// Skip ignored paths.
	if fw.isIgnored(event.Name) {
		return
	}

	// Dynamically add watch for new directories.
	if event.Op&fsnotify.Create != 0 {
		if info, err := os.Lstat(event.Name); err == nil && info.IsDir() {
			if !defaultIgnoreDirs[info.Name()] {
				_ = fw.watcher.Add(event.Name)
			}
		}
	}

	changeType := mapFsnotifyOp(event.Op)

	var immediateFlush bool
	fw.mu.Lock()

	fw.pendingEvents[relPath] = &pb.FileChangeEvent{
		Path:      relPath,
		EventType: changeType,
	}

	if fw.timer == nil {
		// First event: start the coalescing window.
		fw.windowStart = time.Now()
		fw.timer = time.AfterFunc(50*time.Millisecond, fw.flush)
	} else if time.Since(fw.windowStart) >= 200*time.Millisecond {
		// Max latency exceeded: mark for immediate flush (outside lock).
		fw.timer.Stop()
		immediateFlush = true
	}
	// else: within window, let existing timer fire.
	fw.mu.Unlock()

	if immediateFlush {
		msg := fw.collectPendingEvents()
		if msg != nil {
			fw.trySend(msg)
		}
	}
}

// flush is called by time.AfterFunc (not under mutex).
func (fw *fileWatcher) flush() {
	msg := fw.collectPendingEvents()
	if msg != nil {
		fw.trySend(msg)
	}
}

// trySend sends a message to responseCh, aborting if the done channel is closed
// or the current gRPC connection has ended. Prevents goroutine leaks when the
// gRPC stream has ended.
func (fw *fileWatcher) trySend(msg *pb.ClientMessage) {
	ch, _ := fw.connDone.Load().(chan struct{})
	select {
	case fw.responseCh <- msg:
	case <-fw.done:
	case <-ch:
	}
}

// collectPendingEvents gathers accumulated events under the lock and returns
// the message to send. Returns nil if there are no pending events.
// Sending is done outside the lock to avoid deadlock when responseCh is full.
func (fw *fileWatcher) collectPendingEvents() *pb.ClientMessage {
	fw.mu.Lock()
	defer fw.mu.Unlock()

	if len(fw.pendingEvents) == 0 {
		fw.timer = nil
		return nil
	}

	events := make([]*pb.FileChangeEvent, 0, len(fw.pendingEvents))
	for _, e := range fw.pendingEvents {
		events = append(events, e)
	}
	fw.pendingEvents = make(map[string]*pb.FileChangeEvent)
	fw.timer = nil

	return &pb.ClientMessage{
		Message: &pb.ClientMessage_FileChanged{
			FileChanged: &pb.FileChangedNotification{Events: events},
		},
	}
}

// degradedPollLoop runs when inotify watches would exceed the limit.
// Sends a full_purge notification every 5 seconds so the server clears all caches.
func (fw *fileWatcher) degradedPollLoop() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for {
		select {
		case <-fw.done:
			return
		case <-ticker.C:
			fw.trySend(&pb.ClientMessage{
				Message: &pb.ClientMessage_FileChanged{
					FileChanged: &pb.FileChangedNotification{
						FullPurge: true,
					},
				},
			})
		}
	}
}

// mapFsnotifyOp converts fsnotify.Op to the proto FileChangeType.
func mapFsnotifyOp(op fsnotify.Op) pb.FileChangeType {
	switch {
	case op&fsnotify.Create != 0:
		return pb.FileChangeType_FILE_CHANGE_TYPE_CREATED
	case op&fsnotify.Remove != 0:
		return pb.FileChangeType_FILE_CHANGE_TYPE_DELETED
	case op&fsnotify.Rename != 0:
		return pb.FileChangeType_FILE_CHANGE_TYPE_RENAMED
	case op&fsnotify.Chmod != 0:
		return pb.FileChangeType_FILE_CHANGE_TYPE_ATTR_CHANGED
	case op&fsnotify.Write != 0:
		return pb.FileChangeType_FILE_CHANGE_TYPE_MODIFIED
	default:
		return pb.FileChangeType_FILE_CHANGE_TYPE_MODIFIED
	}
}

// isIgnored checks if a path should be ignored based on .elevoignore rules
// and default ignore directories.
func (fw *fileWatcher) isIgnored(absPath string) bool {
	relPath, err := filepath.Rel(fw.rootDir, absPath)
	if err != nil {
		return false
	}

	parts := strings.Split(relPath, string(filepath.Separator))
	for _, part := range parts {
		if defaultIgnoreDirs[part] {
			return true
		}
	}

	for _, rule := range fw.ignoreRules {
		matched, _ := filepath.Match(rule, filepath.Base(absPath))
		if matched {
			return true
		}
		// Also try matching the relative path.
		matched, _ = filepath.Match(rule, relPath)
		if matched {
			return true
		}
	}

	return false
}

// loadElevoIgnore reads .elevoignore from the root directory.
// Each line is treated as a filepath.Match pattern.
func loadElevoIgnore(rootDir string) []string {
	f, err := os.Open(filepath.Join(rootDir, ".elevoignore"))
	if err != nil {
		return nil
	}
	defer f.Close()

	var rules []string
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		rules = append(rules, line)
	}
	return rules
}

// isInotifyLimitError checks if an error is related to inotify watch limits.
func isInotifyLimitError(err error) bool {
	if err == nil {
		return false
	}
	msg := err.Error()
	return strings.Contains(msg, "no space left on device") ||
		strings.Contains(msg, "too many open files")
}
