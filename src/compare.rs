use crate::workbook::{safe_sheet_name, SheetPair, Workbook};
use anyhow::{bail, Context, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use umya_spreadsheet::{Cell, Comment, SheetStateValues, Worksheet};

pub const PREVIEW_LIMIT: usize = 5_000;

#[derive(Clone)]
pub struct Options {
    pub output: PathBuf,
    pub keep_format: bool,
    pub comments: bool,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Removed,
    Modified,
    SheetAdded,
    SheetRemoved,
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Added => "新增",
            Self::Removed => "删除",
            Self::Modified => "修改",
            Self::SheetAdded => "新增工作表",
            Self::SheetRemoved => "删除工作表",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Difference {
    pub sheet: String,
    pub cell: String,
    pub old: String,
    pub new: String,
    pub kind: Kind,
}

#[derive(Debug)]
pub struct Report {
    pub excel: PathBuf,
    pub log: PathBuf,
    pub preview: Vec<Difference>,
    pub total: usize,
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub sheets: usize,
    pub elapsed: Duration,
}

pub fn parse_color(color: &str) -> Result<String> {
    let color = color.strip_prefix('#').unwrap_or(color);
    if color.len() != 6 || !color.bytes().all(|c| c.is_ascii_hexdigit()) {
        bail!("高亮颜色需为 #RRGGBB，例如 #FFF0BD");
    }
    Ok(format!("FF{}", color.to_ascii_uppercase()))
}

fn content(cell: Option<&Cell>) -> (String, String, String) {
    match cell {
        Some(c) if !c.value().is_empty() || !c.formula().is_empty() => (
            c.data_type().to_owned(),
            c.value().into_owned(),
            c.formula().to_owned(),
        ),
        _ => Default::default(),
    }
}

fn display(cell: Option<&Cell>) -> String {
    cell.map(|c| {
        if c.formula().is_empty() {
            c.value().into_owned()
        } else {
            format!("={} [缓存值：{}]", c.formula(), c.value())
        }
    })
    .unwrap_or_default()
}

fn sheet<'a>(book: &'a Workbook, name: &Option<String>) -> Result<Option<&'a Worksheet>> {
    name.as_deref()
        .map(|n| {
            book.book
                .sheet_by_name(n)
                .with_context(|| format!("找不到工作表：{n}"))
        })
        .transpose()
}

