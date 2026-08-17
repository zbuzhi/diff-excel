package main

import (
	"os"
	"testing"

	fyneTest "fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

// 这里测试 CompareExcelFiles 函数，跳过UI，直接调用纯逻辑
func TestCompareExcelFiles(t *testing.T) {
	testApp := fyneTest.NewApp()
	defer testApp.Quit()

	a := ExcelCompareApp{}
	a.srcFile = "testdata/AA.xlsx"
	a.srcSheet = "Sheet1"
	a.cmpFile = "testdata/BB.xlsx"
	a.cmpSheet = "Sheet1"
	a.outExcelFile = "testdata/out_diff.xlsx"
	a.outLogFile = "testdata/out_diff.txt"
	a.highlightClr = "#FF0000"
	a.keepOriginalFormat = false // 不保持原格式

	// 初始化日志缓冲区
	a.logRich = widget.NewMultiLineEntry()

	err := a.CompareExcelFiles()
	if err != nil {
		t.Fatalf("CompareExcelFiles failed: %v", err)
	}

	// 简单断言：检查输出文件是否生成
	if _, err := os.Stat(a.outExcelFile); os.IsNotExist(err) {
		t.Errorf("输出 Excel 文件未生成")
	}
	if _, err := os.Stat(a.outLogFile); os.IsNotExist(err) {
		t.Errorf("输出日志文件未生成")
	}

	// 测试完可以删除生成文件，保持环境干净
	os.Remove(a.outExcelFile)
	os.Remove(a.outLogFile)
}

// 测试保持原格式功能
func TestCompareExcelFilesWithFormat(t *testing.T) {
	testApp := fyneTest.NewApp()
	defer testApp.Quit()

	a := ExcelCompareApp{}
	a.srcFile = "testdata/AA.xlsx"
	a.srcSheet = "Sheet1"
	a.cmpFile = "testdata/BB.xlsx"
	a.cmpSheet = "Sheet1"
	a.outExcelFile = "testdata/out_diff_format.xlsx"
	a.outLogFile = "testdata/out_diff_format.txt"
	a.highlightClr = "#FF0000"
	a.keepOriginalFormat = true // 保持原格式
	a.showOldInComment = true

	// 初始化日志缓冲区
	a.logRich = widget.NewMultiLineEntry()

	err := a.CompareExcelFiles()
	if err != nil {
		t.Fatalf("CompareExcelFiles with format failed: %v", err)
	}

	// 简单断言：检查输出文件是否生成
	if _, err := os.Stat(a.outExcelFile); os.IsNotExist(err) {
		t.Errorf("输出 Excel 文件未生成")
	}
	if _, err := os.Stat(a.outLogFile); os.IsNotExist(err) {
		t.Errorf("输出日志文件未生成")
	}

	// 测试完可以删除生成文件，保持环境干净
	os.Remove(a.outExcelFile)
	os.Remove(a.outLogFile)
}
