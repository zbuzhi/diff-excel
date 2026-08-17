package main

import (
	"fmt"
	"os"
)

// 程序入口
// 应用程序结构体和核心逻辑 app.go
// UI相关逻辑 ui.go
// Excel对比逻辑 compare.go
// 工具函数 utils
func main() {
	app := NewExcelCompareApp(750, 900)
	if err := app.loadFilesFromArgs(os.Args[1:]); err != nil {
		app.appendLog(fmt.Sprintf("启动参数错误：%v\n", err))
	}
	app.Run()
}
