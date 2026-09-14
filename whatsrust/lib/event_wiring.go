package main

/*
#include "callback_log_registration.h"
*/
import "C"

func AddEventHandlers() {
	lifecycleState.registerEventHandlers(func() {
		addEventHandlers()
	})
}

func addEventHandlers() {
	viewOnceDispatcher := newViewOnceUnavailableDispatcher(HandleViewOnceUnavailableMessage)
	clientSnapshot := lifecycleState.clientSnapshot()
	clientSnapshot.AddEventHandler(func(rawEvt any) {
		messageActionCensusDiagnostic(rawEvt)
		dispatchEventFamily(rawEvt, clientSnapshot, viewOnceDispatcher.dispatchOnce)
	})
}
