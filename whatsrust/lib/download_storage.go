package main

import (
	"errors"
	"os"
	"path/filepath"
	"strings"

	"golang.org/x/sys/unix"
)

func safeDownloadTarget(basePath, targetPath string) (string, error) {
	clean := filepath.Clean(targetPath)
	if targetPath == "" || filepath.IsAbs(targetPath) || clean == "." || clean != targetPath || clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
		return "", errors.New("unsafe download target path")
	}
	fd, err := openDownloadRoot(basePath)
	if err != nil {
		return "", err
	}
	unix.Close(fd)
	return filepath.Join(basePath, clean), nil
}

func writeDownload(basePath, targetPath string, data []byte) error {
	return writeDownloadWithWriter(basePath, targetPath, data, func(file *os.File, data []byte) (int, error) {
		return file.Write(data)
	})
}

func writeDownloadWithWriter(basePath, targetPath string, data []byte, write func(*os.File, []byte) (int, error)) error {
	if _, err := safeDownloadTarget(basePath, targetPath); err != nil {
		return err
	}
	parent, err := openDownloadRoot(basePath)
	if err != nil {
		return err
	}
	defer func() {
		unix.Close(parent)
	}()
	parts := strings.Split(targetPath, string(filepath.Separator))
	for _, part := range parts[:len(parts)-1] {
		next, err := unix.Openat(parent, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		if err == unix.ENOENT {
			err = unix.Mkdirat(parent, part, 0o700)
			next, err = unix.Openat(parent, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		}
		if err != nil {
			return err
		}
		unix.Close(parent)
		parent = next
	}
	tempName := "." + parts[len(parts)-1] + ".part"
	fd, err := unix.Openat(parent, tempName, unix.O_WRONLY|unix.O_CREAT|unix.O_EXCL|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0o600)
	if err != nil {
		return err
	}
	tempCreated := true
	defer func() {
		if tempCreated {
			_ = unix.Unlinkat(parent, tempName, 0)
		}
	}()
	file := os.NewFile(uintptr(fd), tempName)
	_, err = write(file, data)
	if closeErr := file.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return err
	}
	if err := renameNoReplace(parent, tempName, parts[len(parts)-1]); err != nil {
		return err
	}
	tempCreated = false
	return nil
}

func openDownloadRoot(basePath string) (int, error) {
	path, err := filepath.Abs(basePath)
	if err != nil {
		return -1, err
	}
	fd, err := unix.Open("/", unix.O_RDONLY|unix.O_DIRECTORY|unix.O_CLOEXEC, 0)
	if err != nil {
		return -1, err
	}
	for _, part := range strings.Split(path, string(filepath.Separator)) {
		if part == "" {
			continue
		}
		next, err := unix.Openat(fd, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		if err == unix.ENOENT {
			err = unix.Mkdirat(fd, part, 0o700)
			next, err = unix.Openat(fd, part, unix.O_RDONLY|unix.O_DIRECTORY|unix.O_NOFOLLOW|unix.O_CLOEXEC, 0)
		}
		unix.Close(fd)
		if err != nil {
			return -1, err
		}
		fd = next
	}
	return fd, nil
}
