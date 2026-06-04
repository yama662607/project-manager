//go:build !darwin

package main

var hotkeyEvents = make(chan struct{}, 8)

func startGlobalHotkey() error {
	return nil
}
