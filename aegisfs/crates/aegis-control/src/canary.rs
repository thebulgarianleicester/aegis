use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Canary {
    pub path: PathBuf,
    pub format: String,
    pub blake3: String,
    pub hidden: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct CanaryManager {
    root: PathBuf,
}

impl CanaryManager {
    pub fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize().context("resolve canary root")?;
        if !root.is_dir() {
            bail!("canary root must be a directory");
        }
        Ok(Self { root })
    }

    /// Deploys deterministic, structurally valid PDF and OOXML decoys. Existing
    /// canaries are accepted only when their bytes still match; user files are
    /// never overwritten.
    pub fn deploy(&self) -> Result<Vec<Canary>> {
        let docx = zip(&[
            ("[Content_Types].xml", br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.as_slice()),
            ("_rels/.rels", br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.as_slice()),
            ("word/document.xml", br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>AegisFS canary - do not modify</w:t></w:r></w:p></w:body></w:document>"#.as_slice()),
        ]);
        let xlsx = zip(&[
            ("[Content_Types].xml", br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.as_slice()),
            ("_rels/.rels", br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.as_slice()),
            ("xl/workbook.xml", br#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Financials" sheetId="1" r:id="rId1"/></sheets></workbook>"#.as_slice()),
            ("xl/_rels/workbook.xml.rels", br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.as_slice()),
            ("xl/worksheets/sheet1.xml", br#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Quarter</t></is></c><c r="B1" t="inlineStr"><is><t>Revenue</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Q4</t></is></c><c r="B2"><v>0</v></c></row></sheetData></worksheet>"#.as_slice()),
        ]);
        self.deploy_files(&[
            ("!00_financials.xlsx", "OOXML spreadsheet", xlsx),
            ("!01_board_minutes.pdf", "PDF document", pdf()),
            (".aegis_tripwire.docx", "OOXML document", docx),
        ])
    }

    pub fn inspect(&self) -> Result<Vec<Canary>> {
        self.deploy()
    }

    pub fn is_canary(&self, path: &Path) -> bool {
        matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("!00_financials.xlsx" | "!01_board_minutes.pdf" | ".aegis_tripwire.docx")
        )
    }

    fn deploy_files(&self, files: &[(&str, &str, Vec<u8>)]) -> Result<Vec<Canary>> {
        let mut deployed = Vec::new();
        for (name, format, expected) in files {
            let path = self.root.join(name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(expected)?;
                    file.sync_all()?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let actual = fs::read(&path).with_context(|| {
                        format!("read existing canary {}", path.display())
                    })?;
                    if actual != *expected {
                        bail!("refusing to overwrite modified or user-owned canary {name}");
                    }
                }
                Err(error) => return Err(error.into()),
            }
            deployed.push(Canary {
                path,
                format: (*format).to_owned(),
                blake3: blake3::hash(expected).to_hex().to_string(),
                hidden: name.starts_with('.'),
                bytes: expected.len() as u64,
            });
        }
        Ok(deployed)
    }
}

fn pdf() -> Vec<u8> {
    let objects = [
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        "5 0 obj\n<< /Length 62 >>\nstream\nBT /F1 14 Tf 72 720 Td (AegisFS canary - do not modify) Tj ET\nendstream\nendobj\n",
    ];
    let mut output = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for object in objects {
        offsets.push(output.len());
        output.extend_from_slice(object.as_bytes());
    }
    let xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    output.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    output
}

/// Minimal ZIP writer using stored (uncompressed) entries. OOXML remains a
/// valid structured ZIP container even though its XML members are stored.
fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    struct Central {
        name: Vec<u8>,
        crc: u32,
        size: u32,
        offset: u32,
    }
    let mut output = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in entries {
        let name = name.as_bytes().to_vec();
        let crc = crc32(bytes);
        let offset = output.len() as u32;
        let size = bytes.len() as u32;
        put_u32(&mut output, 0x0403_4b50);
        put_u16(&mut output, 20);
        put_u16(&mut output, 0x0800); // UTF-8 names
        put_u16(&mut output, 0); // stored
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, crc);
        put_u32(&mut output, size);
        put_u32(&mut output, size);
        put_u16(&mut output, name.len() as u16);
        put_u16(&mut output, 0);
        output.extend_from_slice(&name);
        output.extend_from_slice(bytes);
        central.push(Central {
            name,
            crc,
            size,
            offset,
        });
    }
    let central_offset = output.len() as u32;
    for entry in &central {
        put_u32(&mut output, 0x0201_4b50);
        put_u16(&mut output, 20);
        put_u16(&mut output, 20);
        put_u16(&mut output, 0x0800);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, entry.crc);
        put_u32(&mut output, entry.size);
        put_u32(&mut output, entry.size);
        put_u16(&mut output, entry.name.len() as u16);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u16(&mut output, 0);
        put_u32(&mut output, 0);
        put_u32(&mut output, entry.offset);
        output.extend_from_slice(&entry.name);
    }
    let central_size = output.len() as u32 - central_offset;
    put_u32(&mut output, 0x0605_4b50);
    put_u16(&mut output, 0);
    put_u16(&mut output, 0);
    put_u16(&mut output, central.len() as u16);
    put_u16(&mut output, central.len() as u16);
    put_u32(&mut output, central_size);
    put_u32(&mut output, central_offset);
    put_u16(&mut output, 0);
    output
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canaries_have_expected_structure_and_stable_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CanaryManager::new(temp.path()).unwrap();
        let deployed = manager.deploy().unwrap();
        assert_eq!(deployed.len(), 3);
        assert_eq!(manager.inspect().unwrap(), deployed);
        assert!(fs::read(temp.path().join("!00_financials.xlsx"))
            .unwrap()
            .starts_with(b"PK\x03\x04"));
        assert!(fs::read(temp.path().join("!01_board_minutes.pdf"))
            .unwrap()
            .starts_with(b"%PDF-"));
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn modified_canary_is_not_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let manager = CanaryManager::new(temp.path()).unwrap();
        manager.deploy().unwrap();
        fs::write(temp.path().join("!00_financials.xlsx"), b"user data").unwrap();
        assert!(manager.inspect().is_err());
        assert_eq!(
            fs::read(temp.path().join("!00_financials.xlsx")).unwrap(),
            b"user data"
        );
    }
}
