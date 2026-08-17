package main

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestResolveCLIFilePathsNoArgs(t *testing.T) {
	oldFile, newFile, err := resolveCLIFilePaths(nil, ".")
	if err != nil || oldFile != "" || newFile != "" {
		t.Fatalf("无参数应保持普通 GUI 启动，得到 old=%q new=%q err=%v", oldFile, newFile, err)
	}
}

func TestResolveCLIFilePathsRelativePaths(t *testing.T) {
	workingDir, err := filepath.Abs(".")
	if err != nil {
		t.Fatal(err)
	}
	oldFile, newFile, err := resolveCLIFilePaths(
		[]string{"testdata/AA.xlsx", "testdata/BB.xlsx"},
		workingDir,
	)
	if err != nil {
		t.Fatal(err)
	}
	if !filepath.IsAbs(oldFile) || !filepath.IsAbs(newFile) {
		t.Fatalf("路径未转为绝对路径：old=%q new=%q", oldFile, newFile)
	}
	if filepath.Base(oldFile) != "AA.xlsx" || filepath.Base(newFile) != "BB.xlsx" {
		t.Fatalf("文件顺序错误：old=%q new=%q", oldFile, newFile)
	}
}

func TestResolveCLIFilePathsRejectsInvalidArgumentCount(t *testing.T) {
	_, _, err := resolveCLIFilePaths([]string{"testdata/AA.xlsx"}, ".")
	if err == nil || !strings.Contains(err.Error(), "需要两个文件参数") {
		t.Fatalf("应拒绝非两个文件参数，得到 err=%v", err)
	}
}

func TestResolveCLIFilePathsRejectsMissingFile(t *testing.T) {
	_, _, err := resolveCLIFilePaths(
		[]string{"testdata/missing.xlsx", "testdata/BB.xlsx"},
		".",
	)
	if err == nil || !strings.Contains(err.Error(), "旧文件无效") {
		t.Fatalf("应报告旧文件无效，得到 err=%v", err)
	}
}
