# Rust 2.0 验收与 issue 对应

本版本重新实现功能，没有合并 PR #7，也没有修改 GitHub issue 的状态。

| 项目 | 实现 | 验收 |
| --- | --- | --- |
| #5 全部 Sheet | 按同名生成全部配对，单侧 Sheet 单独标记，支持全不选、逐项勾选和手动配对 | 自动化覆盖顺序变化、同名、单侧、空 Sheet |
| PR #7 Git difftool | 两个位置参数启动后后台读取并填充文件与配对；相对路径基于调用目录解析；支持空版本 | 参数及进程级测试通过；正式 macOS GUI 已验证相对路径启动、文件及 Sheet 自动填充 |
| #4 Windows 启动 | 替换 Fyne/OpenGL，以 wgpu 使用系统图形后端；独立 console 构建完全不初始化图形驱动 | Windows MSVC 目标交叉编译检查通过，原报错机器 GUI 验证待完成；已配置 Windows CI |
| #3 macOS 无法使用 | Intel/Apple Silicon 分架构打包 `.app`/DMG，校验签名，可写的默认输出目录，启动错误提示 | Apple Silicon 本机 GUI 与导出已验证；原 issue 无报错信息，不能断言原问题已复现或解决 |

## 发布验收

- 在 Windows 10/11 中解压运行 GUI，并通过 Git 客户端传入含空格、中文路径。
- 在出现 OpenGL 报错的原 Windows 机器上启动 GUI；若系统图形后端仍不可用，使用 console 版 `--headless`，并记录 GPU/驱动信息。
- 在 Intel 和 Apple Silicon 的真实 Mac 上，从 DMG 复制到 Applications 后启动；分别测试 Finder 启动与 Git 调用。
- 当前打包默认是 ad-hoc 签名，没有 Developer ID 和 notarization。公开发布前应配置自己的 Apple 证书及公证流程；不能把 ad-hoc 包描述为已通过 Gatekeeper 信任。
- 检查带样式、批注、合并、跨 Sheet 公式的样本输出，并在 Excel 或 LibreOffice 中打开确认。复杂图表、透视表、外部链接等 OOXML 对象仍需专门兼容性验收。

## 依赖兼容

umya-spreadsheet 3.1.1 的 zip 依赖指向已撤回的 8.6 版本。`compat/zip` 是本地适配包，转发到 zip 9，并把 zip 9 的可失败文件名读取适配为 umya 使用的 API；无效名称会在打开 entry 时返回错误。它不包含旧 zip 的实现。应在 umya 上游更新依赖后移除这个适配层。`Cargo.lock` 固定实际构建依赖。

上游 XML 读取器还直接匹配未带前缀的 SpreadsheetML 标签。`src/xml_namespace.rs` 在读取入口按命名空间 URI 识别这些元素，使用临时 ZIP 适配合法的 `x:` 等前缀，不修改源文件、文本、公式或属性。原始 1 万行测试文件及命名空间作用域回归测试已通过，详见 `PERFORMANCE.md`。

## 对比语义

- 按单元格坐标比较原始值、类型、公式文本；不计算公式，不识别插行/移动行，不把纯格式变化视为数据变化。
- 输出保留新值；删除的单元格留空并高亮，旧内容写入批注（可选）和完整日志。
- 新增/删除 Sheet 自身各算一条结构变化，该 Sheet 的非空单元格变化另外计数。
- 保留格式时保留新工作簿的其他 Sheet，维护跨 Sheet 引用。删除 Sheet 另建有唯一名称的报告页。
- 每张新 Sheet 最多配对一个旧 Sheet；GUI 手动配对替换冲突的自动配对。
- 预览和搜索上限 5,000 条；Excel 和日志包含全部变化。
- 所有输入只读；输出在独立目录完成后保留，不覆盖原文件或旧报告。
