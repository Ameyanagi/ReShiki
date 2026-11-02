use super::Preset;

#[test]
fn every_arrow_preset_explains_its_meaning_in_one_line() {
    for &preset in Preset::ALL {
        let meaning = preset.meaning();
        assert!(!meaning.trim().is_empty(), "{preset} needs a meaning");
        assert_eq!(meaning.lines().count(), 1, "{preset}");
    }
    assert!(Preset::Resonance.meaning().contains("one species"));
    assert!(Preset::Resonance.meaning().contains("not an equilibrium"));
    assert!(Preset::Equilibrium.meaning().contains('⇌'));
    assert!(Preset::Equilibrium.meaning().contains("tautomers"));
    assert!(Preset::Resonance.meaning().contains('↔'));
    assert!(Preset::Retro.meaning().contains('⇒'));
    assert!(Preset::Retro.meaning().contains("Corey"));
    assert!(Preset::NoGo.meaning().contains("ChemDraw"));
    assert!(Preset::Fishhook.meaning().contains("homolysis"));
    assert!(Preset::Fishhook.meaning().contains("radical"));
}
