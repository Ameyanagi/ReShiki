use super::*;

#[test]
fn predicate_defaults_and_preparation_error_precedence_are_preserved() {
    for &(name, defaults) in PREDICATES {
        let xml = |value| {
            format!(
                "<CDXML><page id=\"1\"><fragment id=\"2\"><n id=\"3\" p=\"0 0\" {name}=\"{value}\"/></fragment></page></CDXML>"
            )
        };
        assert!(matches!(
            read(&xml("query")),
            Err(Error::Unsupported("query or reaction predicate"))
        ));
        let error = prepare_cdxml(&xml("query")).unwrap_err();
        assert_eq!(error.stage, PreparationStage::Validation);
        assert!(matches!(error.cause, PreparationCause::Predicate(value) if value == name));
        for &default in defaults {
            read(&xml(default)).unwrap();
            assert!(!matches!(
                prepare_cdxml(&xml(default)),
                Err(PreparationError {
                    cause: PreparationCause::Predicate(_),
                    ..
                })
            ));
        }
    }
    // Predicate priority follows the fixed table, not XML attribute order, and
    // precedes unsupported objects even when the object comes first.
    let xml =
        "<CDXML><page><unsupported/><n RxnChange=\"yes\" RingBondCount=\"2\"/></page></CDXML>";
    let error = prepare_cdxml(xml).unwrap_err();
    assert_eq!(error.stage, PreparationStage::Validation);
    assert!(matches!(
        error.cause,
        PreparationCause::Predicate("RingBondCount")
    ));
}
