use diff_excel::{
    compare::{compare, Kind, Options},
    workbook::{match_all, safe_sheet_name, SheetPair, Workbook},
};
use std::{collections::HashSet, path::Path};
use umya_spreadsheet as xlsx;

fn save(book: &xlsx::Workbook, path: &Path) -> Workbook {
    xlsx::writer::xlsx::write(book, path).unwrap();
    Workbook::load(path).unwrap()
}

fn options(path: &Path, keep_format: bool) -> Options {
    Options {
        output: path.join("reports"),
        keep_format,
        comments: true,
        color: "#FFF0BD".into(),
    }
}

#[test]
fn all_sheets_match_names_in_workbook_order_and_include_one_sided_sheets() {
    let old = ["B", "A", "deleted"].map(str::to_owned);
    let new = ["A", "B", "added"].map(str::to_owned);
    let pairs = match_all(&old, &new);
    assert_eq!(
        pairs[0],
        SheetPair {
            old: Some("B".into()),
            new: Some("B".into())
        }
    );
    assert_eq!(pairs[2].new, None);
    assert_eq!(pairs[3].old, None);
    assert_eq!(match_all(&[], &[]), vec![]);
}

#[test]
fn sheet_names_are_valid_unique_and_utf16_bounded() {
    let mut used = HashSet::new();
    for base in [
        "Sheet",
        "sheet",
        "'bad:/[]*?\\'",
        "",
        "非常长的工作表名称非常长的工作表名称非常长的工作表名称😀😀😀",
    ] {
        let name = safe_sheet_name(base, &mut used);
        assert!(name.encode_utf16().count() <= 31);
        assert!(!name.is_empty());
        assert!(!name.chars().any(|c| "[]:*?/\\".contains(c)));
        assert!(!name.starts_with('\''));
        assert!(!name.ends_with('\''));
    }
    assert_eq!(used.len(), 5);
}

#[test]
fn compare_preserves_types_formulas_styles_and_existing_comments() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = xlsx::new_file();
    let s = old.sheet_mut(0).unwrap();
    s.cell_mut("A1").set_value("old");
    s.cell_mut("B1").set_value_number(12);
    s.cell_mut("C1").set_formula("1+1");
    s.cell_mut("D1").set_value("removed");
    s.cell_mut("E1").set_value("0012");
    let old = save(&old, &dir.path().join("old.xlsx"));
    let mut new = old.book.clone();
    let s = new.sheet_mut(0).unwrap();
    s.cell_mut("A1").set_value("new\nline\ttext");
    s.cell_mut("B1").set_value_string("12");
    s.cell_mut("C1").set_formula("2+0");
    s.cell_mut("D1").set_cell_value(Default::default());
    s.cell_mut("F1").set_value_bool(true);
    s.style_mut("A1").font_mut().set_bold(true);
    s.column_dimension_mut("A").set_width(24.);
    s.row_dimension_mut(1).set_height(28.);
    s.add_merge_cells("G2:H2");
    let mut comment = xlsx::Comment::default();
    comment
        .new_comment("A1")
        .set_author("original")
        .set_text_string("Existing note");
    s.comments_mut().push(comment);
    let new = save(&new, &dir.path().join("new.xlsx"));
    let old_bytes = std::fs::read(&old.path).unwrap();
    let new_bytes = std::fs::read(&new.path).unwrap();
    for keep_format in [false, true] {
        let report = compare(
            &old,
            &new,
            &match_all(&old.sheets(), &new.sheets()),
            &options(dir.path(), keep_format),
            |_| {},
        )
        .unwrap();
        assert_eq!((report.added, report.removed, report.modified), (1, 1, 3));
        let output = xlsx::reader::xlsx::read(&report.excel).unwrap();
        let s = output.sheet_by_name("Sheet1").unwrap();
        assert_eq!(s.cell("B1").unwrap().data_type(), "s");
        assert_eq!(s.cell("C1").unwrap().formula(), "2+0");
        assert!(s.value("D1").is_empty());
        assert_eq!(s.cell("F1").unwrap().data_type(), "b");
        if keep_format {
            assert!(s.style("A1").font().unwrap().bold());
            assert_eq!(s.merge_cells().len(), 1);
            let comment = s
                .comments()
                .iter()
                .find(|c| c.coordinate().get_coordinate() == "A1")
                .unwrap();
            assert!(comment
                .text()
                .text()
                .unwrap()
                .value()
                .contains("Existing note"));
        }
        let log = std::fs::read_to_string(&report.log).unwrap();
        assert!(log.contains("new\\nline\\ttext"));
    }
    assert_eq!(std::fs::read(&old.path).unwrap(), old_bytes);
    assert_eq!(std::fs::read(&new.path).unwrap(), new_bytes);
}

#[test]
fn empty_added_deleted_sheets_and_git_null_inputs_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut book = xlsx::new_file();
    book.new_sheet("empty").unwrap();
    book.sheet_mut(0)
        .unwrap()
        .cell_mut("XFD1048576")
        .set_value("sparse");
    let full = save(&book, &dir.path().join("full.xlsx"));
    let null = Workbook::load(Path::new(if cfg!(windows) { "NUL" } else { "/dev/null" })).unwrap();
    for (old, new, expected) in [
        (&null, &full, Kind::SheetAdded),
        (&full, &null, Kind::SheetRemoved),
    ] {
        let report = compare(
            old,
            new,
            &match_all(&old.sheets(), &new.sheets()),
            &options(dir.path(), true),
            |_| {},
        )
        .unwrap();
        assert_eq!(report.total, 3); // Two sheet changes and one sparse cell.
        assert!(report.preview.iter().any(|d| d.kind == expected));
        let output = xlsx::reader::xlsx::read(&report.excel).unwrap();
        assert_eq!(output.sheet_count(), 2);
    }
    let empty = dir.path().join("empty-placeholder");
    std::fs::write(&empty, []).unwrap();
    assert!(Workbook::load(&empty).unwrap().empty);
}

