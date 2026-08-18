package main

/*
#include "callback_log_registration.h"
#include <stdlib.h>
*/
import "C"
import (
	"fmt"
	"os"
	"unsafe"
)

var logHandler C.LogHandler
var messageHandler C.MessageHandler
var eventHandler C.EventHandler
var presenceHandler C.PresenceHandler

func LOG_LEVEL(level int, msg string, args ...any) {
	formatted := fmt.Sprintf(msg, args...)
	if os.Getenv("WPTUI_LOGS_STDERR") == "1" {
		// Debug aid for headless capture: mirror every Go log to stderr so a
		// run like `WPTUI_LOGS_STDERR=1 target/debug/wp-tui 2>go.log` lets the
		// user read diagnostics after closing the TUI (Ctrl+L panel is broken).
		fmt.Fprintf(os.Stderr, "[go %d] %s\n", level, formatted)
	}
	if logHandler.callback == nil {
		return
	}
	cmsg := C.CString(formatted)
	defer C.free(unsafe.Pointer(cmsg))
	C.callLogInfo(logHandler, cmsg, C.uint8_t(level))
}

func LOG_ERROR(msg string, args ...any) {
	LOG_LEVEL(0, msg, args...)
}

func LOG_WARN(msg string, args ...any) {
	LOG_LEVEL(1, msg, args...)
}

func LOG_INFO(msg string, args ...any) {
	LOG_LEVEL(2, msg, args...)
}

func LOG_DEBUG(msg string, args ...any) {
	LOG_LEVEL(4, msg, args...)
}

//export C_SetLogHandler
func C_SetLogHandler(callback C.LogHandlerCallback, data unsafe.Pointer) {
	logHandler = C.LogHandler{
		callback:  callback,
		user_data: data,
	}
}

//export C_SetEventHandler
func C_SetEventHandler(callback C.EventCallback, data unsafe.Pointer) {
	eventHandler = C.EventHandler{
		callback:  callback,
		user_data: data,
	}
}

//export C_SetMessageHandler
func C_SetMessageHandler(callback C.MessageHandlerCallback, data unsafe.Pointer) {
	messageHandler = C.MessageHandler{
		callback:  callback,
		user_data: data,
	}
}

//export C_SetPresenceHandler
func C_SetPresenceHandler(callback C.PresenceHandlerCallback, data unsafe.Pointer) {
	presenceHandler = C.PresenceHandler{
		callback:  callback,
		user_data: data,
	}
}
