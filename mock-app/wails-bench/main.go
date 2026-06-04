package main

import (
	"embed"
	"log"
	"runtime"

	"github.com/wailsapp/wails/v3/pkg/application"
)

//go:embed all:frontend/dist
var assets embed.FS

func init() {
	application.RegisterEvent[map[string]string]("show_palette")
	application.RegisterEvent[map[string]string]("benchmark_query")
}

func main() {
	runtime.LockOSThread()

	service := NewBenchService()

	app := application.New(application.Options{
		Name:        "WailsBench",
		Description: "Project launcher benchmark for Wails",
		Services: []application.Service{
			application.NewService(service),
		},
		Assets: application.AssetOptions{
			Handler: application.AssetFileServerFS(assets),
		},
		Mac: application.MacOptions{
			ApplicationShouldTerminateAfterLastWindowClosed: false,
		},
	})

	window := app.Window.NewWithOptions(application.WebviewWindowOptions{
		Name:          "main",
		Title:         "WailsBench",
		Width:         760,
		Height:        520,
		Hidden:        false,
		DisableResize: true,
		Frameless:     true,
		AlwaysOnTop:   true,
		URL:           "/",
		Mac: application.MacWindow{
			TitleBar: application.MacTitleBarHidden,
		},
		BackgroundColour: application.NewRGB(20, 22, 27),
	})
	service.setRuntime(app, window)

	if err := startGlobalHotkey(); err != nil {
		log.Printf("global hotkey registration failed: %v", err)
	}
	go func() {
		for range hotkeyEvents {
			service.Show("hotkey")
		}
	}()

	if err := app.Run(); err != nil {
		log.Fatal(err)
	}
}
