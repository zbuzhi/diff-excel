#![cfg_attr(
    all(target_os = "windows", feature = "desktop"),
    windows_subsystem = "windows"
)]

use anyhow::{bail, Result};
use clap::Parser;
use diff_excel::{
    cli::{default_output, Args},
    compare::{compare, Options},
    workbook::{match_all, Workbook},
};

#[cfg(feature = "desktop")]
mod desktop;

fn main() {
    if let Err(error) = run() {
        eprintln!("Diff Excel: {error:#}");
        #[cfg(feature = "desktop")]
        if !std::env::args_os().any(|arg| arg == "--headless") {
            rfd::MessageDialog::new()
                .set_title("Diff Excel · 启动失败")
                .set_description(format!(
                    "{error:#}\n\n可使用 --headless 模式进行无界面对比。"
                ))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    if args.old.is_some() != args.new.is_some() {
        bail!("需要同时提供旧文件和新文件");
    }
    if args.headless {
        let old = Workbook::load(args.old.as_ref().unwrap())?;
        let new = Workbook::load(args.new.as_ref().unwrap())?;
        let pairs = match_all(&old.sheets(), &new.sheets());
        let report = compare(
            &old,
            &new,
            &pairs,
            &Options {
                output: args.output.unwrap_or_else(default_output),
                keep_format: !args.no_format,
                comments: !args.no_comments,
                color: args.color,
            },
            |text| eprintln!("{text}"),
        )?;
        println!(
            "{} differences\n{}\n{}",
            report.total,
            report.excel.display(),
            report.log.display()
        );
        return Ok(());
    }
    #[cfg(feature = "desktop")]
    return desktop::run(args);
    #[cfg(not(feature = "desktop"))]
    bail!("此构建不包含桌面界面，请使用 --headless")
}
