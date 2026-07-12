//! XLSX reader — extracts tables, text, and formulas from Excel spreadsheets.
//!
//! Reads .xlsx files (ZIP archives containing OOXML) and produces a Document AST
//! with one TableBlock per worksheet.

use latexsnipper_ast::*;
use latexsnipper_foundation::{Result, SnipperError};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

/// Parse a .xlsx file and produce a Document AST (one Page per worksheet).
pub fn read_xlsx(path: impl AsRef<Path>) -> Result<Document> {
    let file = std::fs::File::open(path.as_ref())
        .map_err(|e| SnipperError::Export(format!("Failed to open XLSX: {}", e)))?;
    read_xlsx_archive(file)
}

/// Parse XLSX package bytes using the same importer as the path API.
pub fn read_xlsx_bytes(bytes: &[u8]) -> Result<Document> {
    read_xlsx_archive(Cursor::new(bytes))
}

fn read_xlsx_archive<R: Read + Seek>(reader: R) -> Result<Document> {
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| SnipperError::Export(format!("Failed to read XLSX archive: {}", e)))?;

    // Read shared strings
    let shared_strings = read_shared_strings(&mut archive);
    let styles_xml = read_entry(&mut archive, "xl/styles.xml").unwrap_or_default();
    let date_styles = parse_date_style_indices(&styles_xml);

    // Read workbook for sheet names
    let wb_xml = read_entry(&mut archive, "xl/workbook.xml").unwrap_or_default();

    // Read relationships
    let rels_xml = read_entry(&mut archive, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let rels = parse_rels(&rels_xml);

    // Map sheet entries to actual file paths via relationships
    let mut sheet_entries = parse_workbook_sheets(&wb_xml, &rels);

    // Fallback: try sequential sheet files
    if sheet_entries.is_empty() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..50 {
            let file = format!("sheet{}.xml", i + 1);
            if !seen.insert(file.clone()) {
                continue;
            }
            let path = format!("xl/worksheets/{}", file);
            if read_entry(&mut archive, &path).is_ok() {
                sheet_entries.push((format!("Sheet {}", i + 1), file));
            } else {
                // stop at first missing file
                break;
            }
        }
    }

    let mut pages = Vec::new();

    // Read each sheet and create a page per sheet
    for (idx, (sheet_name, sheet_file)) in sheet_entries.into_iter().enumerate() {
        // sheet_file from rels might be "worksheets/sheet1.xml" (relative to xl/)
        // or "sheet1.xml" (fallback). Handle both.
        let sheet_path = if sheet_file.starts_with("worksheets/") || sheet_file.starts_with("xl/") {
            if sheet_file.starts_with("xl/") {
                sheet_file.clone()
            } else {
                format!("xl/{}", sheet_file)
            }
        } else {
            format!("xl/worksheets/{}", sheet_file)
        };
        let sheet_xml = match read_entry(&mut archive, &sheet_path) {
            Ok(x) => x,
            Err(_) => continue,
        };

        let table = parse_sheet_table(&sheet_xml, &shared_strings, &date_styles);

        let mut blocks: Vec<Block> = Vec::new();
        blocks.push(Block::Table(table));

        // Add sheet name as heading
        blocks.insert(
            0,
            Block::Heading(HeadingBlock {
                level: 2,
                inlines: vec![Inline::Text(TextRun::new(sheet_name))],
                id: None,
                geometry: None,
                source: None,
            }),
        );

        pages.push(Page {
            width: 800.0,
            height: 600.0,
            blocks,
            page_number: Some(idx as u32 + 1),
            layout: None,
            background_asset_id: None,
        });
    }

    Ok(Document {
        metadata: Metadata {
            language: None,
            created_at: None,
            ocr_model: Some("xlsx".to_string()),
            ocr_version: Some("1.0".to_string()),
            ocr_time_ms: None,
        },
        pages,
        assets: Vec::new(),
        diagnostics: Vec::new(),
        id_gen: NodeIdGenerator::new(),
        schema_version: "1.0.0".to_string(),
        notes: Vec::new(),
        outline: None,
    })
}

