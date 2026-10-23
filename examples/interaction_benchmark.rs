//! Deterministic end-to-end fixture for the paged desktop workflow.
use anyhow::Result;
use diff_excel::{
    analysis::{analyze, Cancellation, Query},
    compare::{self, Options},
    workbook::{match_all, Workbook},
};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<()> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("diff-excel-interaction-qa"));
    std::fs::create_dir_all(&root)?;
    let mut old = umya_spreadsheet::new_file();
    let sheet = old.sheet_mut(0).unwrap();
    for col in 1..=20 {
        sheet.cell_mut((col, 1)).set_value(format!("列{col}"));
    }
    for row in 2..=10_000 {
        for col in 1..=20 {
            sheet.cell_mut((col, row)).set_value_number(row * col);
        }
        sheet.cell_mut((2, row)).set_value(format!("客户{row}"));
        sheet
            .cell_mut((20, row))
            .set_formula(format!("A{row}*C{row}"));
    }
    let a = root.join("旧版本-10000x20.xlsx");
    let b = root.join("新版本-10000x20.xlsx");
    umya_spreadsheet::writer::xlsx::write(&old, &a)?;
    for row in 2..=10_000 {
        old.sheet_mut(0)
            .unwrap()
            .cell_mut((2, row))
            .set_value(format!("更新客户{row}"));
    }
    umya_spreadsheet::writer::xlsx::write(&old, &b)?;
    drop(old);
    let a_bytes = std::fs::read(&a)?;
    let b_bytes = std::fs::read(&b)?;
    let t = Instant::now();
    let old = Workbook::load(&a)?;
    let new = Workbook::load(&b)?;
    let load = t.elapsed();
    let cancel = Cancellation::default();
    let result = analyze(
        &old,
        &new,
        &match_all(&old.sheets(), &new.sheets()),
        &cancel,
        |_| {},
    )?;
    assert_eq!(result.summary.total, 9_999);
    let t = Instant::now();
    let page = result.query(
        &Query {
            text: "更新客户10000".into(),
            ..Default::default()
        },
        &cancel,
    )?;
    let search = t.elapsed();
    assert_eq!(page.total, 1);
    assert_eq!(page.rows[0].cell, "B10000");
    let t = Instant::now();
    let report = compare::export(
        &old,
        &new,
        &result,
        &Options {
            output: root.join("reports"),
            keep_format: true,
            comments: true,
            color: "#874BAC".into(),
        },
        &cancel,
        |_| {},
    )?;
    let export = t.elapsed();
    assert_eq!(std::fs::read(&a)?, a_bytes);
    assert_eq!(std::fs::read(&b)?, b_bytes);
    let output = umya_spreadsheet::reader::xlsx::read(&report.excel)?;
    assert_eq!(
        output.sheet_by_name("Sheet1").unwrap().value("B10000"),
        "更新客户10000"
    );
    assert_eq!(
        output.sheet_by_name("Sheet1").unwrap().comments().len(),
        9_999
    );
    println!("fixture={}\nload_s={:.3}\nanalyze_s={:.3}\nfull_search_s={:.3}\nexport_s={:.3}\ndifferences={}\ninputs_unchanged=true\nreport={}",root.display(),load.as_secs_f64(),result.elapsed.as_secs_f64(),search.as_secs_f64(),export.as_secs_f64(),report.total,report.excel.display());
    Ok(())
}
