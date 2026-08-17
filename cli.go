package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// resolveCLIFilePaths 解析 Git difftool 传入的两个位置参数：旧文件和新文件。
// Git 可能把工作区文件作为相对路径传入，因此统一基于当前工作目录转为绝对路径。
func resolveCLIFilePaths(args []string, workingDir string) (string, string, error) {
	if len(args) == 0 {
		return "", "", nil
	}
	if len(args) != 2 {
		return "", "", fmt.Errorf("需要两个文件参数，用法：DiffExcel.exe <旧文件.xlsx> <新文件.xlsx>")
	}

	oldFile, err := resolveCLIFilePath(args[0], workingDir)
	if err != nil {
		return "", "", fmt.Errorf("旧文件无效：%w", err)
	}
	newFile, err := resolveCLIFilePath(args[1], workingDir)
	if err != nil {
		return "", "", fmt.Errorf("新文件无效：%w", err)
	}
	return oldFile, newFile, nil
}

func resolveCLIFilePath(path, workingDir string) (string, error) {
	path = strings.TrimSpace(path)
	if path == "" {
		return "", fmt.Errorf("文件路径为空")
	}
	if !filepath.IsAbs(path) {
		path = filepath.Join(workingDir, path)
	}
	absPath, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	absPath = filepath.Clean(absPath)
	if !strings.EqualFold(filepath.Ext(absPath), ".xlsx") {
		return "", fmt.Errorf("仅支持 .xlsx 文件：%s", absPath)
	}
	info, err := os.Stat(absPath)
	if err != nil {
		return "", err
	}
	if info.IsDir() {
		return "", fmt.Errorf("路径是目录：%s", absPath)
	}
	return absPath, nil
}

func (a *ExcelCompareApp) loadFilesFromArgs(args []string) error {
	workingDir, err := os.Getwd()
	if err != nil {
		return fmt.Errorf("获取当前工作目录失败：%w", err)
	}
	oldFile, newFile, err := resolveCLIFilePaths(args, workingDir)
	if err != nil || oldFile == "" {
		return err
	}
	if err := a.loadSourceFile(oldFile); err != nil {
		return fmt.Errorf("加载旧文件失败：%w", err)
	}
	if err := a.loadCompareFile(newFile); err != nil {
		return fmt.Errorf("加载新文件失败：%w", err)
	}
	a.appendLog(fmt.Sprintf("已从启动参数加载文件：\n旧：%s\n新：%s\n", oldFile, newFile))
	return nil
}