fn read_entry<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| SnipperError::Export(format!("Entry '{}' not found in XLSX", name)))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| SnipperError::Export(format!("Failed to read '{}': {}", name, e)))?;
    Ok(content)
}

fn read_shared_strings<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Vec<String> {
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
                if tag == b"si" {
                    in_si = true;
                    current.clear();
                }
                if tag == b"t" && in_si {
                    in_t = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_t {
                    if let Ok(t) = e.unescape() {
                        current.push_str(&t);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                if tag == b"t" {
                    in_t = false;
                }
                if tag == b"si" {
                    in_si = false;
                    strings.push(current.clone());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    strings
}

fn parse_workbook_sheets(xml: &str, rels: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut sheets = Vec::new();
    // Simple string-based parsing — find each <sheet> tag
    let mut pos = 0;
    while let Some(start) = xml[pos..].find("<sheet") {
        let tag_start = pos + start;
        if let Some(end) = xml[tag_start..].find("/>") {
            let tag = &xml[tag_start..tag_start + end + 2];
            let mut name = String::new();
            let mut rid = String::new();
            // Extract name="..."
            if let Some(ns) = tag.find("name=\"") {
                let val_start = ns + 6;
                if let Some(ve) = tag[val_start..].find('\"') {
                    name = tag[val_start..val_start + ve].to_string();
                }
            }
            // Extract r:id="..." or id="..."
            if let Some(rs) = tag.find("r:id=\"") {
                let val_start = rs + 6;
                if let Some(ve) = tag[val_start..].find('\"') {
                    rid = tag[val_start..val_start + ve].to_string();
                }
            } else if let Some(is) = tag.find("id=\"") {
                let val_start = is + 4;
                if let Some(ve) = tag[val_start..].find('\"') {
                    rid = tag[val_start..val_start + ve].to_string();
                }
            }
            if !name.is_empty() {
                let target = rels
                    .get(&rid)
                    .cloned()
                    .unwrap_or_else(|| format!("sheet{}.xml", sheets.len() + 1));
                sheets.push((name, target));
            }
            pos = tag_start + end + 2;
        } else {
            break;
        }
    }
    sheets
}

fn parse_cell_ref(ref_str: &str) -> Option<(u32, u32)> {
    let chars: Vec<char> = ref_str.chars().collect();
    let col_str: String = chars
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let row_str: String = chars
        .iter()
        .skip_while(|c| c.is_ascii_alphabetic())
        .collect();
    if col_str.is_empty() || row_str.is_empty() {
        return None;
    }
    let col = col_str
        .chars()
        .fold(0u32, |acc, c| acc * 26 + (c as u32 - 'A' as u32 + 1))
        - 1;
    let row = row_str.parse::<u32>().ok()? - 1;
    Some((col, row))
}

fn cell_ref_from_coords(col: u32, row: u32) -> String {
    let mut s = String::new();
    let mut c = col;
    loop {
        let rem = (c % 26) as u8;
        s.insert(0, (b'A' + rem) as char);
        c /= 26;
        if c == 0 {
            break;
        }
        c -= 1;
    }
    s.push_str(&(row + 1).to_string());
    s
}

fn parse_merge_range(range: &str) -> Option<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = range.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let (c1, r1) = parse_cell_ref(parts[0])?;
    let (c2, r2) = parse_cell_ref(parts[1])?;
    Some((c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2)))
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
                        if attr.key.as_ref() == b"Id" {
                            id = String::from_utf8_lossy(&attr.value).to_string();
                        }
                        if attr.key.as_ref() == b"Target" {
                            target = String::from_utf8_lossy(&attr.value).to_string();
                        }
                    }
                    if !id.is_empty() {
                        rels.insert(id, target);
                    }
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

fn parse_sheet_table(
    xml: &str,
    shared_strings: &[String],
    date_styles: &std::collections::HashSet<usize>,
) -> TableBlock {
    let mut rows: Vec<TableRow> = Vec::new();
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
    let mut in_f = false;
    let mut current_cell_formula = String::new();
    let mut current_cell_style = None;
    let mut current_row_index = 0usize;
    let mut current_row_cells: Vec<(String, String, String, String, Option<usize>)> = Vec::new();
    let mut columns: Vec<TableColumn> = Vec::new();
    let mut in_merge_cells = false;
    let mut merges: Vec<(u32, u32, u32, u32)> = Vec::new();
    let mut cell_positions: HashMap<String, (usize, usize)> = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let tag = e.name().as_ref().to_vec();
                match tag.as_slice() {
                    b"sheetData" => in_sheet_data = true,
                    b"row" if in_sheet_data => {
                        in_row = true;
                        current_row_cells.clear();
                        current_row_index = e
                            .attributes()
                            .flatten()
                            .find(|attribute| attribute.key.as_ref() == b"r")
                            .and_then(|attribute| {
                                String::from_utf8_lossy(&attribute.value)
                                    .parse::<usize>()
                                    .ok()
                            })
                            .and_then(|row| row.checked_sub(1))
                            .unwrap_or(rows.len());
                    }
                    b"c" if in_row => {
                        in_c = true;
                        current_cell_ref.clear();
                        current_cell_type.clear();
                        current_cell_value.clear();
                        current_cell_formula.clear();
                        current_cell_style = None;
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref().to_vec();
                            let v = String::from_utf8_lossy(&attr.value).to_string();
                            if k == b"r" {
                                current_cell_ref = v.clone();
                            }
                            if k == b"t" {
                                current_cell_type = v.clone();
                            }
                            if k == b"s" {
                                current_cell_style = v.parse::<usize>().ok();
                            }
                        }
                    }
                    b"v" if in_c => in_v = true,
                    b"is" if in_c => in_is = true,
                    b"t" if in_is => in_is_t = true,
                    b"f" if in_c => {
                        in_f = true;
                        current_cell_formula.clear();
                    }
                    b"cols" => {}
                    b"col" => {
                        let width = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"width")
                            .and_then(|a| String::from_utf8_lossy(&a.value).parse::<f32>().ok());
                        columns.push(TableColumn {
                            width,
                            is_header: false,
                        });
                    }
                    b"mergeCells" => in_merge_cells = true,
                    b"mergeCell" if in_merge_cells => {
                        if let Some(ref_attr) = e
                            .attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == b"ref")
                            .map(|a| String::from_utf8_lossy(&a.value).to_string())
                        {
                            if let Some((sc, sr, ec, er)) = parse_merge_range(&ref_attr) {
                                merges.push((sc, sr, ec, er));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_v {
                    if let Ok(t) = e.unescape() {
                        current_cell_value.push_str(&t);
                    }
                }
                if in_f {
                    if let Ok(t) = e.unescape() {
                        current_cell_formula.push_str(&t);
                    }
                }
                if in_is_t {
                    if let Ok(t) = e.unescape() {
                        current_is_text.push_str(&t);
                    }
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
                    b"f" => in_f = false,
                    b"c" => {
                        in_c = false;
                        if !current_cell_ref.is_empty() {
                            current_row_cells.push((
                                current_cell_ref.clone(),
                                current_cell_value.clone(),
                                current_cell_type.clone(),
                                current_cell_formula.clone(),
                                current_cell_style,
                            ));
                        }
                    }
                    b"row" => {
                        in_row = false;
                        while rows.len() < current_row_index {
                            rows.push(empty_row());
                        }
                        if !current_row_cells.is_empty() {
                            let max_col = current_row_cells
                                .iter()
                                .filter_map(|(reference, ..)| parse_cell_ref(reference))
                                .map(|(column, _)| column as usize)
                                .max()
                                .unwrap_or(0);
                            let mut cells = (0..=max_col).map(|_| empty_cell()).collect::<Vec<_>>();
                            for (reference, value, cell_type, formula, style) in &current_row_cells
                            {
                                let Some((column, _)) = parse_cell_ref(reference) else {
                                    continue;
                                };
                                let column = column as usize;
                                let is_date =
                                    style.is_some_and(|index| date_styles.contains(&index));
                                cells[column] =
                                    make_cell(value, cell_type, formula, shared_strings, is_date);
                                cell_positions
                                    .insert(reference.clone(), (current_row_index, column));
                            }
                            rows.push(TableRow {
                                cells,
                                height: None,
                                is_header: false,
                            });
                        }
                    }
                    b"sheetData" => in_sheet_data = false,
                    b"mergeCells" => in_merge_cells = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // Apply merge info: set colspan/rowspan on primary cells,
    // then remove consumed cells from their rows.
    let mut consumed: Vec<(usize, usize)> = Vec::new();
    for &(start_col, start_row, end_col, end_row) in &merges {
        let colspan = end_col - start_col + 1;
        let rowspan = end_row - start_row + 1;

        for r in start_row..=end_row {
            if r as usize >= rows.len() {
                continue;
            }
            for c in start_col..=end_col {
                let ref_str = cell_ref_from_coords(c, r);
                if let Some(&(ri, ci)) = cell_positions.get(&ref_str) {
                    if ri < rows.len() && ci < rows[ri].cells.len() {
                        if r == start_row && c == start_col {
                            rows[ri].cells[ci].colspan = colspan;
                            rows[ri].cells[ci].rowspan = rowspan;
                        } else {
                            consumed.push((ri, ci));
                        }
                    }
                }
            }
        }
    }

    // Remove consumed cells in reverse order to preserve indices
    consumed.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    consumed.dedup();
    for (ri, ci) in consumed {
        if ri < rows.len() && ci < rows[ri].cells.len() {
            rows[ri].cells.remove(ci);
        }
    }

    TableBlock {
        rows,
        columns,
        caption: None,
        style: None,
        geometry: None,
        source: None,
    }
}

fn parse_date_style_indices(xml: &str) -> std::collections::HashSet<usize> {
    let mut custom_date_formats = std::collections::HashSet::new();
    let mut date_styles = std::collections::HashSet::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut in_cell_xfs = false;
    let mut style_index = 0usize;

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => match event.name().as_ref() {
                b"numFmt" => {
                    let mut id = None;
                    let mut code = None;
                    for attribute in event.attributes().flatten() {
                        match attribute.key.as_ref() {
                            b"numFmtId" => {
                                id = String::from_utf8_lossy(&attribute.value)
                                    .parse::<u32>()
                                    .ok()
                            }
                            b"formatCode" => {
                                code = Some(String::from_utf8_lossy(&attribute.value).to_string())
                            }
                            _ => {}
                        }
                    }
                    if let (Some(id), Some(code)) = (id, code) {
                        if looks_like_date_format(&code) {
                            custom_date_formats.insert(id);
                        }
                    }
                }
                b"cellXfs" => {
                    in_cell_xfs = true;
                    style_index = 0;
                }
                b"xf" if in_cell_xfs => {
                    let num_fmt_id = event
                        .attributes()
                        .flatten()
                        .find(|attribute| attribute.key.as_ref() == b"numFmtId")
                        .and_then(|attribute| {
                            String::from_utf8_lossy(&attribute.value)
                                .parse::<u32>()
                                .ok()
                        })
                        .unwrap_or(0);
                    if is_builtin_date_format(num_fmt_id)
                        || custom_date_formats.contains(&num_fmt_id)
                    {
                        date_styles.insert(style_index);
                    }
                    style_index += 1;
                }
                _ => {}
            },
            Ok(Event::End(event)) if event.name().as_ref() == b"cellXfs" => in_cell_xfs = false,
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    date_styles
}

fn is_builtin_date_format(id: u32) -> bool {
    matches!(id, 14..=22 | 27..=36 | 45..=47 | 50..=58)
}

fn looks_like_date_format(code: &str) -> bool {
    let normalized = code.to_ascii_lowercase();
    normalized.contains('y')
        || normalized.contains('d')
        || normalized.contains("h:")
        || normalized.contains(":m")
        || normalized.contains(":s")
}

fn make_cell(
    value: &str,
    cell_type: &str,
    formula: &str,
    shared_strings: &[String],
    is_date: bool,
) -> TableCell {
    let resolved = resolve_cell_value(value, cell_type, shared_strings, is_date);
    let data_type = if !formula.is_empty() {
        CellDataType::Formula
    } else {
        match cell_type {
            "b" => CellDataType::Boolean,
            "e" => CellDataType::Error,
            "str" | "inline" | "s" => CellDataType::Text,
            _ if value.is_empty() => CellDataType::Empty,
            _ if is_date => CellDataType::Date,
            _ => CellDataType::Number,
        }
    };
    TableCell {
        content: vec![Block::Paragraph(ParagraphBlock {
            inlines: vec![Inline::Text(TextRun::new(resolved))],
            geometry: None,
            source: None,
            style: None,
        })],
        colspan: 1,
        rowspan: 1,
        data_type: Some(data_type),
        formula: (!formula.is_empty()).then(|| formula.to_string()),
        style: None,
        border_style: None,
        border_width: None,
        border_color: None,
        background: None,
        alignment: None,
        geometry: None,
        source: None,
    }
}

fn empty_cell() -> TableCell {
    make_cell("", "", "", &[], false)
}

fn empty_row() -> TableRow {
    TableRow {
        cells: Vec::new(),
        height: None,
        is_header: false,
    }
}

fn resolve_cell_value(
    value: &str,
    cell_type: &str,
    shared_strings: &[String],
    is_date: bool,
) -> String {
    match cell_type {
        "s" => {
            // Shared string: value is the index
            if let Ok(idx) = value.parse::<usize>() {
                shared_strings
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| format!("[ref {}]", idx))
            } else {
                value.to_string()
            }
        }
        "inline" | "str" => value.to_string(),
        "b" => {
            if value == "1" {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        "e" => value.to_string(),
        _ if is_date => excel_serial_to_iso(value).unwrap_or_else(|| value.to_string()),
        _ => value.to_string(),
    }
}

fn excel_serial_to_iso(value: &str) -> Option<String> {
    use chrono::{Duration, NaiveDate};

    let serial = value.parse::<f64>().ok()?;
    if !serial.is_finite() {
        return None;
    }
    let days = serial.floor() as i64;
    let seconds = ((serial - serial.floor()) * 86_400.0).round() as i64;
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    let date_time = epoch
        .checked_add_signed(Duration::days(days))?
        .checked_add_signed(Duration::seconds(seconds))?;
    if seconds == 0 {
        Some(date_time.format("%Y-%m-%d").to_string())
    } else {
        Some(date_time.format("%Y-%m-%dT%H:%M:%S").to_string())
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
        let has_table = doc
            .all_blocks()
            .iter()
            .any(|b| matches!(b, Block::Table(_)));
        assert!(has_table, "should contain a table block");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn sparse_error_and_date_cells_keep_types_and_coordinates() {
        let styles = r#"<styleSheet><cellXfs count="2">
          <xf numFmtId="0"/><xf numFmtId="14"/>
        </cellXfs></styleSheet>"#;
        let date_styles = parse_date_style_indices(styles);
        let sheet = r#"<worksheet><sheetData>
          <row r="2">
            <c r="C2" t="e"><v>#DIV/0!</v></c>
            <c r="E2" s="1"><v>2</v></c>
          </row>
        </sheetData></worksheet>"#;
        let table = parse_sheet_table(sheet, &[], &date_styles);

        assert_eq!(table.rows.len(), 2, "missing first row must remain sparse");
        assert!(table.rows[0].cells.is_empty());
        assert_eq!(table.rows[1].cells.len(), 5, "E2 must remain in column E");
        assert_eq!(table.rows[1].cells[0].data_type, Some(CellDataType::Empty));
        assert_eq!(table.rows[1].cells[2].data_type, Some(CellDataType::Error));
        assert_eq!(table.rows[1].cells[4].data_type, Some(CellDataType::Date));
        assert_eq!(cell_text(&table.rows[1].cells[2]), "#DIV/0!");
        assert_eq!(cell_text(&table.rows[1].cells[4]), "1900-01-01");
    }

    fn cell_text(cell: &TableCell) -> String {
        cell.content
            .iter()
            .filter_map(|block| match block {
                Block::Paragraph(paragraph) => Some(
                    paragraph
                        .inlines
                        .iter()
                        .filter_map(|inline| match inline {
                            Inline::Text(text) => Some(text.text.as_str()),
                            _ => None,
                        })
                        .collect::<String>(),
                ),
                _ => None,
            })
            .collect()
    }
}
