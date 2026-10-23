//! Disk-backed comparison results. Only a page of matching rows enters the UI.
use crate::{
    compare::{Difference, Kind},
    workbook::{SheetPair, Workbook},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            bail!("操作已取消")
        }
        Ok(())
    }
}
#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub total: usize,
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}
impl Summary {
    pub fn add(&mut self, kind: &Kind) {
        self.total += 1;
        match kind {
            Kind::Added | Kind::SheetAdded => self.added += 1,
            Kind::Removed | Kind::SheetRemoved => self.removed += 1,
            Kind::Modified => self.modified += 1,
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StoredDifference {
    pub pair: usize,
    pub difference: Difference,
}
#[derive(Debug)]
pub struct Analysis {
    directory: tempfile::TempDir,
    pub pairs: Vec<SheetPair>,
    pub summary: Summary,
    pub elapsed: Duration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub kind: usize,
    pub sheet: Option<String>,
    pub text: String,
    pub offset: usize,
    pub limit: usize,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            kind: 0,
            sheet: None,
            text: String::new(),
            offset: 0,
            limit: 200,
        }
    }
}
#[derive(Debug, Default)]
pub struct Page {
    pub rows: Vec<Difference>,
    pub total: usize,
}
impl Analysis {
    pub(crate) fn rows(&self) -> Result<impl Iterator<Item = Result<StoredDifference>>> {
        let reader = BufReader::new(File::open(self.directory.path().join("differences.jsonl"))?);
        Ok(reader.lines().map(|line| Ok(serde_json::from_str(&line?)?)))
    }
    pub fn query(&self, query: &Query, cancel: &Cancellation) -> Result<Page> {
        let mut page = Page::default();
        let text = query.text.to_lowercase();
        for (index, record) in self.rows()?.enumerate() {
            if index % 256 == 0 {
                cancel.check()?;
            }
            let d = record?.difference;
            let kind = match query.kind {
                1 => matches!(d.kind, Kind::Added | Kind::SheetAdded),
                2 => matches!(d.kind, Kind::Removed | Kind::SheetRemoved),
                3 => d.kind == Kind::Modified,
                _ => true,
            };
            if !kind || query.sheet.as_ref().is_some_and(|s| *s != d.sheet) {
                continue;
            }
            if !text.is_empty()
                && ![&d.sheet, &d.cell, &d.old, &d.new, &d.reason]
                    .iter()
                    .any(|s| s.to_lowercase().contains(&text))
            {
                continue;
            }
            if page.total >= query.offset && page.rows.len() < query.limit {
                page.rows.push(d);
            }
            page.total += 1;
        }
        cancel.check()?;
        Ok(page)
    }
}

pub fn analyze(
    old: &Workbook,
    new: &Workbook,
    pairs: &[SheetPair],
    cancel: &Cancellation,
    mut progress: impl FnMut(String),
) -> Result<Analysis> {
    let started = Instant::now();
    cancel.check()?;
    if pairs.is_empty() {
        bail!("请至少添加一组工作表");
    }
    let mut destinations = HashSet::new();
    for pair in pairs {
        if pair.old.is_none() && pair.new.is_none() {
            bail!("对比组的两侧不能同时为空");
        }
        crate::compare::sheet(old, &pair.old)?;
        crate::compare::sheet(new, &pair.new)?;
        if let Some(name) = &pair.new {
            if !destinations.insert(name.clone()) {
                bail!("新工作表 {name} 被重复配对，请为每张新工作表保留一组对比");
            }
        }
    }
    let directory = tempfile::Builder::new()
        .prefix("diff-excel-results-")
        .tempdir()
        .context("无法创建本地差异缓存")?;
    let mut writer = BufWriter::new(File::create(directory.path().join("differences.jsonl"))?);
    let mut summary = Summary::default();
    for (index, pair) in pairs.iter().enumerate() {
        cancel.check()?;
        progress(format!(
            "正在比较工作表 {}/{} · {}",
            index + 1,
            pairs.len(),
            pair.label()
        ));
        let before = crate::compare::sheet(old, &pair.old)?;
        let after = crate::compare::sheet(new, &pair.new)?;
        let mut record = |difference: Difference| -> Result<()> {
            summary.add(&difference.kind);
            serde_json::to_writer(
                &mut writer,
                &StoredDifference {
                    pair: index,
                    difference,
                },
            )?;
            writer.write_all(b"\n")?;
            Ok(())
        };
        if pair.old.is_none() || pair.new.is_none() {
            record(Difference {
                sheet: pair.label(),
                cell: "—".into(),
                old: pair.old.clone().unwrap_or_default(),
                new: pair.new.clone().unwrap_or_default(),
                kind: if pair.old.is_none() {
                    Kind::SheetAdded
                } else {
                    Kind::SheetRemoved
                },
                reason: "工作表变化".into(),
                old_type: String::new(),
                new_type: String::new(),
            })?;
        }
        let mut coordinates = BTreeSet::new();
        for sheet in [before, after].into_iter().flatten() {
            for (i, cell) in sheet.cells().iter().enumerate() {
                if i % 4096 == 0 {
                    cancel.check()?;
                }
                let coordinate = cell.coordinate();
                coordinates.insert((coordinate.row_num(), coordinate.col_num()));
            }
        }
        let count = coordinates.len();
        for (i, (row, col)) in coordinates.into_iter().enumerate() {
            if i % 4096 == 0 {
                cancel.check()?;
                progress(format!(
                    "正在比较工作表 {}/{} · {} · {}/{} 个单元格",
                    index + 1,
                    pairs.len(),
                    pair.label(),
                    i,
                    count
                ));
            }
            let old_cell = before.and_then(|s| s.cell((col, row)));
            let new_cell = after.and_then(|s| s.cell((col, row)));
            let a = crate::compare::content(old_cell);
            let b = crate::compare::content(new_cell);
            if a == b {
                continue;
            }
            let kind = if a == Default::default() {
                Kind::Added
            } else if b == Default::default() {
                Kind::Removed
            } else {
                Kind::Modified
            };
            let reason = if kind != Kind::Modified {
                "内容变化"
            } else if a.2 != b.2 {
                "公式变化"
            } else if a.0 != b.0 {
                "类型变化"
            } else if !a.2.is_empty() {
                "计算结果变化"
            } else {
                "内容变化"
            };
            record(Difference {
                sheet: pair.label(),
                cell: umya_spreadsheet::helper::coordinate::coordinate_from_index(col, row),
                old: crate::compare::display(old_cell),
                new: crate::compare::display(new_cell),
                kind,
                reason: reason.into(),
                old_type: a.0,
                new_type: b.0,
            })?;
        }
    }
    writer.flush()?;
    cancel.check()?;
    Ok(Analysis {
        directory,
        pairs: pairs.to_vec(),
        summary,
        elapsed: started.elapsed(),
    })
}

/// Highlight the changed middle, respecting UTF-8 boundaries. Deliberately not a
/// character edit-distance matrix: very long spreadsheet cells stay cheap.
pub fn changed_parts<'a>(value: &'a str, other: &str) -> (&'a str, &'a str, &'a str) {
    let prefix: usize = value
        .chars()
        .zip(other.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    let other_prefix: usize = other
        .chars()
        .take(value[..prefix].chars().count())
        .map(char::len_utf8)
        .sum();
    let suffix: usize = value[prefix..]
        .chars()
        .rev()
        .zip(other[other_prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    (
        &value[..prefix],
        &value[prefix..value.len() - suffix],
        &value[value.len() - suffix..],
    )
}