pub fn compare(
    old: &Workbook,
    new: &Workbook,
    pairs: &[SheetPair],
    options: &Options,
    mut progress: impl FnMut(String),
) -> Result<Report> {
    let started = Instant::now();
    let color = parse_color(&options.color)?;
    if pairs.is_empty() {
        bail!("请至少添加一组工作表");
    }
    let mut destinations = HashSet::new();
    for pair in pairs {
        if pair.old.is_none() && pair.new.is_none() {
            bail!("对比组的两侧不能同时为空");
        }
        sheet(old, &pair.old)?;
        sheet(new, &pair.new)?;
        if let Some(name) = &pair.new {
            if !destinations.insert(name.clone()) {
                bail!("新工作表 {name} 被重复配对，请为每张新工作表保留一组对比");
            }
        }
    }
    std::fs::create_dir_all(&options.output).context("无法创建输出目录，请选择可写目录")?;
    // Work in a private staging directory; publish only after both outputs succeed.
    let staging = tempfile::Builder::new()
        .prefix(".diff-excel-")
        .tempdir_in(&options.output)?;
    let excel = staging.path().join("diff.xlsx");
    let log = staging.path().join("changes.txt");
    let mut writer = BufWriter::new(std::fs::File::create(&log)?);
    writeln!(
        writer,
        "Diff Excel 2\n旧文件：{}\n新文件：{}\n",
        old.path.display(),
        new.path.display()
    )?;
    writeln!(
        writer,
        "说明：按坐标比较值、类型和公式；不检测行移动或纯样式差异。"
    )?;
    let mut output = if options.keep_format && !new.empty {
        new.book.clone()
    } else {
        let mut plain = umya_spreadsheet::new_file_empty_worksheet();
        plain.set_date1904(new.book.date1904());
        plain.set_defined_names(new.book.defined_names().to_vec());
        // Keep formula dependencies even when exporting without formatting.
        for source in new.book.sheet_collection() {
            let target = plain.new_sheet(source.name())?;
            target.set_defined_names(source.defined_names().to_vec());
            for cell in source.cells() {
                let coordinate = cell.coordinate().get_coordinate();
                target
                    .cell_mut(coordinate.as_str())
                    .set_cell_value(cell.cell_value().clone());
            }
        }
        plain
    };
    let mut used: HashSet<String> = output
        .sheet_collection()
        .iter()
        .map(|s| s.name().to_lowercase())
        .collect();
    let mut report = Report {
        excel: PathBuf::new(),
        log: PathBuf::new(),
        preview: Vec::new(),
        total: 0,
        added: 0,
        removed: 0,
        modified: 0,
        sheets: pairs.len(),
        elapsed: Duration::ZERO,
    };

    for (index, pair) in pairs.iter().enumerate() {
        progress(format!(
            "正在对比 {}/{} · {}",
            index + 1,
            pairs.len(),
            pair.label()
        ));
        let old_sheet = sheet(old, &pair.old)?;
        let new_sheet = sheet(new, &pair.new)?;
        let output_name = if pair.new.is_some() {
            pair.new.clone().unwrap()
        } else {
            let base = pair.new.as_deref().or(pair.old.as_deref()).unwrap();
            let base = if pair.new.is_none() {
                format!("{base}（已删除）")
            } else {
                base.to_owned()
            };
            let name = safe_sheet_name(&base, &mut used);
            let target = output.new_sheet(&name)?;
            if options.keep_format {
                let source = old_sheet.unwrap();
                // Copy presentation only. Cloning an old worksheet wholesale
                // could duplicate table names and old workbook-scoped objects.
                for cell in source.cells() {
                    let coordinate = cell.coordinate().get_coordinate();
                    target
                        .cell_mut(coordinate.as_str())
                        .set_style(cell.style().clone());
                }
                *target.column_dimensions_mut() = source.column_dimensions().to_vec();
                *target.row_dimensions_to_hashmap_mut() =
                    source.row_dimensions_to_hashmap().clone();
                *target.merge_cells_mut() = source.merge_cells().to_vec();
                *target.comments_mut() = source.comments().to_vec();
                target.set_sheet_format_properties(source.sheet_format_properties().clone());
            }
            name
        };
        writeln!(writer, "\n=== {} | 输出：{} ===", pair.label(), output_name)?;
        if pair.old.is_none() || pair.new.is_none() {
            record(
                &mut report,
                &mut writer,
                Difference {
                    sheet: pair.label(),
                    cell: "—".into(),
                    old: pair.old.clone().unwrap_or_default(),
                    new: pair.new.clone().unwrap_or_default(),
                    kind: if pair.old.is_none() {
                        Kind::SheetAdded
                    } else {
                        Kind::SheetRemoved
                    },
                },
            )?;
        }
        // Visit only stored cells, including cells that exist on just one side.
        let mut coordinates = BTreeSet::new();
        for s in [old_sheet, new_sheet].into_iter().flatten() {
            for c in s.cells() {
                let coordinate = c.coordinate();
                coordinates.insert((coordinate.row_num(), coordinate.col_num()));
            }
        }
        let target = output.sheet_by_name_mut(&output_name).unwrap();
        let mut comment_indices: HashMap<String, usize> = target
            .comments()
            .iter()
            .enumerate()
            .map(|(i, c)| (c.coordinate().get_coordinate(), i))
            .collect();
        for (row, col) in coordinates {
            let before = old_sheet.and_then(|s| s.cell((col, row)));
            let after = new_sheet.and_then(|s| s.cell((col, row)));
            let old_content = content(before);
            let new_content = content(after);
            if old_content == new_content {
                continue;
            }
            let kind = if old_content == Default::default() {
                Kind::Added
            } else if new_content == Default::default() {
                Kind::Removed
            } else {
                Kind::Modified
            };
            let cell = umya_spreadsheet::helper::coordinate::coordinate_from_index(col, row);
            let old_text = display(before);
            let new_text = display(after);
            // The destination already contains new values/formulas. Deleted cells are
            // blank and highlighted; their old contents remain available in notes/log.
            if after.is_none() || new_content == Default::default() {
                target
                    .cell_mut((col, row))
                    .set_cell_value(Default::default());
            }
            target
                .style_mut((col, row))
                .set_background_color_solid(&color);
            if options.comments && before.is_some() {
                let existing = comment_indices.get(&cell).copied();
                let note = format!(
                    "Diff Excel · {}\n旧内容：{}\n新内容：{}",
                    kind.label(),
                    old_text,
                    new_text
                );
                if let Some(index) = existing {
                    let comment = &mut target.comments_mut()[index];
                    let text = comment.text();
                    let previous = text
                        .text()
                        .map(|t| t.value().to_owned())
                        .or_else(|| text.rich_text().map(|r| r.text().into_owned()))
                        .unwrap_or_default();
                    comment.text_mut().remove_rich_text();
                    comment.set_text_string(format!("{previous}\n\n{note}"));
                } else {
                    let mut comment = Comment::default();
                    comment
                        .new_comment((col, row))
                        .set_author("Diff Excel")
                        .set_text_string(note);
                    comment_indices.insert(cell.clone(), target.comments().len());
                    target.comments_mut().push(comment);
                }
            }
            record(
                &mut report,
                &mut writer,
                Difference {
                    sheet: pair.label(),
                    cell,
                    old: old_text,
                    new: new_text,
                    kind,
                },
            )?;
        }
    }
    // Excel requires at least one visible worksheet.
    let visible = output
        .sheet_collection()
        .iter()
        .position(|s| matches!(s.state(), SheetStateValues::Visible));
    if visible.is_none() {
        output
            .sheet_mut(0)
            .unwrap()
            .set_state(SheetStateValues::Visible);
    }
    output.set_active_sheet(visible.unwrap_or(0) as u32);
    progress("正在保存 Excel 和完整差异日志…".to_owned());
    umya_spreadsheet::writer::xlsx::write(&output, &excel).context("写入差异 Excel 失败")?;
    writeln!(
        writer,
        "\n总差异：{}，新增：{}，删除：{}，修改：{}",
        report.total, report.added, report.removed, report.modified
    )?;
    writer.flush()?;
    drop(writer);
    // Reserve a unique final name. Rename within the same filesystem is atomic.
    let final_dir = tempfile::Builder::new()
        .prefix("comparison-")
        .tempdir_in(&options.output)?;
    std::fs::rename(&excel, final_dir.path().join("diff.xlsx"))?;
    std::fs::rename(&log, final_dir.path().join("changes.txt"))?;
    let final_path = final_dir.keep();
    report.excel = final_path.join("diff.xlsx");
    report.log = final_path.join("changes.txt");
    report.elapsed = started.elapsed();
    Ok(report)
}

fn record(report: &mut Report, writer: &mut impl Write, diff: Difference) -> Result<()> {
    report.total += 1;
    match diff.kind {
        Kind::Added | Kind::SheetAdded => report.added += 1,
        Kind::Removed | Kind::SheetRemoved => report.removed += 1,
        Kind::Modified => report.modified += 1,
    }
    // Debug string escaping keeps tabs/newlines in cells from corrupting the log.
    writeln!(
        writer,
        "[{}] {} {} | {:?} → {:?}",
        diff.kind.label(),
        diff.sheet,
        diff.cell,
        diff.old,
        diff.new
    )?;
    if report.preview.len() < PREVIEW_LIMIT {
        report.preview.push(diff);
    }
    Ok(())
}
