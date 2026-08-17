package main

import (
	"diff_excel/utils"
	"fmt"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/widget"
)

// 初始化Sheet选择器
func (a *ExcelCompareApp) initSheetSelectors() {
	// 初始化新的Sheet对比组选择器
	a.currentSrcSheetSelect = widget.NewSelect([]string{}, nil)
	a.currentSrcSheetSelect.PlaceHolder = "选择旧文件Sheet"

	a.currentCmpSheetSelect = widget.NewSelect([]string{}, nil)
	a.currentCmpSheetSelect.PlaceHolder = "选择新文件Sheet"

	// 添加对比组按钮
	a.addPairButton = widget.NewButton("添加Sheet对比组", func() {
		a.addSheetPair()
	})

	// Sheet对比组列表
	a.sheetPairsList = widget.NewList(
		func() int { return len(a.sheetPairs) },
		func() fyne.CanvasObject {
			return container.NewHBox(
				widget.NewLabel(""),
				widget.NewButton("删除", nil),
			)
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			if id >= len(a.sheetPairs) {
				return
			}
			box := obj.(*fyne.Container)
			label := box.Objects[0].(*widget.Label)
			deleteBtn := box.Objects[1].(*widget.Button)

			pair := a.sheetPairs[id]
			label.SetText(pair.DisplayName) // 直接使用DisplayName

			deleteBtn.OnTapped = func() {
				a.removeSheetPair(id)
			}
		},
	)

	// 单Sheet模式的Select组件（保持向后兼容）
	a.srcSheetMultiSelect = widget.NewList(
		func() int { return len(a.srcSheets) },
		func() fyne.CanvasObject {
			check := widget.NewCheck("", nil)
			return container.NewHBox(check, widget.NewLabel(""))
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			if id >= len(a.srcSheets) {
				return
			}
			box := obj.(*fyne.Container)
			check := box.Objects[0].(*widget.Check)
			label := box.Objects[1].(*widget.Label)

			sheetName := a.srcSheets[id]
			label.SetText(sheetName)
			check.SetChecked(a.selectedSrcSheets[sheetName])
			check.OnChanged = func(checked bool) {
				a.selectedSrcSheets[sheetName] = checked
				a.updateSheetMappings()
			}
		},
	)

	a.cmpSheetMultiSelect = widget.NewList(
		func() int { return len(a.cmpSheets) },
		func() fyne.CanvasObject {
			check := widget.NewCheck("", nil)
			return container.NewHBox(check, widget.NewLabel(""))
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			if id >= len(a.cmpSheets) {
				return
			}
			box := obj.(*fyne.Container)
			check := box.Objects[0].(*widget.Check)
			label := box.Objects[1].(*widget.Label)

			sheetName := a.cmpSheets[id]
			label.SetText(sheetName)
			check.SetChecked(a.selectedCmpSheets[sheetName])
			check.OnChanged = func(checked bool) {
				a.selectedCmpSheets[sheetName] = checked
				a.updateSheetMappings()
			}
		},
	)

	// Sheet映射列表
	a.sheetMappingList = widget.NewList(
		func() int { return len(a.sheetMappings) },
		func() fyne.CanvasObject {
			return widget.NewLabel("")
		},
		func(id widget.ListItemID, obj fyne.CanvasObject) {
			label := obj.(*widget.Label)
			mappings := a.getSheetMappingsList()
			if id < len(mappings) {
				label.SetText(mappings[id])
			}
		},
	)
}

// 获取Sheet映射列表显示
func (a *ExcelCompareApp) getSheetMappingsList() []string {
	var mappings []string
	for src, cmp := range a.sheetMappings {
		mappings = append(mappings, fmt.Sprintf("%s → %s", src, cmp))
	}
	return mappings
}

