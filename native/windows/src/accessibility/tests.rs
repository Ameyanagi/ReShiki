use super::*;
#[test]
fn generations_are_unique_across_owner_threads_and_never_wrap() {
    let counter = std::sync::Arc::new(AtomicUsize::new(1));
    let old = generation(&counter).unwrap();
    let other = counter.clone();
    let replacement = std::thread::spawn(move || generation(&other).unwrap())
        .join()
        .unwrap();
    assert_ne!(
        old, replacement,
        "a recycled HWND cannot match the old owner's property"
    );
    let exhausted = AtomicUsize::new(usize::MAX);
    assert!(generation(&exhausted).is_err());
    assert_eq!(exhausted.load(Ordering::Relaxed), usize::MAX);
}
