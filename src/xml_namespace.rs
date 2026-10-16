//! Adapt legal prefixed SpreadsheetML to the unqualified names expected by umya.
//! Never modify the source ZIP, XML text, formulas or namespaced attributes.
use anyhow::{bail, Context, Result};
use quick_xml::{
    events::{attributes::Attribute, BytesEnd, BytesStart, Event},
    name::{QName, ResolveResult},
    NsReader, Writer,
};
use std::{borrow::Cow, fs::File, io::Read, path::Path};
use tempfile::NamedTempFile;
use zip_data::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const MAIN: &[u8] = b"http://schemas.openxmlformats.org/spreadsheetml/2006/main";

pub(crate) fn prepare(path: &Path) -> Result<Option<NamedTempFile>> {
    let mut source = ZipArchive::new(File::open(path)?)?;
    let mut temporary = NamedTempFile::new()?;
    let mut output = ZipWriter::new(temporary.as_file_mut());
    let mut changed = false;
    for index in 0..source.len() {
        let mut part = source.by_index(index)?;
        let name = part.name()?.into_owned();
        let replacement = if name.ends_with(".xml") {
            let mut data = Vec::new();
            part.read_to_end(&mut data)?;
            normalize(&data).with_context(|| format!("XML 部件 {name}"))?
        } else {
            None
        };
        if let Some(data) = replacement {
            changed = true;
            output.start_file(
                &name,
                SimpleFileOptions::default().compression_method(part.compression()),
            )?;
            std::io::Write::write_all(&mut output, &data)?;
        } else {
            // Reading XML above advances the ZIP stream. Reopen before copying.
            drop(part);
            output.raw_copy_file(source.by_index(index)?)?;
        }
    }
    output.finish()?;
    Ok(changed.then_some(temporary))
}

fn normalize(data: &[u8]) -> Result<Option<Vec<u8>>> {
    // This is only a fast rejection, never a text replacement. Namespace scope
    // and actual element identity are resolved by the XML reader below.
    if !data.windows(6).any(|s| s == b"xmlns:") || !data.windows(MAIN.len()).any(|s| s == MAIN) {
        return Ok(None);
    }
    let mut reader = NsReader::from_reader(data);
    let mut writer = Writer::new(Vec::with_capacity(data.len()));
    let mut stack: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut changed = false;
    loop {
        let (namespace, event) = reader.read_resolved_event()?;
        match event {
            Event::Start(ref element) | Event::Empty(ref element) => {
                let uri = match namespace {
                    ResolveResult::Bound(uri) => uri.as_ref().to_vec(),
                    ResolveResult::Unbound => Vec::new(),
                    ResolveResult::Unknown(prefix) => bail!(
                        "未声明的 XML 命名空间前缀：{}",
                        String::from_utf8_lossy(&prefix)
                    ),
                };
                let original = element.name();
                let name = if uri == MAIN {
                    element.local_name().as_ref().to_vec()
                } else {
                    original.as_ref().to_vec()
                };
                changed |= name != original.as_ref();
                let mut default = stack.last().map_or_else(Vec::new, |(_, ns)| ns.clone());
                for attribute in element.attributes() {
                    let attribute = attribute?;
                    if attribute.key.as_ref() == b"xmlns" {
                        default = attribute.value.into_owned();
                    }
                }
                // Removing a prefix changes how descendants inherit the default
                // namespace. Restore foreign/empty defaults at their boundaries.
                let override_default = !name.contains(&b':') && default != uri;
                if override_default {
                    default = uri;
                }
                let mut adapted = BytesStart::new(std::str::from_utf8(&name)?);
                for attribute in element.attributes() {
                    let attribute = attribute?;
                    if !override_default || attribute.key.as_ref() != b"xmlns" {
                        adapted.push_attribute(attribute);
                    }
                }
                if override_default {
                    adapted.push_attribute(Attribute {
                        key: QName(b"xmlns"),
                        value: Cow::Borrowed(&default),
                    });
                }
                if matches!(event, Event::Start(_)) {
                    writer.write_event(Event::Start(adapted))?;
                    stack.push((name, default));
                } else {
                    writer.write_event(Event::Empty(adapted))?;
                }
            }
            Event::End(_) => {
                let (name, _) = stack.pop().context("XML 结束标签没有开始标签")?;
                writer.write_event(Event::End(BytesEnd::new(std::str::from_utf8(&name)?)))?;
            }
            Event::Eof => {
                if !stack.is_empty() {
                    bail!("XML 包含未关闭的标签");
                }
                break;
            }
            other => writer.write_event(other)?,
        }
    }
    Ok(changed.then(|| writer.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elements(data: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut reader = NsReader::from_reader(data);
        let mut result = Vec::new();
        loop {
            match reader.read_resolved_event().unwrap() {
                (ns, Event::Start(e) | Event::Empty(e)) => {
                    let uri = match ns {
                        ResolveResult::Bound(ns) => ns.as_ref().to_vec(),
                        ResolveResult::Unbound => Vec::new(),
                        _ => panic!("unresolved namespace"),
                    };
                    result.push((uri, e.local_name().as_ref().to_vec()));
                }
                (_, Event::Eof) => break,
                _ => (),
            }
        }
        result
    }

    #[test]
    fn resolves_aliases_and_preserves_rebound_and_foreign_namespaces() {
        for prefix in ["x", "spreadsheet", "工作表"] {
            let source = format!(
                r#"<?xml version="1.0"?><{prefix}:worksheet xmlns:{prefix}="{}" xmlns="urn:foreign" xmlns:r="urn:relations"><{prefix}:sheetData><{prefix}:row r:id="x:sheet"><{prefix}:c><{prefix}:f><![CDATA[IF(A1="x:sheet",1,0)]]></{prefix}:f><{prefix}:v>&lt;x:row&gt;&amp;</{prefix}:v></{prefix}:c><plain/><{prefix}:c xmlns:{prefix}="urn:rebound"/><inner xmlns=""><empty/></inner></{prefix}:row></{prefix}:sheetData><!-- x:worksheet --></{prefix}:worksheet>"#,
                std::str::from_utf8(MAIN).unwrap()
            );
            let result = normalize(source.as_bytes()).unwrap().unwrap();
            assert_eq!(elements(source.as_bytes()), elements(&result));
            let result = String::from_utf8(result).unwrap();
            assert!(result.contains("r:id=\"x:sheet\""));
            assert!(result.contains("<![CDATA[IF(A1=\"x:sheet\",1,0)]]>"));
            assert!(result.contains("&lt;x:row&gt;&amp;"));
            assert!(result.contains("<!-- x:worksheet -->"));
        }
    }

    #[test]
    fn unchanged_xml_and_invalid_xml_are_not_silently_rewritten() {
        let ordinary = format!(
            r#"<worksheet xmlns="{}" xmlns:r="urn:r"><c>xmlns:x=&quot;x:sheet&quot;</c></worksheet>"#,
            std::str::from_utf8(MAIN).unwrap()
        );
        assert!(normalize(ordinary.as_bytes()).unwrap().is_none());
        let invalid = format!(
            r#"<x:worksheet xmlns:x="{}"><missing:c/></x:worksheet>"#,
            std::str::from_utf8(MAIN).unwrap()
        );
        assert!(normalize(invalid.as_bytes()).is_err());
    }
}
