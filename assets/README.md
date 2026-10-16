# Diff Excel 图标

两张错位的表格表示旧、新工作簿，蓝黄单元格表示变化，双向箭头表示对比。
使用与界面一致的米白、墨色、灰蓝和芥末黄，避免在 Dock 小尺寸图标内放置文字。

- `diff-excel.png`：透明背景设计原稿。
- `icon-256.png`：桌面窗口使用的内嵌 PNG。
- `DiffExcel.icns`：macOS 应用包图标，包含 16–1024 像素多种尺寸。

原稿使用内置 imagegen 工具生成。最终提示词：

> Create a single production-ready macOS desktop application icon for 'Diff Excel', an independent spreadsheet comparison tool. Square 1024x1024 composition with transparent pixels outside one large rounded-square app tile, tile fills about 86 percent of canvas centered with generous consistent macOS icon padding. Design language: refined flat editorial neo-brutalist, warm ivory paper #F5EDD8 tile, charcoal #2B2B27 crisp bold outlines, muted slate blue #3F5870 and mustard yellow #E8BC4F. Central symbol: two overlapping spreadsheet cards, rear card slate blue offset up-left, foreground ivory spreadsheet offset down-right with a strong simple 3-by-3 grid. One mustard square cell and one blue square cell clearly highlight changed cells. A compact charcoal bidirectional comparison arrow bridges the two sheets near lower edge. Visually balanced, iconic silhouette, bold readable shape at 32px, minimal details, clean professional vector-like edges, restrained small hard offset shadow inside tile, no texture. Not an Excel/Microsoft logo. No letters, no wordmark, no text, no surrounding scene, no mockup, no additional icons. The result must be the actual app icon asset, front view.

macOS 上运行 `sh scripts/generate-icons.sh` 可从原稿重建尺寸资源。
