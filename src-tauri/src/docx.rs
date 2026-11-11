//! Writes text as a Word document: Markdown headings and list items become headings and
//! bullets, every other line a paragraph.

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

fn escaped(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// One line as a paragraph. `size` is in half points; 0 leaves the default.
fn paragraph(text: &str, bold: bool, size: u32) -> String {
    let mut look = String::new();
    if bold {
        look.push_str("<w:b/>");
    }
    if size > 0 {
        look.push_str(&format!(
            "<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
        ));
    }
    format!(
        "<w:p><w:r><w:rPr>{look}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escaped(text)
    )
}

/// The document's main part.
pub fn document_xml(text: &str) -> String {
    let body: String = text
        .lines()
        .map(|line| {
            let line = line.trim_end();
            if let Some(title) = line.strip_prefix("# ") {
                paragraph(title, true, 36)
            } else if let Some(title) = line.strip_prefix("## ") {
                paragraph(title, true, 28)
            } else if let Some(title) = line.strip_prefix("### ") {
                paragraph(title, true, 24)
            } else if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
                paragraph(&format!("• {item}"), false, 0)
            } else {
                paragraph(line, false, 0)
            }
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}</w:body></w:document>"
    )
}

/// A zip archive holding `files` uncompressed.
fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, data) in files {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let offset = out.len() as u32;
        let size = data.len() as u32;
        // Version 2.0, UTF-8 names, stored, no time, then checksum, sizes and name length.
        let mut common = Vec::new();
        common.extend_from_slice(&[20, 0, 0, 8, 0, 0, 0, 0, 0x21, 0]);
        common.extend_from_slice(&crc.sum().to_le_bytes());
        common.extend_from_slice(&size.to_le_bytes());
        common.extend_from_slice(&size.to_le_bytes());
        common.extend_from_slice(&(name.len() as u16).to_le_bytes());
        common.extend_from_slice(&[0, 0]);
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&common);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        directory.extend_from_slice(b"PK\x01\x02");
        directory.extend_from_slice(&[20, 0]);
        directory.extend_from_slice(&common);
        directory.extend_from_slice(&[0; 10]);
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&directory);
    out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(directory.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

/// `text` as the bytes of a .docx file.
pub fn docx(text: &str) -> Vec<u8> {
    zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("word/document.xml", document_xml(text).as_bytes()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_bullets_and_plain_lines_become_paragraphs() {
        let xml = document_xml("# 会议纪要\n## 结论\n- 预算 <20%> & 审批\n普通一行");
        assert_eq!(xml.matches("<w:p>").count(), 4);
        assert!(xml.contains("<w:b/><w:sz w:val=\"36\"/><w:szCs w:val=\"36\"/></w:rPr><w:t xml:space=\"preserve\">会议纪要<"));
        assert!(xml.contains("<w:sz w:val=\"28\"/>"));
        assert!(xml.contains(">• 预算 &lt;20%&gt; &amp; 审批<"));
        assert!(xml.contains("<w:rPr></w:rPr><w:t xml:space=\"preserve\">普通一行<"));
    }

    /// Writes a document to `OT_OUT` for opening with other programs.
    #[test]
    #[ignore]
    fn writes_a_document_to_disk() {
        let text = "# 会议纪要\n## 结论\n- 周五上线 & 压测 <两轮>\n普通一行";
        std::fs::write(std::env::var("OT_OUT").unwrap(), docx(text)).unwrap();
    }

    #[test]
    fn the_archive_lists_every_file_with_its_size_and_place() {
        let bytes = zip(&[("a.txt", b"hello"), ("b/c.txt", b"hi")]);
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let end = bytes.len() - 22;
        assert_eq!(&bytes[end..end + 4], b"PK\x05\x06");
        assert_eq!(u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]), 2);
        let start = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
        assert_eq!(&bytes[start..start + 4], b"PK\x01\x02");
        // The second entry's record says where its data starts.
        let second = start + 46 + "a.txt".len();
        let offset =
            u32::from_le_bytes(bytes[second + 42..second + 46].try_into().unwrap()) as usize;
        assert_eq!(&bytes[offset..offset + 4], b"PK\x03\x04");
        assert_eq!(&bytes[offset + 30..offset + 37], b"b/c.txt");
        assert_eq!(&bytes[offset + 37..offset + 39], b"hi");
    }
}
