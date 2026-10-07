use super::*;

#[test]
fn cumulative_budget_survives_stage_boundaries() {
    let mut remaining = 3;
    assert!(finish_with_work(1, &[], None, Options::default(), &mut remaining).is_ok());
    assert_eq!(remaining, 1);
    assert!(matches!(
        finish_with_work(1, &[], None, Options::default(), &mut remaining),
        Err(Error::Limit)
    ));
    assert_eq!(remaining, 0);
}