// 添加Sheet对比组
func (a *ExcelCompareApp) addSheetPair() {
	srcSheet := a.currentSrcSheetSelect.Selected
	cmpSheet := a.currentCmpSheetSelect.Selected

	if srcSheet == "" || cmpSheet == "" {
		dialog.ShowError(fmt.Errorf("请先选择旧文件和新文件的Sheet"), a.myWindow)
		return
	}

	// 检查是否已经存在相同的对比组
	for _, pair := range a.sheetPairs {
		if pair.SrcSheet == srcSheet && pair.CmpSheet == cmpSheet {
			dialog.ShowError(fmt.Errorf("该Sheet对比组已存在"), a.myWindow)
			return
		}
	}

	// 创建新的对比组，使用新的命名格式
	newPair := SheetPair{
		SrcSheet:    srcSheet,
		CmpSheet:    cmpSheet,
		DisplayName: fmt.Sprintf("【新】%s <<【旧】%s", cmpSheet, srcSheet),
	}

	a.sheetPairs = append(a.sheetPairs, newPair)

	// 刷新列表显示
	if a.sheetPairsList != nil {
		a.sheetPairsList.Refresh()
		// 重新创建UI布局以更新容器高度
		a.myWindow.SetContent(a.createUILayout())
	}

	// 重置选择器
	a.currentSrcSheetSelect.SetSelected("")
	a.currentCmpSheetSelect.SetSelected("")

	a.appendLog(fmt.Sprintf("已添加Sheet对比组: %s\n", newPair.DisplayName))
}

// 删除Sheet对比组
func (a *ExcelCompareApp) removeSheetPair(index int) {
	if index >= 0 && index < len(a.sheetPairs) {
		removedPair := a.sheetPairs[index]
		a.sheetPairs = append(a.sheetPairs[:index], a.sheetPairs[index+1:]...)

		// 刷新列表显示
		if a.sheetPairsList != nil {
			a.sheetPairsList.Refresh()
			// 重新创建UI布局以更新容器高度
			a.myWindow.SetContent(a.createUILayout())
		}

		a.appendLog(fmt.Sprintf("已删除Sheet对比组: %s\n", removedPair.DisplayName))
	}
}

// 更新Sheet映射（保持向后兼容）
func (a *ExcelCompareApp) updateSheetMappings() {
	// 清空现有映射
	a.sheetMappings = make(map[string]string)

	// 获取选中的源Sheet和目标Sheet
	var selectedSrc []string
	var selectedCmp []string

	for sheet, selected := range a.selectedSrcSheets {
		if selected {
			selectedSrc = append(selectedSrc, sheet)
		}
	}
	for sheet, selected := range a.selectedCmpSheets {
		if selected {
			selectedCmp = append(selectedCmp, sheet)
		}
	}

	// 一对一映射（按顺序匹配）
	minLen := len(selectedSrc)
	if len(selectedCmp) < minLen {
		minLen = len(selectedCmp)
	}

	for i := 0; i < minLen; i++ {
		a.sheetMappings[selectedSrc[i]] = selectedCmp[i]
	}

	// 更新显示
	if a.sheetMappingList != nil {
		a.sheetMappingList.Refresh()
	}
}

// 更新Sheet UI显示
func (a *ExcelCompareApp) updateSheetUI() {
	// 这里先留空，在UI布局中动态切换显示的组件
	// 由于当前实现复杂度较高，暂时保持简单实现
}

// 创建清空按钮的工具函数
func (a *ExcelCompareApp) makeClearBtn(entry *widget.Entry, sheetSelect *widget.Select, clearVars ...*string) *widget.Button {
	return widget.NewButton("清空", func() {
		entry.SetText("")
		for _, v := range clearVars {
			*v = ""
		}
		if sheetSelect != nil {
			sheetSelect.Options = []string{}
			sheetSelect.SetSelected("")
		}
	})
}

