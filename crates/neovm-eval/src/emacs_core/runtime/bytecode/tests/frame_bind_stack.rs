use super::frame_bindings::{BindStack, FrameUnbindError};

#[test]
fn vm_frame_bind_stack_checks_counts_before_consuming_marks() {
    let mut binds = BindStack::new();
    binds.push(7);
    binds.push(9);
    assert_eq!(binds.consume(0, 10), Ok(None));
    assert_eq!(binds.as_slice(), &[7, 9]);
    assert_eq!(
        binds.consume(3, 10),
        Err(FrameUnbindError::TooMany {
            requested: 3,
            available: 2
        })
    );
    assert_eq!(binds.as_slice(), &[7, 9]);
    assert_eq!(
        binds
            .consume(1, 10)
            .map(|target| target.map(|target| target.depth())),
        Ok(Some(9))
    );
    assert_eq!(binds.as_slice(), &[7]);
    assert_eq!(
        binds
            .consume(1, 8)
            .map(|target| target.map(|target| target.depth())),
        Ok(Some(7))
    );
    assert!(binds.is_empty());
}

#[test]
fn vm_frame_bind_stack_rejects_stale_marks_without_consuming() {
    let mut binds = BindStack::new();
    binds.push(7);
    assert_eq!(
        binds.consume(1, 7),
        Err(FrameUnbindError::StaleMark {
            depth: 7,
            specpdl_len: 7
        })
    );
    assert_eq!(binds.as_slice(), &[7]);
    assert_eq!(
        binds.consume(1, 6),
        Err(FrameUnbindError::StaleMark {
            depth: 7,
            specpdl_len: 6
        })
    );
    assert_eq!(binds.as_slice(), &[7]);
}

#[test]
fn vm_frame_bind_stack_zero_and_transferred_marks_keep_their_scope() {
    let mut empty = BindStack::new();
    assert_eq!(empty.consume(0, 0), Ok(None));
    // OSR/resume transfers the frame's existing marks. Its fresh native entry
    // depth may be above both marks; consume the oldest selected mark exactly.
    let mut transferred: BindStack = [7, 9].into_iter().collect();
    assert_eq!(
        transferred
            .consume(2, 11)
            .map(|target| target.map(|target| target.depth())),
        Ok(Some(7))
    );
    assert!(transferred.is_empty());
}
