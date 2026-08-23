package main

/*
#include <stdint.h>
*/
import "C"

import (
	"math"
	"mime"
	"sort"
)

func SliceIndex(list []string, value string, defaultValue int) int {
	for index, item := range list {
		if item == value {
			return index
		}
	}
	return defaultValue
}

// ExtensionByType chooses a stable, useful extension for a MIME type when
// constructing the relative media path exposed through the C ABI.
func ExtensionByType(mimeType string, defaultExt string) string {
	ext := defaultExt
	exts, extErr := mime.ExtensionsByType(mimeType)
	if extErr == nil && len(exts) > 0 {
		// Prefer common extensions over less common (.jpe, etc) returned by mime.
		preferredExts := []string{".jpg", ".jpeg"}
		sort.Slice(exts, func(i, j int) bool {
			return SliceIndex(preferredExts, exts[i], math.MaxInt32) < SliceIndex(preferredExts, exts[j], math.MaxInt32)
		})
		ext = exts[0]
	}
	return ext
}

//export C_DownloadFile
func C_DownloadFile(fileId *C.char, basePath *C.char) C.uint8_t {
	goFileId := C.GoString(fileId)
	goBasePath := C.GoString(basePath)
	status := DownloadFromFileId(client, goFileId, goBasePath)
	return C.uint8_t(status)
}
