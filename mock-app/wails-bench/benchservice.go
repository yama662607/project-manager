package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"

	"github.com/wailsapp/wails/v3/pkg/application"
)

const appName = "wails"

type Project struct {
	ID           string   `json:"id"`
	Name         string   `json:"name"`
	Path         string   `json:"path"`
	Tags         []string `json:"tags"`
	Aliases      []string `json:"aliases"`
	Language     string   `json:"language"`
	LastOpenedAt string   `json:"lastOpenedAt"`
}

type BenchLogger struct {
	start time.Time
	mu    sync.Mutex
	file  *os.File
}

func NewBenchLogger() *BenchLogger {
	dir, err := os.UserHomeDir()
	if err != nil {
		dir = "."
	}
	dir = filepath.Join(dir, "Library", "Logs", "ProjectLauncherBench")
	_ = os.MkdirAll(dir, 0o755)
	file, err := os.OpenFile(filepath.Join(dir, fmt.Sprintf("wails-%d.jsonl", os.Getpid())), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		file = os.Stderr
	}
	return &BenchLogger{start: time.Now(), file: file}
}

func (l *BenchLogger) Log(event string, cycleID string, fields map[string]any) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if fields == nil {
		fields = map[string]any{}
	}
	fields["app"] = appName
	fields["event"] = event
	fields["mono_ns"] = time.Since(l.start).Nanoseconds()
	fields["wall_ms"] = time.Now().UnixMilli()
	if cycleID != "" {
		fields["cycle_id"] = cycleID
	}
	data, err := json.Marshal(fields)
	if err == nil {
		_, _ = l.file.Write(append(data, '\n'))
	}
}

type BenchService struct {
	app      *application.App
	window   application.Window
	projects []Project
	logger   *BenchLogger
}

func NewBenchService() *BenchService {
	return &BenchService{
		projects: loadProjects(),
		logger:   NewBenchLogger(),
	}
}

func (s *BenchService) setRuntime(app *application.App, window application.Window) {
	s.app = app
	s.window = window
	s.logger.Log("app_ready", "", map[string]any{"project_count": len(s.projects)})
}

func (s *BenchService) LoadProjects() []Project {
	return s.projects
}

func (s *BenchService) LogEvent(event string, cycleID string, fields map[string]any) {
	s.logger.Log(event, cycleID, fields)
}

func (s *BenchService) LogMetric(event string, cycleID string, metric string, durationMs float64, query string, resultCount int) {
	s.logger.Log(event, cycleID, map[string]any{
		"metric":       metric,
		"duration_ms":  durationMs,
		"query":        query,
		"result_count": resultCount,
	})
}

func (s *BenchService) Hide() {
	if s.window != nil {
		s.window.Hide()
	}
}

func (s *BenchService) Show(source string) {
	cycleID := fmt.Sprintf("%s-%d", source, time.Now().UnixNano())
	s.logger.Log("hotkey_received", cycleID, map[string]any{"source": source})
	if s.window != nil {
		s.window.Center()
		s.window.Show().Focus()
	}
	if s.app != nil {
		s.app.Event.Emit("show_palette", map[string]string{
			"cycle_id": cycleID,
			"source":   source,
		})
	}
}

func (s *BenchService) RunBenchmark() {
	queries := []string{"a", "pr", "api", "web", "manager", "ios", "zed"}
	go func() {
		for i := 0; i < 100; i++ {
			cycleID := fmt.Sprintf("benchmark-%d", time.Now().UnixNano())
			s.logger.Log("hotkey_received", cycleID, map[string]any{"source": "benchmark"})
			if s.window != nil {
				s.window.Show().Focus()
			}
			if s.app != nil {
				s.app.Event.Emit("benchmark_query", map[string]string{
					"cycle_id": cycleID,
					"query":    queries[i%len(queries)],
				})
			}
			time.Sleep(14 * time.Millisecond)
		}
		s.logger.Log("benchmark_cycle_completed", "", map[string]any{"count": 100})
	}()
}

func (s *BenchService) OpenProject(cycleID string, projectPath string, projectID string, scenario string, query string, selectedIndex int) {
	s.logger.Log("open_requested", cycleID, nil)
	_ = exec.Command(zedCommand(), projectPath).Start()
	s.logger.Log("open_dispatched", cycleID, map[string]any{
		"project_id":     projectID,
		"scenario":       scenario,
		"query":          query,
		"selected_index": selectedIndex,
	})
}

func zedCommand() string {
	if path, err := exec.LookPath("zed"); err == nil {
		return path
	}
	for _, candidate := range []string{"/usr/local/bin/zed", "/opt/homebrew/bin/zed"} {
		if isExecutableFile(candidate) {
			return candidate
		}
	}
	return "zed"
}

func isExecutableFile(path string) bool {
	info, err := os.Stat(path)
	if err != nil || info.IsDir() {
		return false
	}
	return info.Mode()&0o111 != 0
}

func loadProjects() []Project {
	candidates := []string{}
	if exe, err := os.Executable(); err == nil {
		if macOSDir := filepath.Dir(exe); macOSDir != "." {
			if contentsDir := filepath.Dir(macOSDir); contentsDir != "." {
				candidates = append(candidates, filepath.Join(contentsDir, "Resources", "projects.json"))
			}
		}
	}
	candidates = append(candidates,
		filepath.Join("resources", "projects.json"),
		filepath.Join("..", "shared", "projects.json"),
		filepath.Join("shared", "projects.json"),
	)
	for _, candidate := range candidates {
		data, err := os.ReadFile(candidate)
		if err != nil {
			continue
		}
		var projects []Project
		if json.Unmarshal(data, &projects) == nil {
			for i := range projects {
				if projects[i].Aliases == nil {
					projects[i].Aliases = []string{}
				}
			}
			return projects
		}
	}
	return nil
}
