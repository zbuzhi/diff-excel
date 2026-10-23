use diff_excel::{
    analysis::{analyze, changed_parts, Cancellation, Query},
    compare::{self, Options},
    preferences::Preferences,
    workbook::{match_all, Workbook},
};
use std::path::Path;

fn save(book: &umya_spreadsheet::Workbook, path: &Path) -> Workbook {
    umya_spreadsheet::writer::xlsx::write(book, path).unwrap();
    Workbook::load(path).unwrap()
}
#[test]
fn full_search_pages_beyond_5000_and_export_reuses_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = umya_spreadsheet::new_file();
    let sheet = a.sheet_mut(0).unwrap();
    for row in 1..=6_200 {
        sheet.cell_mut((1, row)).set_value(format!("旧客户{row}"));
    }
    let old = save(&a, &dir.path().join("old.xlsx"));
    let mut b = a.clone();
    let sheet = b.sheet_mut(0).unwrap();
    for row in 1..=6_200 {
        sheet
            .cell_mut((1, row))
            .set_value(format!("新客户{row}\n说明\t😀"));
    }
    b.new_sheet("新增表")
        .unwrap()
        .cell_mut("B2")
        .set_value("目标");
    let new = save(&b, &dir.path().join("new.xlsx"));
    let old_bytes = std::fs::read(&old.path).unwrap();
    let new_bytes = std::fs::read(&new.path).unwrap();
    let cancel = Cancellation::default();
    let analysis = analyze(
        &old,
        &new,
        &match_all(&old.sheets(), &new.sheets()),
        &cancel,
        |_| {},
    )
    .unwrap();
    assert_eq!(analysis.summary.total, 6_202);
    let mut q = Query {
        text: "新客户6200".into(),
        ..Default::default()
    };
    let found = analysis.query(&q, &cancel).unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.rows[0].cell, "A6200");
    assert!(found.rows[0].new.contains('\n'));
    q = Query {
        offset: 6_000,
        limit: 200,
        ..Default::default()
    };
    let page = analysis.query(&q, &cancel).unwrap();
    assert_eq!(page.total, 6_202);
    assert_eq!(page.rows.len(), 200);
    assert_eq!(page.rows[0].cell, "A6001");
    q = Query {
        kind: 1,
        sheet: Some("（新增） → 新增表".into()),
        ..Default::default()
    };
    assert_eq!(analysis.query(&q, &cancel).unwrap().total, 2);
    q.kind = 2;
    assert_eq!(analysis.query(&q, &cancel).unwrap().total, 0);
    let mut opts = Options {
        output: dir.path().join("reports"),
        keep_format: true,
        comments: true,
        color: "#874BAC".into(),
    };
    assert!(!opts.output.exists(), "分析阶段不创建导出目录");
    for comments in [true, false] {
        opts.comments = comments;
        let report = compare::export(&old, &new, &analysis, &opts, &cancel, |_| {}).unwrap();
        assert_eq!(report.total, 6_202);
        assert_eq!(report.preview.len(), 5_000);
        let out = umya_spreadsheet::reader::xlsx::read(&report.excel).unwrap();
        let sheet = out.sheet_by_name("Sheet1").unwrap();
        assert_eq!(sheet.value("A6200"), "新客户6200\n说明\t😀");
        assert_eq!(!sheet.comments().is_empty(), comments);
        assert!(std::fs::read_to_string(report.log)
            .unwrap()
            .contains("新客户6200"));
    }
    assert_eq!(std::fs::read(&old.path).unwrap(), old_bytes);
    assert_eq!(std::fs::read(&new.path).unwrap(), new_bytes);
}
#[test]
fn cancellation_during_analysis_query_and_export_leaves_no_published_report() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = umya_spreadsheet::new_file();
    a.sheet_mut(0).unwrap().cell_mut("A1").set_value("old");
    let old = save(&a, &dir.path().join("old.xlsx"));
    a.sheet_mut(0).unwrap().cell_mut("A1").set_value("new");
    let new = save(&a, &dir.path().join("new.xlsx"));
    let pairs = match_all(&old.sheets(), &new.sheets());
    let cancel = Cancellation::default();
    assert!(analyze(&old, &new, &pairs, &cancel, |_| cancel.cancel()).is_err());
    let analysis = analyze(&old, &new, &pairs, &Cancellation::default(), |_| {}).unwrap();
    assert!(analysis.query(&Query::default(), &cancel).is_err());
    let opts = Options {
        output: dir.path().join("reports"),
        keep_format: true,
        comments: true,
        color: "#FFF0BD".into(),
    };
    let cancel = Cancellation::default();
    assert!(compare::export(&old, &new, &analysis, &opts, &cancel, |_| cancel.cancel()).is_err());
    assert_eq!(std::fs::read_dir(&opts.output).unwrap().count(), 0);
    assert!(compare::export(
        &old,
        &new,
        &analysis,
        &opts,
        &Cancellation::default(),
        |_| {}
    )
    .is_ok());
}
#[test]
fn preferences_recover_from_invalid_data_and_preserve_recent_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prefs.json");
    std::fs::write(&path, "bad json").unwrap();
    let mut p = Preferences::load(&path);
    p.color = "#874BAC".into();
    p.comments = false;
    for n in 0..12 {
        p.remember(&dir.path().join(format!("{n}.xlsx")));
    }
    let again = dir.path().join("8.xlsx");
    p.remember(&again);
    p.save(&path).unwrap();
    let p = Preferences::load(&path);
    assert_eq!(p.recent.len(), 8);
    assert_eq!(p.recent[0], again);
    assert_eq!(p.color, "#874BAC");
    assert!(!p.comments);
    std::fs::write(&path, r##"{"color":"bad"}"##).unwrap();
    assert_eq!(Preferences::load(&path).color, "#FFF0BD");
}
#[test]
fn changed_ranges_support_unicode_insert_delete_and_equal_content() {
    assert_eq!(
        changed_parts("客户😀上海", "客户😀北京"),
        ("客户😀", "上海", "")
    );
    assert_eq!(changed_parts("甲新增乙", "甲乙"), ("甲", "新增", "乙"));
    assert_eq!(changed_parts("甲乙", "甲新增乙"), ("甲", "", "乙"));
    assert_eq!(changed_parts("same", "same"), ("same", "", ""));
    assert_eq!(changed_parts("", "x"), ("", "", ""));
}

#[test]
fn identical_visible_values_still_explain_type_and_formula_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = umya_spreadsheet::new_file();
    let sheet = a.sheet_mut(0).unwrap();
    sheet.cell_mut("A1").set_value_number(12);
    sheet.cell_mut("B1").set_formula("1+1");
    let old = save(&a, &dir.path().join("old.xlsx"));
    a.sheet_mut(0)
        .unwrap()
        .cell_mut("A1")
        .set_value_string("12");
    a.sheet_mut(0).unwrap().cell_mut("B1").set_formula("2+0");
    let new = save(&a, &dir.path().join("new.xlsx"));
    let result = analyze(
        &old,
        &new,
        &match_all(&old.sheets(), &new.sheets()),
        &Cancellation::default(),
        |_| {},
    )
    .unwrap();
    let page = result
        .query(&Query::default(), &Cancellation::default())
        .unwrap();
    assert_eq!(page.rows[0].old, page.rows[0].new);
    assert_eq!(page.rows[0].reason, "类型变化");
    assert_ne!(page.rows[0].old_type, page.rows[0].new_type);
    assert_eq!(page.rows[1].reason, "公式变化");
}
