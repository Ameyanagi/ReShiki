use super::*;
#[test]
fn nonblocking_pipe_keeps_empty_connected_reads_open_between_chunks() {
    let (mut reader, writer) = pipe().unwrap();
    let mut writer = File::from(writer);
    let mut buffer = [0_u8; 128];
    assert_eq!(read(&mut reader, &mut []).unwrap(), 0);
    let mut written = 0;
    // SAFETY: the live synchronous client handle receives a zero-length
    // slice and a writable DWORD; no operation retains either pointer.
    unsafe {
        windows::Win32::Storage::FileSystem::WriteFile(
            HANDLE(writer.as_raw_handle()),
            Some(&[]),
            Some(&mut written),
            None,
        )
    }
    .unwrap();
    assert_eq!(written, 0);
    assert_eq!(
        read(&mut reader, &mut buffer).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let mut captured = Vec::new();
    for chunk in [b"{\"protocol\":1,".as_slice(), b"\"value\":\"delayed\"}"] {
        writer.write_all(chunk).unwrap();
        let count = read(&mut reader, &mut buffer).unwrap();
        captured.extend_from_slice(&buffer[..count]);
        assert_eq!(
            read(&mut reader, &mut buffer).unwrap_err().kind(),
            io::ErrorKind::WouldBlock,
            "A live writer may produce another response chunk later"
        );
    }
    assert_eq!(captured, b"{\"protocol\":1,\"value\":\"delayed\"}");
    drop(writer);
    assert_eq!(read(&mut reader, &mut buffer).unwrap(), 0);
}

#[test]
fn windows_argument_quoting_preserves_spaces_quotes_and_trailing_slashes() {
    for (input, expected) in [
        ("", "\"\""),
        ("a b", "\"a b\""),
        ("a\"b", "\"a\\\"b\""),
        ("C:\\a b\\", "\"C:\\a b\\\\\""),
    ] {
        assert_eq!(
            String::from_utf16(&quoted(OsStr::new(input)).unwrap()).unwrap(),
            expected
        );
    }
}
