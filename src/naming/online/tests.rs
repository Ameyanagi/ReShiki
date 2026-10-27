use super::*;
use serde_json::json;

fn property(cid: u64, smiles: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"PropertyTable":{"Properties":[{
        "CID":cid,"SMILES":smiles
    }]}}))
    .unwrap()
}

#[test]
fn request_is_a_complete_name_lookup_with_one_encoded_segment() {
    assert_eq!(
        name_url("(E)-a/b + #?% β"),
        format!("{API}/name/%28E%29-a%2Fb%20%2B%20%23%3F%25%20%CE%B2/cids/JSON?name_type=complete")
    );
}

#[test]
fn require_one_distinct_positive_cid_without_first_row_picking() {
    assert_eq!(
        parse_cid(br#"{"IdentifierList":{"CID":[702,702]}}"#).unwrap(),
        702
    );
    assert!(matches!(
        parse_cid(br#"{"IdentifierList":{"CID":[702,241]}}"#),
        Err(Error::Rejected(_))
    ));
    for bytes in [
        br#"{}"#.as_slice(),
        br#"{"IdentifierList":{"CID":[0]}}"#,
        br#"{"IdentifierList":{"CID":["702"]}}"#,
        b"<html>unavailable</html>",
    ] {
        assert!(matches!(parse_cid(bytes), Err(Error::Worker(_))));
    }
    assert!(matches!(
        parse_cid(br#"{"IdentifierList":{"CID":[]}}"#),
        Err(Error::Rejected(_))
    ));
}

#[test]
fn reject_extra_or_wrong_properties_and_never_use_connectivity_smiles() {
    for reply in [
        json!({}),
        json!({"PropertyTable":{"Properties":[{"CID":702,"ConnectivitySMILES":"CCO"}]}}),
        json!({"PropertyTable":{"Properties":[{"CID":241,"SMILES":"CCO"}]}}),
    ] {
        assert!(parse_record("ethanol", 702, &serde_json::to_vec(&reply).unwrap()).is_err());
    }
    let two = json!({"PropertyTable":{"Properties":[{"CID":702,"SMILES":"CCO"},{"CID":241,"SMILES":"c1ccccc1"}]}});
    assert!(matches!(
        parse_record("ethanol", 702, &serde_json::to_vec(&two).unwrap()),
        Err(Error::Rejected(_))
    ));
}

#[test]
fn charge_isotope_and_stereochemistry_are_retained() {
    let smiles = "[15NH3+][C@H](C)C(=O)[O-]";
    let record = parse_record("labelled alanine", 123, &property(123, smiles)).unwrap();
    assert_eq!(record.smiles, smiles);
    super::super::verify_identity(smiles, &record.canonical_smiles).unwrap();
    assert_ne!(
        record.canonical_smiles,
        canonical_smiles("[15NH3+][C@@H](C)C(=O)[O-]").unwrap()
    );
    assert_eq!(record.provenance, Provenance::PubChem { cid: 123 });
    assert_eq!(
        record.provenance.source_url().as_deref(),
        Some("https://pubchem.ncbi.nlm.nih.gov/compound/123")
    );
    assert!(record.synonyms.is_empty());
}

#[test]
fn salts_are_not_converted_to_free_bases_and_graph_bounds_still_apply() {
    // PubChem's clozapine hydrochloride is a distinct CID and includes .Cl;
    // a zero total Charge field would not establish connectedness.
    assert!(matches!(
        parse_record("salt", 1, &property(1, "C[NH2+]C.[Cl-]")),
        Err(Error::Disconnected(_))
    ));
    for smiles in ["C".repeat(513), "CC|r|".into(), "CC>>CC".into()] {
        assert!(matches!(
            parse_record("bad", 1, &property(1, &smiles)),
            Err(Error::Rejected(_))
        ));
    }
    for smiles in ["CC trailing name", "CC |r|"] {
        assert!(matches!(
            parse_record("bad response", 1, &property(1, smiles)),
            Err(Error::Worker(_))
        ));
    }
    assert!(matches!(
        parse_cid(&vec![b' '; LIMIT + 1]),
        Err(Error::Worker(_))
    ));
}

#[test]
fn only_structured_not_found_is_reported_as_a_missing_name() {
    assert!(matches!(
        checked_response(404, br#"{"Fault":{"Code":"PUGREST.NotFound"}}"#.to_vec()),
        Err(Error::Rejected(_))
    ));
    for status in [302, 404, 429, 503] {
        assert!(matches!(
            checked_response(status, b"<html>unavailable</html>".to_vec()),
            Err(Error::Worker(_))
        ));
    }
    let no_iupac = br#"{"PropertyTable":{"Properties":[{"CID":702,"SMILES":"CCO"}]}}"#;
    assert_eq!(
        parse_record("ethanol", 702, no_iupac)
            .unwrap()
            .systematic_name,
        None
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod network {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn already_cancelled_does_not_construct_or_poll_a_request() {
        let cancel = Cancel::default();
        cancel.stop();
        let called = std::sync::atomic::AtomicBool::new(false);
        let result: Result<(), Error> = bounded(cancel.clone(), DEADLINE, || {
            called.store(true, std::sync::atomic::Ordering::SeqCst);
            async { Ok(()) }
        })
        .await;
        assert!(matches!(result, Err(Error::Cancelled(_))));
        assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
        // The public function also exits before any HTTP transport is built.
        assert!(matches!(
            resolve_name_online("ethanol", cancel).await,
            Err(Error::Cancelled(_))
        ));
    }

    async fn server(
        response: Vec<u8>,
    ) -> (
        String,
        tokio::sync::oneshot::Receiver<()>,
        tokio::task::JoinHandle<usize>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let (ready, received) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut buf = [0; 1024];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0 && request.len() + n < 8192);
                request.extend_from_slice(&buf[..n]);
            }
            socket.write_all(&response).await.unwrap();
            let _ = ready.send(());
            let mut byte = [0];
            tokio::time::timeout(Duration::from_secs(2), socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap()
        });
        (url, received, task)
    }

    fn transport() -> Transport {
        Transport(
            reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
        )
    }

    async fn interrupted(cancel_it: bool) {
        let (url, received, server) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{".to_vec(),
        )
        .await;
        let cancel = Cancel::default();
        let copy = cancel.clone();
        let transport = transport();
        let lookup = tokio::spawn(async move {
            bounded(
                copy,
                if cancel_it {
                    DEADLINE
                } else {
                    Duration::from_millis(500)
                },
                || async { transport.get(&url).await },
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(1), received)
            .await
            .unwrap()
            .unwrap();
        if cancel_it {
            cancel.stop();
        }
        let error = tokio::time::timeout(Duration::from_secs(1), lookup)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        if cancel_it {
            assert!(matches!(error, Error::Cancelled(_)));
        } else {
            assert!(matches!(error, Error::Worker(ref text) if text.contains("time limit")));
        }
        assert_eq!(
            server.await.unwrap(),
            0,
            "dropping the request closes the pending body connection"
        );
    }

    #[tokio::test]
    async fn cancel_aborts_a_pending_response_body() {
        interrupted(true).await;
    }

    #[tokio::test]
    async fn deadline_aborts_a_pending_response_body() {
        interrupted(false).await;
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_while_streaming_without_content_length() {
        let (url, received, server) = server(
            [
                b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n".as_slice(),
                &vec![b'x'; LIMIT + 1],
            ]
            .concat(),
        )
        .await;
        let lookup = tokio::spawn(async move { transport().get(&url).await });
        received.await.unwrap();
        let error = tokio::time::timeout(Duration::from_secs(1), lookup)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, Error::Worker(ref text) if text.contains("64 KiB")));
        assert_eq!(server.await.unwrap(), 0);
    }
}
