#[test]
fn snapshot_header_preflight_is_bounded_and_accepts_a_leading_prefix() -> std::io::Result<()> {
    use super::has_pdf_header;
    use std::io::Cursor;

    for offset in [0, 16, 1023, 1024] {
        let mut bytes = vec![b' '; offset];
        bytes.extend_from_slice(b"%PDF-1.7\n");
        bytes.extend_from_slice(&[0; 4096]);
        let mut reader = Cursor::new(bytes);
        assert_eq!(has_pdf_header(&mut reader)?, offset < 1024);
        assert_eq!(reader.position(), 1028, "only the bounded prefix is read");
    }
    for bytes in [b"This is not a PDF".as_slice(), b"%PDF", b""] {
        assert!(!has_pdf_header(bytes)?);
    }
    Ok(())
}
