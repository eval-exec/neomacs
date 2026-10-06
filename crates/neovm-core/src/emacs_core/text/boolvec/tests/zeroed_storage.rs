use super::{
    BoolVectorWordLayout, allocate_bool_vector_word_capacity, allocate_zeroed_bool_vector_words,
};
use crate::emacs_core::alloc::AllocationFailure;
use crate::tagged::header::BoolVectorObj;

#[test]
fn bool_vector_zeroed_storage_owns_checked_initialized_words() {
    crate::test_utils::init_test_tracing();

    let empty = allocate_zeroed_bool_vector_words(BoolVectorWordLayout::try_from(0).unwrap())
        .expect("empty word storage");
    assert!(empty.is_empty());
    assert_eq!(empty.capacity(), 0);
    assert!(matches!(
        BoolVectorWordLayout::try_from(usize::MAX),
        Err(AllocationFailure::InvalidLayout(_))
    ));

    for bits in [1, 63, 64, 65, 127, 128, 129, 100_000] {
        let count = BoolVectorObj::words_for(bits);
        let words =
            allocate_zeroed_bool_vector_words(BoolVectorWordLayout::try_from(count).unwrap())
                .expect("zeroed initialized storage");
        assert_eq!(words.len(), count);
        assert_eq!(words.capacity(), count);
        assert!(words.iter().all(|&word| word == 0));
        assert_eq!(words[count - 1] & !BoolVectorObj::last_word_mask(bits), 0);
    }

    let extent = BoolVectorWordLayout::try_from(3).unwrap();
    let mut first = allocate_zeroed_bool_vector_words(extent).unwrap();
    let second = allocate_zeroed_bool_vector_words(extent).unwrap();
    assert_ne!(first.as_ptr(), second.as_ptr());
    first[2] = u64::MAX;
    assert_eq!(second, [0, 0, 0]);
    first.try_reserve_exact(8).expect("ordinary Vec growth");
    first.extend_from_slice(&[7; 8]);
    assert_eq!(&first[..3], &[0, 0, u64::MAX]);
    assert_eq!(&first[3..], &[7; 8]);

    let empty_capacity =
        allocate_bool_vector_word_capacity(BoolVectorWordLayout::try_from(0).unwrap()).unwrap();
    assert_eq!(empty_capacity.len(), 0);
    assert_eq!(empty_capacity.capacity(), 0);
    for bits in [1, 63, 64, 65, 127, 128, 129, 100_000] {
        let count = BoolVectorObj::words_for(bits);
        let mut words =
            allocate_bool_vector_word_capacity(BoolVectorWordLayout::try_from(count).unwrap())
                .expect("uninitialized truthy word capacity");
        assert_eq!(words.len(), 0);
        assert_eq!(words.capacity(), count);
        let pointer = words.as_ptr();
        words.resize(count, u64::MAX);
        assert_eq!(words.as_ptr(), pointer, "fill must use owned capacity");
        let vector = BoolVectorObj::new(bits, words);
        assert_eq!(vector.words().as_ptr(), pointer, "exact backing is adopted");
        assert!(
            vector.words()[..count - 1]
                .iter()
                .all(|&word| word == u64::MAX)
        );
        assert_eq!(
            vector.words()[count - 1],
            BoolVectorObj::last_word_mask(bits)
        );
    }

    let mut capacity = allocate_bool_vector_word_capacity(extent).unwrap();
    assert!(capacity.is_empty());
    assert_eq!(capacity.capacity(), 3);
    capacity.resize(3, u64::MAX);
    capacity[1] = 7;
    capacity
        .try_reserve_exact(8)
        .expect("ordinary truthy Vec growth");
    capacity.extend_from_slice(&[11; 8]);
    assert_eq!(&capacity[..3], &[u64::MAX, 7, u64::MAX]);
    assert_eq!(&capacity[3..], &[11; 8]);
    drop(capacity);

    // Both original and resized storage are reclaimed by ordinary Vec Drop.
    drop(first);
    drop(second);
}
