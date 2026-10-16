use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Diff Excel · 本地 Excel 差异对比",
    after_help = "示例：diff-excel old.xlsx new.xlsx\n无界面对比：diff-excel --headless old.xlsx new.xlsx --output ./reports"
)]
pub struct Args {
    /// 旧文件；Git 新增文件时支持 /dev/null、NUL 或空占位文件
    pub old: Option<PathBuf>,
    /// 新文件；Git 删除文件时支持 /dev/null、NUL 或空占位文件
    #[arg(requires = "old")]
    pub new: Option<PathBuf>,
    /// 无图形界面，按同名 Sheet 对比全部工作表
    #[arg(long, requires_all = ["old", "new"])]
    pub headless: bool,
    /// 尝试软件图形适配器（如 Windows WARP），用于显卡驱动不可用的环境
    #[arg(long, conflicts_with = "headless")]
    pub software: bool,
    /// 报告输出目录，默认使用系统文档目录下的 Diff Excel
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// 不保留单元格格式（仍保留值的类型和公式）
    #[arg(long)]
    pub no_format: bool,
    /// 不在差异单元格中添加旧内容批注
    #[arg(long)]
    pub no_comments: bool,
    /// 高亮颜色，#RRGGBB
    #[arg(long, default_value = "#FFF0BD")]
    pub color: String,
}

pub fn default_output() -> PathBuf {
    // A launcher/Git cwd can be read-only or a temporary directory.
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    home.map(PathBuf::from)
        .map(|p| p.join("Documents").join("Diff Excel"))
        .unwrap_or_else(|| std::env::temp_dir().join("Diff Excel"))
}
