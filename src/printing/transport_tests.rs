#[tokio::test]
async fn print_errors_keep_prefix_and_drain_remaining_bytes_to_eof() {
    use tokio::io::AsyncWriteExt;
    let bytes: Vec<_> = (0..65537).map(|index| (index % 251) as u8).collect();
    let expected = bytes[..8192].to_vec();
    let (mut writer, reader) = tokio::io::duplex(1024);
    let producer = tokio::spawn(async move {
        writer.write_all(&bytes).await.unwrap();
        writer.shutdown().await.unwrap();
    });
    let retained = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        super::drain_errors(reader),
    )
    .await
    .unwrap()
    .unwrap();
    producer.await.unwrap();
    assert_eq!(retained, expected);
}
