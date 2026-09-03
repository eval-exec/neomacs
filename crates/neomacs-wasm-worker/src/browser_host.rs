//! Checked Rust facade over the Worker's deliberately small raw Wasm ABI.

use std::time::Duration;

use neomacs_wasm_protocol::InputBatchSequence;

const MAX_STARTUP_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_RUNTIME_IMAGE_BYTES: usize = 512 * 1024 * 1024;
const MAX_RUNTIME_IMAGE_ID_BYTES: usize = 128;
const MAX_RUNTIME_RESOURCE_BUNDLE_BYTES: usize = 512 * 1024 * 1024;
const MAX_RUNTIME_RESOURCE_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostWake {
    Input,
    TimedOut,
}

#[link(wasm_import_module = "neomacs_host")]
unsafe extern "C" {
    #[link_name = "wait_for_input"]
    safe fn imported_wait_for_input(timeout_milliseconds: f64) -> u32;
    #[link_name = "monotonic_time_milliseconds"]
    safe fn imported_monotonic_time_milliseconds() -> f64;
    #[link_name = "wall_time_milliseconds"]
    safe fn imported_wall_time_milliseconds() -> f64;
    safe fn startup_len() -> u32;
    safe fn copy_startup(destination: *mut u8, capacity: u32) -> u32;
    safe fn runtime_image_len() -> u32;
    safe fn copy_runtime_image(destination: *mut u8, capacity: u32) -> u32;
    safe fn runtime_image_id_len() -> u32;
    safe fn copy_runtime_image_id(destination: *mut u8, capacity: u32) -> u32;
    safe fn runtime_resource_bundle_len() -> u32;
    safe fn copy_runtime_resource_bundle(destination: *mut u8, capacity: u32) -> u32;
    safe fn runtime_resource_id_len() -> u32;
    safe fn copy_runtime_resource_id(destination: *mut u8, capacity: u32) -> u32;
    safe fn input_len() -> u32;
    safe fn copy_input(destination: *mut u8, capacity: u32) -> u32;
    #[link_name = "acknowledge_input"]
    safe fn imported_acknowledge_input(source: *const u8, length: u32) -> u32;
    safe fn publish_frame(source: *const u8, length: u32) -> u32;
    safe fn post_status(source: *const u8, length: u32);
    safe fn post_failure(source: *const u8, length: u32);
    safe fn fs_stat(path: *const u8, path_length: u32) -> u32;
    safe fn fs_read(path: *const u8, path_length: u32) -> u32;
    safe fn fs_read_directory(path: *const u8, path_length: u32) -> u32;
    safe fn fs_write(
        path: *const u8,
        path_length: u32,
        source: *const u8,
        source_length: u32,
        mode: u32,
        offset: f64,
        sync: u32,
    ) -> u32;
    safe fn fs_create_directory(path: *const u8, path_length: u32, parents: u32) -> u32;
    safe fn fs_remove_file(path: *const u8, path_length: u32) -> u32;
    safe fn fs_remove_directory(path: *const u8, path_length: u32, recursive: u32) -> u32;
    safe fn fs_rename(
        from: *const u8,
        from_length: u32,
        to: *const u8,
        to_length: u32,
        replace: u32,
    ) -> u32;
    safe fn fs_canonicalize(path: *const u8, path_length: u32) -> u32;
    safe fn fs_result_kind() -> u32;
    safe fn fs_result_len() -> f64;
    safe fn fs_result_modified_milliseconds() -> f64;
    safe fn fs_result_error_len() -> u32;
    safe fn fs_copy_result(destination: *mut u8, capacity: u32) -> u32;
    safe fn fs_copy_result_error(destination: *mut u8, capacity: u32) -> u32;
}

pub(crate) fn wait_for_input(timeout_milliseconds: f64) -> u32 {
    imported_wait_for_input(timeout_milliseconds)
}

pub(crate) fn monotonic_time_milliseconds() -> f64 {
    imported_monotonic_time_milliseconds()
}

pub(crate) fn wall_time_milliseconds() -> f64 {
    imported_wall_time_milliseconds()
}

pub(crate) fn wait(timeout: Duration) -> Result<HostWake, String> {
    match imported_wait_for_input(timeout.as_secs_f64() * 1000.0) {
        1 => Ok(HostWake::Input),
        2 => Ok(HostWake::TimedOut),
        other => Err(format!("browser host returned unknown wake code {other}")),
    }
}

pub(crate) fn startup_bytes() -> Result<Vec<u8>, String> {
    copy_host_bytes("startup message", startup_len(), MAX_STARTUP_BYTES, |buffer| {
        copy_startup(buffer.as_mut_ptr(), buffer.len() as u32)
    })
}

pub(crate) fn runtime_image_bytes() -> Result<Vec<u8>, String> {
    copy_host_bytes(
        "portable runtime image",
        runtime_image_len(),
        MAX_RUNTIME_IMAGE_BYTES,
        |buffer| copy_runtime_image(buffer.as_mut_ptr(), buffer.len() as u32),
    )
}

pub(crate) fn runtime_image_id_bytes() -> Result<Vec<u8>, String> {
    copy_host_bytes(
        "portable runtime image ID",
        runtime_image_id_len(),
        MAX_RUNTIME_IMAGE_ID_BYTES,
        |buffer| copy_runtime_image_id(buffer.as_mut_ptr(), buffer.len() as u32),
    )
}