// 创建文件上传功能
func (a *ExcelCompareApp) createFileUploadHandlers() {
	a.srcUploadFunc = func() {
		fd := dialog.NewFileOpen(func(r fyne.URIReadCloser, err error) {
			if err != nil {
				dialog.ShowError(err, a.myWindow)
				return
			}
			if r == nil {
				return
			}
			path := r.URI().Path()
			r.Close()
			if err := a.loadSourceFile(path); err != nil {
				dialog.ShowError(err, a.myWindow)
			}
		}, a.myWindow)
		fd.SetFilter(storage.NewExtensionFileFilter([]string{".xlsx"}))
		fd.Show()
	}

	a.cmpUploadFunc = func() {
		fd := dialog.NewFileOpen(func(r fyne.URIReadCloser, err error) {
			if err != nil {
				dialog.ShowError(err, a.myWindow)
				return
			}
			if r == nil {
				return
			}
			path := r.URI().Path()
			r.Close()
			if err := a.loadCompareFile(path); err != nil {
				dialog.ShowError(err, a.myWindow)
			}
		}, a.myWindow)
		fd.SetFilter(storage.NewExtensionFileFilter([]string{".xlsx"}))
		fd.Show()
	}
}

func (a *ExcelCompareApp) loadSourceFile(path string) error {
	sheets, err := utils.GetSheets(path)
	if err != nil {
		return err
	}
	a.srcFile = path
	a.srcEntry.SetText(path)
	a.srcSheets = sheets
	a.selectedSrcSheets = make(map[string]bool, len(sheets))
	for _, sheet := range sheets {
		a.selectedSrcSheets[sheet] = false
	}
	a.srcSheetSelect.Options = sheets
	a.currentSrcSheetSelect.Options = sheets
	if len(sheets) > 0 {
		a.srcSheetSelect.SetSelected(sheets[0])
		a.srcSheet = sheets[0]
		a.currentSrcSheetSelect.SetSelected(sheets[0])
	} else {
		a.srcSheetSelect.SetSelected("")
		a.srcSheet = ""
		a.currentSrcSheetSelect.SetSelected("")
	}
	a.resetSheetPairsAndTryDefault()
	if a.srcSheetMultiSelect != nil {
		a.srcSheetMultiSelect.Refresh()
	}
	return nil
}

func (a *ExcelCompareApp) loadCompareFile(path string) error {
	sheets, err := utils.GetSheets(path)
	if err != nil {
		return err
	}
	a.cmpFile = path
	a.cmpEntry.SetText(path)
	a.cmpSheets = sheets
	a.selectedCmpSheets = make(map[string]bool, len(sheets))
	for _, sheet := range sheets {
		a.selectedCmpSheets[sheet] = false
	}
	a.cmpSheetSelect.Options = sheets
	a.currentCmpSheetSelect.Options = sheets
	if len(sheets) > 0 {
		a.cmpSheetSelect.SetSelected(sheets[0])
		a.cmpSheet = sheets[0]
		a.currentCmpSheetSelect.SetSelected(sheets[0])
	} else {
		a.cmpSheetSelect.SetSelected("")
		a.cmpSheet = ""
		a.currentCmpSheetSelect.SetSelected("")
	}
	a.resetSheetPairsAndTryDefault()
	if a.cmpSheetMultiSelect != nil {
		a.cmpSheetMultiSelect.Refresh()
	}
	return nil
}

func (a *ExcelCompareApp) resetSheetPairsAndTryDefault() {
	a.sheetPairs = []SheetPair{}
	if a.sheetPairsList != nil {
		a.sheetPairsList.Refresh()
	}
	a.tryAddDefaultPair()
}

