//! XLSX reader — extracts tables, text, and formulas from Excel spreadsheets.
//!
//! Reads .xlsx files (ZIP archives containing OOXML) and produces a Document AST
//! with one TableBlock per worksheet.

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// Parse a .xlsx file and produce a Document AST (one Page per worksheet).
pub fn read_xlsx(path: impl AsRef<Path>) -> Result<Document> {
    let file = std::fs::File::open(path.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to open XLSX: {}", e)))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SnipperError::Export(format!("Failed to read XLSX archive: {}", e)))?;

    // Read shared strings
    let shared_strings = read_shared_strings(&mut archive);

    // Read workbook for sheet names
    let wb_xml = read_entry(&mut archive, "xl/workbook.xml").unwrap_or_default();
    let sheet_names = parse_sheet_names(&wb_xml);

    // Read relationships
    let rels_xml = read_entry(&mut archive, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let _rels = parse_rels(&rels_xml);

    let mut pages = Vec::new();

    // Find all sheet XML files
    for sheet_idx in 1..=50 {
        let sheet_file = format!("xl/worksheets/sheet{}.xml", sheet_idx);
        let sheet_xml = match read_entry(&mut archive, &sheet_file) {
            Ok(x) => x,
            Err(_) => {
                if sheet_idx > 1 { break; }
                continue;
            }
        };

        let sheet_name = sheet_names.get(&sheet_idx)
            .cloned()
            .unwrap_or_else(|| format!("Sheet{}", sheet_idx));

        let table = parse_sheet_table(&sheet_xml, &shared_strings);

        let mut blocks: Vec<Block> = Vec::new();
        blocks.push(Block::Table(table));

        // Add sheet name as heading
        blocks.insert(0, Block::Heading(HeadingBlock {
            level: 2,
            inlines: vec![Inline::Text(TextRun::new(sheet_name))],
            id: None, geometry: None, source: None,
        }));

        pages.push(Page {
            width: 800.0, height: 600.0,
            blocks,
            page_number: Some(sheet_idx as u32),
        });
    }

    Ok(Document {
        metadata: Metadata {
            language: None, created_at: None,
            ocr_model: Some("xlsx".to_string()), ocr_version: Some("1.0".to_string()),
            ocr_time_ms: None,
        },
        pages,
        assets: Vec::new(), diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
    })
}

fn read_entry(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<String> {
    let mut file = archive.by_name(name)
        .map_err(|_| SnipperError::Export(format!("Entry '{}' not found in XLSX", name)))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| SnipperError::Export(format!("Failed to read '{}': {}", name, e)))?;
    Ok(content)
}

fn read_shared_strings(archive: &mut zip::ZipArchive<std::fs::File>) -> Vec<String> {
    let xml = match read_entry(archive, "xl/sharedStrings.xml") {
        Ok(x) => x,
        Err(_) => return Vec::new(),
    };

    let mut strings = Vec::new();
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_si = false;
    let mut in_t = false;
    let mut current = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                if tag == b"si" { in_si = true; current.clear(); }
                if tag == b"t" && in_si { in_t = true; }
            }
            Ok(Event::Text(ref e)) => {
                if in_t { if let Ok(t) = e.unescape() { current.push_str(&t); } }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                if tag == b"t" { in_t = false; }
                if tag == b"si" { in_si = false; strings.push(current.clone()); }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    strings
}

fn parse_sheet_names(xml: &str) -> HashMap<usize, String> {
    let mut names = HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut sheet_idx = 0usize;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"sheet" {
                    sheet_idx += 1;
                    let mut name = String::new();
                    for attr in e.attributes().flatten() {
                        let k = attr.key.as_ref().to_vec();
                        if k == b"name" {
                            name = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                    if !name.is_empty() {
                        // sheetId attribute is the 1-based index
                        let id = e.attributes().flatten()
                            .find(|a| a.key.as_ref() == b"sheetId")
                            .and_then(|a| String::from_utf8_lossy(&a.value).parse::<usize>().ok())
                            .unwrap_or(sheet_idx);
                        names.insert(id, name);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    names
}

fn parse_rels(xml: &str) -> HashMap<String, String> {
    let mut rels = HashMap::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"Relationship" {
                    let mut id = String::new();
                    let mut target = String::new();
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"Id" { id = String::from_utf8_lossy(&attr.value).to_string(); }
                        if attr.key.as_ref() == b"Target" { target = String::from_utf8_lossy(&attr.value).to_string(); }
                    }
                    if !id.is_empty() { rels.insert(id, target); }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rels
}

fn parse_sheet_table(xml: &str, shared_strings: &[String]) -> TableBlock {
    let mut rows: Vec<Vec<TableCell>> = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_sheet_data = false;
    let mut in_row = false;
    let mut in_c = false;
    let mut in_v = false;
    let mut in_is = false;
    let mut in_is_t = false;
    let mut current_cell_ref = String::new();
    let mut current_cell_type = String::new();
    let mut current_cell_value = String::new();
    let mut current_is_text = String::new();
    let mut current_row_cells: Vec<(String, String, String)> = Vec::new(); // (ref, value, type)

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"sheetData" => in_sheet_data = true,
                    b"row" if in_sheet_data => {
                        in_row = true;
                        current_row_cells.clear();
                    }
                    b"c" if in_row => {
                        in_c = true;
                        current_cell_ref.clear();
                        current_cell_type.clear();
                        current_cell_value.clear();
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_vec();
                            let v = String::from_utf8_lossy(&attr.value).to_string();
                            if k == b"r" { current_cell_ref = v.clone(); }
                            if k == b"t" { current_cell_type = v.clone(); }
                            if k == b"s" { current_cell_type = v; }
                        }
                    }
                    b"v" if in_c => in_v = true,
                    b"is" if in_c => in_is = true,
                    b"t" if in_is => in_is_t = true,
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_v {
                    if let Ok(t) = e.unescape() { current_cell_value.push_str(&t); }
                }
                if in_is_t {
                    if let Ok(t) = e.unescape() { current_is_text.push_str(&t); }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"t" if in_is => in_is_t = false,
                    b"is" => {
                        in_is = false;
                        // Inline string: use the text from <is><t>...</t></is>
                        if !current_is_text.is_empty() {
                            current_cell_value = current_is_text.clone();
                            current_cell_type = "inline".to_string();
                        }
                        current_is_text.clear();
                    }
                    b"v" => in_v = false,
                    b"c" => {
                        in_c = false;
                        if !current_cell_ref.is_empty() {
                            current_row_cells.push((
                                current_cell_ref.clone(),
                                current_cell_value.clone(),
                                current_cell_type.clone(),
                            ));
                        }
                    }
                    b"row" => {
                        in_row = false;
                        // Build row of cells
                        let table_row: Vec<TableCell> = current_row_cells.iter().map(|(_, val, typ)| {
                            let resolved = resolve_cell_value(val, typ, shared_strings);
                            TableCell {
                                inlines: vec![Inline::Text(TextRun::new(resolved))],
                                colspan: 1, rowspan: 1,
                                border_style: None, border_width: None, border_color: None,
                                background: None, alignment: None,
                                geometry: None, source: None,
                            }
                        }).collect();

                        // Fill gaps for proper column alignment
                        if !table_row.is_empty() {
                            rows.push(table_row);
                        }
                    }
                    b"sheetData" => in_sheet_data = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    TableBlock { rows, geometry: None, source: None }
}

fn resolve_cell_value(value: &str, cell_type: &str, shared_strings: &[String]) -> String {
    match cell_type {
        "s" => {
            // Shared string: value is the index
            if let Ok(idx) = value.parse::<usize>() {
                shared_strings.get(idx).cloned().unwrap_or_else(|| format!("[ref {}]", idx))
            } else {
                value.to_string()
            }
        }
        "inline" | "str" => value.to_string(),
        "b" => if value == "1" { "TRUE".to_string() } else { "FALSE".to_string() },
        "e" => format!("={}", value),
        _ => value.to_string(), // number or other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_minimal_xlsx(suffix: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("test_xlsx_{}_{}.xlsx", suffix, std::process::id()));
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = || zip::write::FileOptions::default();

        zip.start_file("[Content_Types].xml", opts()).unwrap();
        write!(zip, r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
</Types>"#).unwrap();

        zip.add_directory("xl/", opts()).unwrap();
        zip.add_directory("xl/worksheets/", opts()).unwrap();

        zip.start_file("xl/workbook.xml", opts()).unwrap();
        write!(zip, r#"<?xml version="1.0"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheets><sheet name="Sheet1" sheetId="1" r:id="rId1" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"/></sheets>
</workbook>"#).unwrap();

        zip.start_file("xl/worksheets/sheet1.xml", opts()).unwrap();
        write!(zip, r#"<?xml version="1.0"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1"><c r="A1" t="inline"><is><t>Hello</t></is></c><c r="B1" t="inline"><is><t>World</t></is></c></row>
    <row r="2"><c r="A2" t="inline"><is><t>42</t></is></c><c r="B2" t="inline"><is><t>3.14</t></is></c></row>
  </sheetData>
</worksheet>"#).unwrap();

        zip.finish().unwrap();
        path
    }

    #[test]
    fn test_read_xlsx_simple() {
        let path = create_minimal_xlsx("p");
        let doc = read_xlsx(&path).unwrap();
        assert!(!doc.pages.is_empty(), "should have at least one sheet");
        let has_table = doc.all_blocks().iter().any(|b| matches!(b, Block::Table(_)));
        assert!(has_table, "should contain a table block");
        std::fs::remove_file(&path).ok();
    }
}
