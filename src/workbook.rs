use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use umya_spreadsheet::Workbook as XlsxWorkbook;

#[derive(Clone, Debug)]
pub struct Workbook {
    pub path: PathBuf,
    pub book: XlsxWorkbook,
    pub empty: bool,
}

impl Workbook {
    pub fn load(path: &Path) -> Result<Self> {
        // Git uses a null device for an absent side of an added/deleted file.
        let null = path == Path::new("/dev/null")
            || path
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("NUL");
        let path = if null || path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()?.join(path)
        };
        if null {
            return Ok(Self {
                path,
                book: umya_spreadsheet::new_file_empty_worksheet(),
                empty: true,
            });
        }
        let metadata = std::fs::metadata(&path)
            .with_context(|| format!("无法读取文件：{}", path.display()))?;
        if !metadata.is_file() {
            bail!("请选择文件而非目录：{}", path.display());
        }
        // Git clients can also pass a zero-byte temporary file for an absent side.
        if metadata.len() == 0 {
            return Ok(Self {
                path,
                book: umya_spreadsheet::new_file_empty_worksheet(),
                empty: true,
            });
        }
        if !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx"))
        {
            bail!("目前支持 .xlsx 文件：{}", path.display());
        }
        let adapted = crate::xml_namespace::prepare(&path)
            .with_context(|| format!("无法解析 Excel XML：{}", path.display()))?;
        let source = adapted.as_ref().map_or(path.as_path(), |file| file.path());
        let book = umya_spreadsheet::reader::xlsx::read(source).with_context(|| {
            format!(
                "无法打开 Excel（请确认文件未加密且格式有效）：{}",
                path.display()
            )
        })?;
        if book.sheet_count() == 0 {
            bail!(
                "工作簿中没有可读取的工作表，请检查格式或在 Excel 中另存为 .xlsx：{}",
                path.display()
            );
        }
        Ok(Self {
            path,
            book,
            empty: false,
        })
    }

    pub fn sheets(&self) -> Vec<String> {
        self.book
            .sheet_collection()
            .iter()
            .map(|s| s.name().to_owned())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetPair {
    pub old: Option<String>,
    pub new: Option<String>,
}

impl SheetPair {
    pub fn label(&self) -> String {
        format!(
            "{} → {}",
            self.old.as_deref().unwrap_or("（新增）"),
            self.new.as_deref().unwrap_or("（删除）")
        )
    }
}

/// Stable workbook order; never pair unrelated sheets by their position.
pub fn match_all(old: &[String], new: &[String]) -> Vec<SheetPair> {
    let mut pairs: Vec<_> = old
        .iter()
        .map(|name| SheetPair {
            old: Some(name.clone()),
            new: new.contains(name).then(|| name.clone()),
        })
        .collect();
    pairs.extend(
        new.iter()
            .filter(|name| !old.contains(name))
            .map(|name| SheetPair {
                old: None,
                new: Some(name.clone()),
            }),
    );
    pairs
}

pub fn safe_sheet_name(base: &str, used: &mut HashSet<String>) -> String {
    let clean: String = base
        .chars()
        .map(|c| {
            if "[]:*?/\\".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_matches('\'');
    let clean = if clean.is_empty() { "Sheet" } else { clean };
    for n in 0.. {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!(" ({n})")
        };
        // Excel's limit is UTF-16 code units, not UTF-8 bytes.
        let mut name = String::new();
        let mut length = suffix.encode_utf16().count();
        for c in clean.chars() {
            length += c.len_utf16();
            if length > 31 {
                break;
            }
            name.push(c);
        }
        name.push_str(&suffix);
        if used.insert(name.to_lowercase()) {
            return name;
        }
    }
    unreachable!()
}
