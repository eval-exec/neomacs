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
}

pub(crate) fn wait_for_input(timeout_milliseconds: f64) -> u32 {
    imported_wait_for_input(timeout_milliseconds)
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
