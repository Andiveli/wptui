package main

import (
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"strings"
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
		if err != nil {
			reportTempRootAncestors(t, root)
		}
		t.Fatalf("nested target = %q, %v", got, err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "escape")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape/file.jpg", []byte("data")); err == nil {
		t.Fatal("symlink escape was accepted")
	}
}

// reportTempRootAncestors records only fixture paths, and only when the root open fails.
func reportTempRootAncestors(t *testing.T, root string) {
	t.Helper()
	path, err := filepath.Abs(root)
	if err != nil {
		t.Logf("temp root %q: absolute path error: %v", root, err)
		return
	}
	t.Logf("temp root: %q", path)
	ancestor := string(filepath.Separator)
	for _, part := range strings.Split(strings.TrimPrefix(path, ancestor), ancestor) {
		if part != "" {
			ancestor = filepath.Join(ancestor, part)
		}
		info, statErr := os.Lstat(ancestor)
		if statErr != nil {
			t.Logf("temp ancestor %q: lstat error: %v", ancestor, statErr)
		} else if info.Mode()&os.ModeSymlink != 0 {
			t.Logf("temp ancestor %q: symlink", ancestor)
		} else if !info.IsDir() {
			t.Logf("temp ancestor %q: non-directory (%s)", ancestor, info.Mode())
		}
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
	if _, err := os.Stat(filepath.Join(outside, "file.jpg")); !os.IsNotExist(err) {
		t.Fatalf("rejected root changed outside destination: %v", err)
	}
	if err := writeDownload(root, "nested/file.jpg", []byte("data")); err != nil {
		t.Fatal(err)
	}
	if got, err := os.ReadFile(filepath.Join(root, "nested/file.jpg")); err != nil || string(got) != "data" {
		t.Fatalf("download = %q, %v", got, err)
	}
	outsideFile := filepath.Join(outside, "escape.jpg")
	if err := os.WriteFile(outsideFile, []byte("original"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outsideFile, filepath.Join(root, "escape.jpg")); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(root, "escape.jpg", []byte("data")); err == nil {
		t.Fatal("symlinked final path was replaced")
	}
	got, err := os.ReadFile(outsideFile)
	if err != nil || string(got) != "original" {
		t.Fatalf("rejected final symlink changed outside file: %q, %v", got, err)
	}
}

func TestMacOSSystemVarAliasDoesNotAllowOtherSymlinkAncestors(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("macOS system /var alias")
	}
	root := t.TempDir()
	outside := t.TempDir()
	link := filepath.Join(root, "linked")
	destination := filepath.Join(outside, "file.jpg")
	if err := os.WriteFile(destination, []byte("original"), 0o600); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, link); err != nil {
		t.Fatal(err)
	}
	if err := writeDownload(link, "file.jpg", []byte("data")); err == nil {
		t.Fatal("caller-controlled symlink root was accepted")
	}
	got, err := os.ReadFile(destination)
	if err != nil || string(got) != "original" {
		t.Fatalf("rejected root changed outside destination: %q, %v", got, err)
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