#[test]
fn invalid_pairs_and_colors_fail_without_publishing_reports() {
    let dir = tempfile::tempdir().unwrap();
    let empty_path = dir.path().join("no-sheets.xlsx");
    xlsx::writer::xlsx::write(&xlsx::new_file_empty_worksheet(), &empty_path).unwrap();
    assert!(Workbook::load(&empty_path).is_err());
    let book = save(&xlsx::new_file(), &dir.path().join("book.xlsx"));
    let pair = SheetPair {
        old: Some("Sheet1".into()),
        new: Some("Sheet1".into()),
    };
    let opts = options(dir.path(), true);
    assert!(compare(&book, &book, &[], &opts, |_| {}).is_err());
    assert!(compare(&book, &book, &[pair.clone(), pair.clone()], &opts, |_| {}).is_err());
    let mut invalid = opts.clone();
    invalid.color = "#ZZZZZZ".into();
    assert!(compare(&book, &book, &[pair], &invalid, |_| {}).is_err());
    assert!(!opts.output.exists());
}

#[test]
fn cli_parses_spaces_unicode_and_missing_file_pairs() {
    use clap::Parser;
    use diff_excel::cli::Args;
    let args =
        Args::try_parse_from(["diff-excel", "--headless", "旧 文件.xlsx", "新 文件.xlsx"]).unwrap();
    assert_eq!(args.old.unwrap(), Path::new("旧 文件.xlsx"));
    assert!(Args::try_parse_from(["diff-excel", "--headless", "one.xlsx"]).is_err());
}

#[test]
fn subset_export_preserves_cross_sheet_dependencies_and_long_names() {
    let dir = tempfile::tempdir().unwrap();
    let name = "很长的工作表名称很长的工作表名称很长的工作表名称";
    let mut before = xlsx::new_file();
    before.set_date1904(true);
    let mut rate = xlsx::DefinedName::default();
    rate.set_name("Rate").set_address("Data!$A$1");
    before.defined_names_mut().push(rate);
    before
        .new_sheet("Data")
        .unwrap()
        .cell_mut("A1")
        .set_value_number(10);
    before
        .sheet_mut(0)
        .unwrap()
        .cell_mut("A1")
        .set_formula("Data!A1");
    before
        .sheet_mut(0)
        .unwrap()
        .set_state(xlsx::SheetStateValues::Hidden);
    before.new_sheet(name).unwrap();
    let old = save(&before, &dir.path().join("old.xlsx"));
    let mut after = before.clone();
    after.remove_sheet_by_name(name).unwrap();
    after
        .sheet_mut(0)
        .unwrap()
        .cell_mut("A1")
        .set_formula("Data!A1*2");
    let new = save(&after, &dir.path().join("new.xlsx"));
    let pairs = vec![
        SheetPair {
            old: Some("Sheet1".into()),
            new: Some("Sheet1".into()),
        },
        SheetPair {
            old: Some(name.into()),
            new: None,
        },
    ];
    for keep_format in [false, true] {
        let report = compare(
            &old,
            &new,
            &pairs,
            &options(dir.path(), keep_format),
            |_| {},
        )
        .unwrap();
        let result = xlsx::reader::xlsx::read(&report.excel).unwrap();
        assert_eq!(result.sheet_count(), 3);
        assert!(result.date1904());
        assert!(matches!(
            result.active_sheet().state(),
            xlsx::SheetStateValues::Visible
        ));
        if keep_format {
            assert_eq!(result.active_sheet().name(), "Data");
        }
        assert!(result
            .defined_names()
            .iter()
            .chain(
                result
                    .sheet_collection()
                    .iter()
                    .flat_map(|s| s.defined_names())
            )
            .any(|n| n.name() == "Rate"));
        assert_eq!(result.sheet_by_name("Data").unwrap().value("A1"), "10");
        assert_eq!(
            result
                .sheet_by_name("Sheet1")
                .unwrap()
                .cell("A1")
                .unwrap()
                .formula(),
            "Data!A1*2"
        );
        assert!(result
            .sheet_collection()
            .iter()
            .all(|s| s.name().encode_utf16().count() <= 31));
    }
}

#[test]
fn headless_binary_handles_real_unicode_paths_and_returns_errors() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join(" 旧 文件.xlsx");
    let new_path = dir.path().join("新 文件.xlsx");
    let old = xlsx::new_file();
    save(&old, &old_path);
    let mut new = old.clone();
    new.sheet_mut(0)
        .unwrap()
        .cell_mut("A1")
        .set_value("changed");
    save(&new, &new_path);
    let binary = env!("CARGO_BIN_EXE_diff-excel");
    let output = std::process::Command::new(binary)
        .arg("--headless")
        .arg(&old_path)
        .arg(&new_path)
        .arg("--output")
        .arg(dir.path().join("reports"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 differences"));
    let failure = std::process::Command::new(binary)
        .args(["--headless", "missing.xlsx", "missing.xlsx"])
        .output()
        .unwrap();
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("无法读取文件"));
}