pub(crate) fn runtime_resource_bundle_bytes() -> Result<Vec<u8>, String> {
    copy_host_bytes(
        "runtime resource bundle",
        runtime_resource_bundle_len(),
        MAX_RUNTIME_RESOURCE_BUNDLE_BYTES,
        |buffer| copy_runtime_resource_bundle(buffer.as_mut_ptr(), buffer.len() as u32),
    )
}

pub(crate) fn runtime_resource_id_bytes() -> Result<Vec<u8>, String> {
    copy_host_bytes(
        "runtime resource bundle ID",
        runtime_resource_id_len(),
        MAX_RUNTIME_RESOURCE_ID_BYTES,
        |buffer| copy_runtime_resource_id(buffer.as_mut_ptr(), buffer.len() as u32),
    )
}

pub(crate) fn take_input_bytes() -> Result<Vec<u8>, String> {
    copy_host_bytes("browser input batch", input_len(), MAX_INPUT_BYTES, |buffer| {
        copy_input(buffer.as_mut_ptr(), buffer.len() as u32)
    })
}

pub(crate) fn acknowledge_input(sequence: InputBatchSequence) -> Result<(), String> {
    let wire = sequence.get().to_string();
    if imported_acknowledge_input(wire.as_ptr(), wire.len() as u32) == 1 {
        Ok(())
    } else {
        Err(format!(
            "browser host rejected input acknowledgement {}",
            sequence.get()
        ))
    }
}

fn copy_host_bytes(
    description: &str,
    length: u32,
    maximum: usize,
    copy: impl FnOnce(&mut [u8]) -> u32,
) -> Result<Vec<u8>, String> {
    let length = length as usize;
    if length == 0 {
        return Err(format!("browser host supplied an empty {description}"));
    }
    if length > maximum {
        return Err(format!(
            "browser host {description} is {length} bytes; maximum is {maximum}"
        ));
    }
    let mut bytes = vec![0; length];
    let copied = copy(&mut bytes) as usize;
    if copied != length {
        return Err(format!(
            "browser host copied {copied} of {length} {description} bytes"
        ));
    }
    Ok(bytes)
}

pub(crate) fn send_frame(bytes: &[u8]) -> Result<(), String> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| "browser presentation exceeds the Wasm32 transfer limit".to_owned())?;
    if publish_frame(bytes.as_ptr(), length) == 1 {
        Ok(())
    } else {
        Err("browser host rejected the frame transfer".to_owned())
    }
}

pub(crate) fn report_status(message: &str) {
    post_status(message.as_ptr(), message.len() as u32);
}

pub(crate) fn report_failure(message: &str) {
    post_failure(message.as_ptr(), message.len() as u32);
}

pub(super) fn filesystem_stat(path: &str) -> u32 {
    fs_stat(path.as_ptr(), path.len() as u32)
}

pub(super) fn filesystem_read(path: &str) -> u32 {
    fs_read(path.as_ptr(), path.len() as u32)
}

pub(super) fn filesystem_read_directory(path: &str) -> u32 {
    fs_read_directory(path.as_ptr(), path.len() as u32)
}

pub(super) fn filesystem_write(
    path: &str,
    contents: &[u8],
    mode: u32,
    offset: u64,
    sync: bool,
) -> u32 {
    fs_write(
        path.as_ptr(),
        path.len() as u32,
        contents.as_ptr(),
        contents.len() as u32,
        mode,
        offset as f64,
        u32::from(sync),
    )
}

pub(super) fn filesystem_create_directory(path: &str, parents: bool) -> u32 {
    fs_create_directory(path.as_ptr(), path.len() as u32, u32::from(parents))
}

pub(super) fn filesystem_remove_file(path: &str) -> u32 {
    fs_remove_file(path.as_ptr(), path.len() as u32)
}

pub(super) fn filesystem_remove_directory(path: &str, recursive: bool) -> u32 {
    fs_remove_directory(
        path.as_ptr(),
        path.len() as u32,
        u32::from(recursive),
    )
}

pub(super) fn filesystem_rename(from: &str, to: &str, replace: bool) -> u32 {
    fs_rename(
        from.as_ptr(),
        from.len() as u32,
        to.as_ptr(),
        to.len() as u32,
        u32::from(replace),
    )
}

pub(super) fn filesystem_canonicalize(path: &str) -> u32 {
    fs_canonicalize(path.as_ptr(), path.len() as u32)
}

pub(super) fn filesystem_result_kind() -> u32 {
    fs_result_kind()
}

pub(super) fn filesystem_result_len() -> f64 {
    fs_result_len()
}

pub(super) fn filesystem_result_modified_milliseconds() -> f64 {
    fs_result_modified_milliseconds()
}

pub(super) fn filesystem_result_error() -> Result<String, String> {
    let length = fs_result_error_len() as usize;
    let mut bytes = vec![0; length];
    let copied = fs_copy_result_error(bytes.as_mut_ptr(), bytes.len() as u32) as usize;
    if copied != length {
        return Err(format!(
            "browser filesystem copied {copied} of {length} error bytes"
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(super) fn filesystem_result_bytes(length: usize) -> Result<Vec<u8>, String> {
    let capacity = u32::try_from(length)
        .map_err(|_| "browser filesystem result exceeds the Wasm32 transfer limit".to_owned())?;
    let mut bytes = vec![0; length];
    let copied = fs_copy_result(bytes.as_mut_ptr(), capacity) as usize;
    if copied != length {
        return Err(format!(
            "browser filesystem copied {copied} of {length} result bytes"
        ));
    }
    Ok(bytes)
}
