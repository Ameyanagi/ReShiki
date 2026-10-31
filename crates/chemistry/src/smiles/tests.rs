use super::Error;

#[test]
fn stereochemistry_work_limit_keeps_its_actionable_message() {
    let message = crate::stereo::CIP_WORK_LIMIT_MESSAGE;
    assert_eq!(Error::Stereo(message.into()).to_string(), message);
}

#[test]
fn other_stereochemistry_errors_keep_the_smiles_context() {
    for message in [
        "Invalid CIP ranking index",
        "CIP ranking storage limit exceeded",
        "Atropisomer cleanup work limit exceeded",
    ] {
        assert_eq!(
            Error::Stereo(message.into()).to_string(),
            format!("SMILES stereochemistry: {message}")
        );
    }
}
