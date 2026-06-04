package main

/*
#cgo LDFLAGS: -framework Carbon
#include <Carbon/Carbon.h>

extern void goHotKeyPressed(void);

static EventHotKeyRef hotKeyRef = NULL;
static EventHandlerRef eventHandlerRef = NULL;

static OSStatus hotKeyHandler(EventHandlerCallRef nextHandler, EventRef event, void *userData) {
    goHotKeyPressed();
    return noErr;
}

static OSStatus registerBenchHotKey(void) {
    EventTypeSpec eventType;
    eventType.eventClass = kEventClassKeyboard;
    eventType.eventKind = kEventHotKeyPressed;
    OSStatus handlerStatus = InstallEventHandler(GetApplicationEventTarget(), &hotKeyHandler, 1, &eventType, NULL, &eventHandlerRef);
    if (handlerStatus != noErr) {
        return handlerStatus;
    }

    EventHotKeyID hotKeyID;
    hotKeyID.signature = 'WBNH';
    hotKeyID.id = 1;
    return RegisterEventHotKey(kVK_ANSI_M, controlKey, hotKeyID, GetApplicationEventTarget(), 0, &hotKeyRef);
}
*/
import "C"
import "fmt"

var hotkeyEvents = make(chan struct{}, 8)

//export goHotKeyPressed
func goHotKeyPressed() {
	select {
	case hotkeyEvents <- struct{}{}:
	default:
	}
}

func startGlobalHotkey() error {
	status := C.registerBenchHotKey()
	if status != 0 {
		return fmt.Errorf("RegisterEventHotKey failed: %d", int(status))
	}
	return nil
}