// 初始化UI组件
func (a *ExcelCompareApp) initUIComponents() {
	w := a.myWindow.Canvas().Size().Width
	a.srcEntry, a.srcEntryBox = utils.MakeWideMultiLineEntry(w/2-10, 60, "", "旧文件地址")
	a.cmpEntry, a.cmpEntryBox = utils.MakeWideMultiLineEntry(w/2-10, 60, "", "新文件地址")
	a.outDirEntry, a.outDirBox = utils.MakeWideMultiLineEntry(w/2-10, 60, a.outDir, "输出文件目录") // 显示默认输出目录

	// 初始化单Sheet选择器（默认模式）
	a.srcSheetSelect = widget.NewSelect([]string{}, func(selected string) {
		a.srcSheet = selected
	})
	a.srcSheetSelect.PlaceHolder = "选择Sheet"

	a.cmpSheetSelect = widget.NewSelect([]string{}, func(selected string) {
		a.cmpSheet = selected
	})
	a.cmpSheetSelect.PlaceHolder = "选择Sheet"

	// 多Sheet模式切换复选框
	a.multiSheetCheckbox = widget.NewCheck("多Sheet对比模式", func(checked bool) {
		a.multiSheetMode = checked
		a.updateSheetUI()
	})

	// 初始化Sheet选择组件
	a.initSheetSelectors()

	a.commentCheckbox = widget.NewCheck("备注显示旧内容", func(checked bool) {
		a.showOldInComment = checked
	})

	a.formatCheckbox = widget.NewCheck("保持新文件格式（单元格大小、合并等）", func(checked bool) {
		a.keepOriginalFormat = checked
	})
	a.formatCheckbox.SetChecked(true) // 默认开启

	// 颜色输入框
	a.colorEntry = widget.NewEntry()
	a.colorEntry.SetText(a.highlightClr)
	a.colorEntry.SetPlaceHolder("输入颜色代码，如 #FF0000")

	// 创建文件上传处理器
	a.createFileUploadHandlers()
}

// 创建UI布局
func (a *ExcelCompareApp) createUILayout() fyne.CanvasObject {
	// 创建按钮
	srcBtn := widget.NewButton("上传", a.srcUploadFunc)
	cmpBtn := widget.NewButton("上传", a.cmpUploadFunc)
	utils.SetBlue(srcBtn)
	utils.SetBlue(cmpBtn)

	srcClearBtn := a.makeClearBtn(a.srcEntry, a.srcSheetSelect, &a.srcFile, &a.srcSheet)
	cmpClearBtn := a.makeClearBtn(a.cmpEntry, a.cmpSheetSelect, &a.cmpFile, &a.cmpSheet)

	outDirBtn := widget.NewButton("选择输出目录", func() {
		fd := dialog.NewFolderOpen(func(uri fyne.ListableURI, err error) {
			if err != nil {
				dialog.ShowError(err, a.myWindow)
				return
			}
			if uri == nil {
				return
			}
			a.outDir = uri.Path()
			a.outDirEntry.SetText(a.outDir)
		}, a.myWindow)
		fd.Show()
	})
	outDirClearBtn := widget.NewButton("清空", func() {
		a.outDir = ""
		a.outDirEntry.SetText("")
	})

	compareBtn := widget.NewButton("开始对比", func() {
		a.compareFunc()
	})
	utils.SetBlue(compareBtn)

	// 日志框滚动容器，固定高度
	logScroll := container.NewScroll(a.logRich)
	logScroll.SetMinSize(fyne.NewSize(750, 150))

	// 清空日志按钮，带确认对话框
	clearLogBtn := widget.NewButton("清空日志", func() {
		dialog.ShowConfirm("确认清空", "确定要清空日志吗？", func(ok bool) {
			if ok {
				a.logRich.SetText("")
				a.appendLog("")
			}
		}, a.myWindow)
	})

	// 日志标题栏，带清空按钮
	logHeader := container.NewBorder(nil, nil, nil, clearLogBtn, widget.NewLabel("日志输出："))

	spacing := canvas.NewRectangle(nil)     // 空矩形
	spacing.SetMinSize(fyne.NewSize(0, 10)) // 高度 10 像素，宽度 0

	line := canvas.NewRectangle(nil)
	line.SetMinSize(fyne.NewSize(50, 1))
	line.FillColor = utils.ParseHexColor("#fff")
	line.StrokeColor = utils.ParseHexColor("#fff")

	leftBox := container.NewVBox(
		utils.SetTitle("旧文件", 18),
		spacing,
		container.NewHBox(srcBtn, srcClearBtn), // 移除单Sheet选择器
		a.srcEntryBox,
	)
	rightBox := container.NewVBox(
		utils.SetTitle("新文件", 18),
		spacing,
		container.NewHBox(cmpBtn, cmpClearBtn), // 移除单Sheet选择器
		a.cmpEntryBox,
	)

	leftBox2 := container.NewVBox(
		utils.SetTitle("输出目录", 18),
		spacing,
		container.NewHBox(outDirBtn, outDirClearBtn),
		a.outDirBox,
	)

	makeColorSelector := utils.MakeColorSelector
	rightBox2 := container.NewVBox(
		utils.SetTitle("差异单元格设置", 18),
		utils.SetTitle("颜色高亮", 14),
		widget.NewSeparator(),
		makeColorSelector(func(color string) {
			a.colorEntry.SetText(color)
		}),
		a.colorEntry,
		widget.NewSeparator(),
		a.commentCheckbox,
		a.formatCheckbox,
	)

	content := container.NewVBox(
		container.New(layout.NewGridLayout(2), leftBox, rightBox),
		spacing,
		container.NewVBox(
			widget.NewSeparator(),
			container.New(layout.NewGridLayout(3),
				a.currentSrcSheetSelect,
				a.currentCmpSheetSelect,
				a.addPairButton,
			),
			utils.SetTitle("对比组列表", 14),
			a.createSheetPairsListContainer(), // 使用新的容器函数
		),
		spacing,
		container.New(layout.NewGridLayout(2), leftBox2, rightBox2),
		// 新的Sheet对比组功能区域
		container.NewVBox(
			widget.NewLabel(""),
			widget.NewSeparator(), // 分割线
		),
		compareBtn,
		logHeader,
		logScroll,
	)

	paddedContent := container.NewVBox(
		spacing, // 上间距
		container.NewHBox(
			spacing, // 左间距
			content,
			spacing, // 右间距
		),
		spacing, // 下间距
	)

	return container.NewScroll(paddedContent)
}

