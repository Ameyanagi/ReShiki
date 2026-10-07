use super::*;

#[test]
fn unbudgeted_resize_preserves_data_and_alignment() {
    let allocator = BoundedHeap;
    for alignment in [1, 8, 16, 64, 4096] {
        let mut original = Layout::from_size_align(3, alignment).unwrap();
        // SAFETY: This test owns the allocation and always supplies the
        // original layout when resizing/freeing it. The budget stays off.
        unsafe {
            let mut data = allocator.alloc_zeroed(original);
            assert!(!data.is_null());
            assert_eq!(data as usize % alignment, 0);
            assert_eq!(std::slice::from_raw_parts(data, 3), &[0; 3]);
            data.write_bytes(0x5a, original.size());
            for size in [200, 8193, 17, 1] {
                let resized = allocator.realloc(data, original, size);
                assert!(!resized.is_null());
                assert_eq!(resized as usize % alignment, 0);
                assert!(
                    std::slice::from_raw_parts(resized, original.size().min(size))
                        .iter()
                        .all(|byte| *byte == 0x5a)
                );
                resized.write_bytes(0x5a, size);
                data = resized;
                original = Layout::from_size_align(size, alignment).unwrap();
            }
            allocator.dealloc(data, original);
        }
    }
    assert_eq!(USED.load(Ordering::SeqCst), 0);
}
