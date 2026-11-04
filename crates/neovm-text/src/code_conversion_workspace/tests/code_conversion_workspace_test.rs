use super::*;

#[test]
fn nested_conversion_uses_and_then_kills_a_temporary_buffer() {
    let mut buffers = BufferManager::new();
    let mut workspace = CodeConversionWorkspace::default();

    let outer = workspace.acquire(&mut buffers, ConversionBufferEncoding::Multibyte);
    let outer_id = outer.buffer_id();
    let nested = workspace.acquire(&mut buffers, ConversionBufferEncoding::Unibyte);
    let nested_id = nested.buffer_id();

    assert!(
        buffers
            .get(outer_id)
            .is_some_and(|buffer| buffer.has_name(CODE_CONVERSION_WORK_BUFFER_NAME))
    );
    assert_ne!(nested_id, outer_id);
    assert!(buffers.get(nested_id).is_some());

    workspace.release(&mut buffers, nested);
    assert!(buffers.get(nested_id).is_none());
    assert!(buffers.get(outer_id).is_some());

    workspace.release(&mut buffers, outer);
    assert_eq!(
        buffers.find_buffer_by_name(CODE_CONVERSION_WORK_BUFFER_NAME),
        Some(outer_id)
    );
}