func (a *ExcelCompareApp) initUI() {
	// 初始化所有UI组件
	a.initUIComponents()

	// 初始化Sheet选择器UI
	a.updateSheetUI()

	// 创建并设置UI布局
	a.myWindow.SetContent(a.createUILayout())
}

// createSheetPairsListContainer 创建对比组列表容器，支持动态高度
func (a *ExcelCompareApp) createSheetPairsListContainer() fyne.CanvasObject {
	// 创建带滚动的容器
	scrollContainer := container.NewScroll(a.sheetPairsList)

	// 计算高度：最少显示3行，最多6行
	itemCount := len(a.sheetPairs)
	if itemCount < 3 {
		itemCount = 3 // 最少显示3行
	} else if itemCount > 6 {
		itemCount = 6 // 最多显示6行
	}

	// 每行高度约30像素，加上一些边距
	height := float32(itemCount*35 + 10)
	scrollContainer.SetMinSize(fyne.NewSize(400, height))

	return scrollContainer
}

// tryAddDefaultPair 在两个文件都已选择的情况下自动添加默认对比组
func (a *ExcelCompareApp) tryAddDefaultPair() {
	if a.srcFile != "" && a.cmpFile != "" && len(a.srcSheets) > 0 && len(a.cmpSheets) > 0 {
		// 使用两个文件的第一个Sheet创建默认对比组
		defaultPair := SheetPair{
			SrcSheet:    a.srcSheets[0],
			CmpSheet:    a.cmpSheets[0],
			DisplayName: fmt.Sprintf("【新】%s << 【旧】%s", a.cmpSheets[0], a.srcSheets[0]),
		}
		a.sheetPairs = []SheetPair{defaultPair}
		if a.sheetPairsList != nil {
			a.sheetPairsList.Refresh()
			// 重新创建UI布局以更新容器高度
			a.myWindow.SetContent(a.createUILayout())
		}
		a.appendLog(fmt.Sprintf("自动添加默认对比组: %s\n", defaultPair.DisplayName))
	}
}
