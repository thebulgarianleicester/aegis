/// Signature evidence, not full file-format validation.
pub fn signature(bytes: &[u8]) -> Option<&'static str> {
    const SIGNATURES: &[(&[u8], &str)] = &[
        (b"%PDF-", "PDF"),
        (b"PK\x03\x04", "ZIP/OOXML"),
        (b"\x89PNG\r\n\x1a\n", "PNG"),
        (b"\xff\xd8\xff", "JPEG"),
        (b"SQLite format 3\0", "SQLite"),
        (b"\x7fELF", "ELF"),
        (b"\x1f\x8b\x08", "gzip"),
        (b"\x28\xb5\x2f\xfd", "zstd"),
        (b"GIF87a", "GIF"),
        (b"GIF89a", "GIF"),
    ];
    SIGNATURES
        .iter()
        .find_map(|(magic, name)| bytes.starts_with(magic).then_some(*name))
}
