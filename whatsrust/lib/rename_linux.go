//go:build linux

package main

import "golang.org/x/sys/unix"

// renameNoReplace atomically renames oldName to newName inside parentDir,
// failing with EEXIST when newName already exists.
func renameNoReplace(parentDir int, oldName, newName string) error {
	return unix.Renameat2(parentDir, oldName, parentDir, newName, unix.RENAME_NOREPLACE)
}
