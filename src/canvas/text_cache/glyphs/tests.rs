use super::*;

#[test]
fn zooming_evicts_glyphs_instead_of_retaining_every_size() {
    let mut glyphs = Glyphs::default();
    let mut previous = 0;
    let mut evicted = false;
    for i in 0..160 {
        glyphs.draw(
            &Text {
                content: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789".into(),
                size: (8. + i as f32 * 0.125).into(),
                ..Default::default()
            },
            |_, _| {},
        );
        assert!(glyphs.within_budget());
        let count = glyphs.cache.outline_command_cache.len();
        evicted |= count < previous;
        previous = count;
    }
    assert!(evicted, "The workload must exercise cache eviction");
}
