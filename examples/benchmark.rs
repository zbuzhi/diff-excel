//! Release-mode benchmark using existing XLSX fixtures; timings exclude fixture generation.
use anyhow::{bail, Result};
use diff_excel::{
    compare::{compare, Options},
    workbook::{match_all, Workbook},
};
use std::{path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        bail!("usage: benchmark OLD.xlsx NEW.xlsx OUTPUT [--no-comments]");
    }
    let started = Instant::now();
    let old = Workbook::load(&PathBuf::from(&args[0]))?;
    let old_seconds = started.elapsed().as_secs_f64();
    let next = Instant::now();
    let new = Workbook::load(&PathBuf::from(&args[1]))?;
    let new_seconds = next.elapsed().as_secs_f64();
    let pairs = match_all(&old.sheets(), &new.sheets());
    let mut save_started = None;
    let compare_started = Instant::now();
    let report = compare(
        &old,
        &new,
        &pairs,
        &Options {
            output: PathBuf::from(&args[2]),
            keep_format: true,
            comments: args.get(3).is_none_or(|arg| arg != "--no-comments"),
            color: "#FFF0BD".into(),
        },
        |message| {
            if message.starts_with("正在保存") {
                save_started = Some(Instant::now());
            }
        },
    )?;
    let compare_seconds = compare_started.elapsed().as_secs_f64();
    let save_seconds = save_started.map_or(0., |start| start.elapsed().as_secs_f64());
    println!("old_read_s\t{old_seconds:.6}\nnew_read_s\t{new_seconds:.6}\ncompare_export_s\t{compare_seconds:.6}\nexport_s\t{save_seconds:.6}\ntotal_s\t{:.6}\nadded\t{}\nremoved\t{}\nmodified\t{}\ntotal\t{}\npreview\t{}\nexcel\t{}\nlog\t{}", started.elapsed().as_secs_f64(), report.added,report.removed,report.modified,report.total,report.preview.len(),report.excel.display(),report.log.display());
    Ok(())
}
