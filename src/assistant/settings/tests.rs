use super::*;
fn model(id: &str, default: bool) -> Model {
    serde_json::from_value(serde_json::json!({"model":id,"displayName":id,"isDefault":default,"defaultReasoningEffort":"medium","supportedReasoningEfforts":[{"reasoningEffort":"medium","description":"Balanced"}]})).unwrap()
}
#[test]
fn default_prefers_astra_but_explicit_choice_survives_refresh() {
    let mut models = vec![model("gpt-5.6-sol", true), model("gpt-6-sol", false)];
    let mut prefs = Preferences::default();
    assert_eq!(prefs.resolve(&models).unwrap().id, "gpt-5.6-sol");
    models.push(model("gpt-6-astra", false));
    assert_eq!(prefs.resolve(&models).unwrap().id, DEFAULT_MODEL);
    models.push(model("gpt-6.1", false));
    assert_eq!(prefs.resolve(&models).unwrap().id, DEFAULT_MODEL);
    prefs.model = Some("gpt-5.6-sol".into());
    let persisted: Preferences =
        serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
    assert_eq!(persisted.resolve(&models).unwrap().id, "gpt-5.6-sol");
    models.remove(0);
    assert!(persisted.resolve(&models).is_err());
    assert!(prefs.resolve(&[]).is_err());
}
#[test]
fn unsupported_effort_uses_the_models_advertised_default() {
    let model = model("gpt-6-astra", false);
    let mut prefs = Preferences::default();
    prefs.efforts.insert(model.id.clone(), "unsupported".into());
    assert_eq!(prefs.effort(&model), "medium");
    assert_eq!(latest(&[]), None);
    assert_eq!(generation("gpt-10.2-test"), vec![10, 2]);
}
#[test]
fn new_preferences_use_standard_low_and_keep_explicit_overrides() {
    let mut model = model("gpt-6-astra", false);
    model.efforts.push(Effort {
        id: "low".into(),
        description: "Fast".into(),
    });
    model.tiers.push(ServiceTier {
        id: "priority".into(),
        name: "Fast".into(),
        description: "Increased usage".into(),
    });
    model.default_tier = Some("priority".into());
    let mut prefs = Preferences::default();
    assert_eq!(prefs.effort(&model), "low");
    assert_eq!(prefs.tier(&model), Some("default"));
    prefs.efforts.insert(model.id.clone(), "medium".into());
    prefs.tiers.insert(model.id.clone(), "priority".into());
    assert_eq!(prefs.effort(&model), "medium");
    assert_eq!(prefs.tier(&model), Some("priority"));
}
#[test]
fn unreadable_preferences_are_set_aside_before_a_save_replaces_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("assistant-preferences.json");
    let newer = br#"{"auto_apply":"a-value-from-a-newer-build","model":"gpt-6-astra"}"#;
    std::fs::write(&path, newer).unwrap();
    let loaded = Preferences::load_from(&path);
    assert_eq!(loaded.model, None);
    loaded.save_to(&path).unwrap();
    let bad = dir.path().join("assistant-preferences.json.bad");
    assert_eq!(std::fs::read(&bad).unwrap(), newer);

    // A readable file and a missing one leave nothing behind.
    std::fs::remove_file(&bad).unwrap();
    assert_eq!(Preferences::load_from(&path).model, None);
    std::fs::remove_file(&path).unwrap();
    let _ = Preferences::load_from(&path);
    assert!(!bad.exists());
}
