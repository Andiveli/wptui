package main

import (
	"errors"
	"os"
	"path/filepath"
	"testing"
)

func TestSafeDownloadTargetRejectsUnsafePathsAndPreservesNestedPaths(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	for _, target := range []string{"../escape", "/absolute", "", "."} {
		if _, err := safeDownloadTarget(root, target); err == nil {
			t.Fatalf("unsafe target %q was accepted", target)
		}
	}
	if got, err := safeDownloadTarget(root, "nested/file.jpg"); err != nil || got != filepath.Join(root, "nested/file.jpg") {
		t.Fatalf("nested target = %q, %v", got, err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "escape")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape/file.jpg", []byte("data")); err == nil {
		t.Fatal("symlink escape was accepted")
	}
}

func TestWriteDownloadRejectsSymlinkedRootsAndFinalPaths(t *testing.T) {
	root, outside := t.TempDir(), t.TempDir()
	link := filepath.Join(t.TempDir(), "media")
	if err := os.Symlink(outside, link); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(link, "file.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked root was accepted")
	}
	if err := writeDownload(root, "nested/file.jpg", []byte("data")); err != nil {
		t.Fatal(err)
	}
	if got, err := os.ReadFile(filepath.Join(root, "nested/file.jpg")); err != nil || string(got) != "data" {
		t.Fatalf("download = %q, %v", got, err)
	}
	if err := os.Symlink(filepath.Join(outside, "escape.jpg"), filepath.Join(root, "escape.jpg")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked final path was replaced")
	}
}

func TestWriteDownloadNeverAcceptsExistingOrPartialRegularFilesAndCleansFailedWrites(t *testing.T) {
	root := t.TempDir()
	destination := filepath.Join(root, "partial.jpg")
	if err := os.WriteFile(destination, []byte("partial"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "partial.jpg", []byte("complete")); err == nil {
		t.Fatal("existing partial regular file was accepted")
	}
	got, err := os.ReadFile(destination)
	if err != nil || string(got) != "partial" {
		t.Fatalf("existing file was changed: %q, %v", got, err)
	}
	if _, err := os.Stat(filepath.Join(root, ".partial.jpg.part")); !os.IsNotExist(err) {
		t.Fatalf("rename failure left a stale temporary file: %v", err)
	}
	if err := os.Remove(destination); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "partial.jpg", []byte("retry")); err != nil {
		t.Fatalf("retry after destination removal failed: %v", err)
	}
}

func TestWriteDownloadCleansTemporaryFileAfterWriteFailure(t *testing.T) {
	root := t.TempDir()
	writeFailure := errors.New("injected write failure")

	err := writeDownloadWithWriter(root, "failed.jpg", []byte("data"), func(*os.File, []byte) (int, error) {
		return 0, writeFailure
	})

	if !errors.Is(err, writeFailure) {
		t.Fatalf("write error = %v, want injected failure", err)
	}
	if _, err := os.Stat(filepath.Join(root, ".failed.jpg.part")); !os.IsNotExist(err) {
		t.Fatalf("failed write left a stale temporary file: %v", err)
	}
}
