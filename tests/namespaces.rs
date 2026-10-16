use diff_excel::{
    compare::{compare, Options},
    workbook::{match_all, Workbook},
};
use quick_xml::{
    events::{BytesEnd, BytesStart, Event},
    Reader, Writer,
};
use std::{
    fs::File,
    io::{Read, Write},
};
use zip_data::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[test]
fn reads_prefixed_workbooks_and_parts_and_compares_without_changing_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().join("base.xlsx");
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .cell_mut("A1")
        .set_value("<x:sheet> 中文 & xmlns:x=内容");
    sheet.cell_mut("B2").set_value_number(42);
    sheet.cell_mut("C3").set_formula("B2*2");
    sheet.style_mut("A1").font_mut().set_bold(true);
    sheet.add_merge_cells("D4:E4");
    umya_spreadsheet::writer::xlsx::write(&book, &base).unwrap();
    for mode in ["all", "workbook", "parts"] {
        let prefixed = dir.path().join(format!("{mode}.xlsx"));
        let mut input = ZipArchive::new(File::open(&base).unwrap()).unwrap();
        let mut output = ZipWriter::new(File::create(&prefixed).unwrap());
        for index in 0..input.len() {
            let mut part = input.by_index(index).unwrap();
            let name = part.name().unwrap().into_owned();
            let mut data = Vec::new();
            part.read_to_end(&mut data).unwrap();
            let selected = mode == "all" || (mode == "workbook") == (name == "xl/workbook.xml");
            if selected
                && name.ends_with(".xml")
                && String::from_utf8_lossy(&data)
                    .contains("xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"")
            {
                let mut reader = Reader::from_reader(data.as_slice());
                let mut writer = Writer::new(Vec::new());
                loop {
                    let event = reader.read_event().unwrap();
                    match event {
                        Event::Start(ref element) | Event::Empty(ref element) => {
                            let name = format!(
                                "ss:{}",
                                std::str::from_utf8(element.name().as_ref()).unwrap()
                            );
                            let mut adapted = BytesStart::new(name);
                            for attribute in element.attributes() {
                                adapted.push_attribute(attribute.unwrap());
                            }
                            adapted.push_attribute((
                                "xmlns:ss",
                                "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
                            ));
                            writer
                                .write_event(if matches!(event, Event::Start(_)) {
                                    Event::Start(adapted)
                                } else {
                                    Event::Empty(adapted)
                                })
                                .unwrap();
                        }
                        Event::End(element) => {
                            writer
                                .write_event(Event::End(BytesEnd::new(format!(
                                    "ss:{}",
                                    std::str::from_utf8(element.name().as_ref()).unwrap()
                                ))))
                                .unwrap();
                        }
                        Event::Eof => break,
                        event => {
                            writer.write_event(event).unwrap();
                        }
                    }
                }
                data = writer.into_inner();
            }
            output
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            output.write_all(&data).unwrap();
        }
        output.finish().unwrap();
        let before = std::fs::read(&prefixed).unwrap();
        let old = Workbook::load(&base).unwrap();
        let new = Workbook::load(&prefixed).unwrap();
        assert_eq!(new.sheets(), vec!["Sheet1"]);
        let sheet = new.book.sheet(0).unwrap();
        assert_eq!(sheet.value("A1"), "<x:sheet> 中文 & xmlns:x=内容");
        assert_eq!(sheet.cell("C3").unwrap().formula(), "B2*2");
        assert!(sheet.style("A1").font().unwrap().bold());
        assert_eq!(sheet.merge_cells().len(), 1);
        let report = compare(
            &old,
            &new,
            &match_all(&old.sheets(), &new.sheets()),
            &Options {
                output: dir.path().join("reports"),
                keep_format: true,
                comments: true,
                color: "#FFF0BD".into(),
            },
            |_| {},
        )
        .unwrap();
        assert_eq!(report.total, 0);
        assert_eq!(std::fs::read(&prefixed).unwrap(), before);
    }
}
