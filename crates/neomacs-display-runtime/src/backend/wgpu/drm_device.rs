//! DRM device discovery for GPU device path mapping.
//!
//! This module provides functions to discover DRM render nodes
//! and map wgpu adapter info to specific GPU device paths.

use std::fs;
use std::path::{Path, PathBuf};

/// DRM device information.
#[derive(Debug, Clone)]
pub struct DrmDeviceInfo {
    /// The render node path (e.g., "/dev/dri/renderD128")
    pub render_node: PathBuf,
    /// The card path (e.g., "/dev/dri/card0")
    pub card_path: Option<PathBuf>,
    /// PCI slot name (e.g., "0000:03:00.0")
    pub pci_slot: Option<String>,
    /// Vendor ID (e.g., 0x1002 for AMD)
    pub vendor_id: Option<u32>,
    /// Device ID
    pub device_id: Option<u32>,
    /// Device name from driver
    pub driver_name: Option<String>,
}

/// Find all DRM render nodes on the system.
pub fn find_drm_render_nodes() -> Vec<DrmDeviceInfo> {
    tracing::debug!("find_drm_render_nodes: starting");
    let mut devices = Vec::new();

    let drm_dir = Path::new("/sys/class/drm");
    if !drm_dir.exists() {
        tracing::warn!("DRM sysfs directory not found");
        return devices;
    }
    tracing::debug!("find_drm_render_nodes: scanning /sys/class/drm");

    // Look for renderD* devices
    if let Ok(entries) = fs::read_dir(drm_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            tracing::debug!("find_drm_render_nodes: found entry: {}", name_str);

            if name_str.starts_with("renderD") {
                tracing::debug!(
                    "find_drm_render_nodes: processing render node: {}",
                    name_str
                );
                if let Some(info) = get_drm_device_info(&entry.path()) {
                    tracing::debug!(
                        "find_drm_render_nodes: got info for {}: vendor={:04x?}, device={:04x?}",
                        name_str,
                        info.vendor_id,
                        info.device_id
                    );
                    devices.push(info);
                }
            }
        }
    }

    // Sort by render node number for consistent ordering
    devices.sort_by(|a, b| a.render_node.cmp(&b.render_node));

    tracing::info!("Found {} DRM render nodes", devices.len());
    for dev in &devices {
        tracing::info!(
            "  {:?}: pci={:?}, vendor={:04x?}, device={:04x?}, driver={:?}",
            dev.render_node,
            dev.pci_slot,
            dev.vendor_id,
            dev.device_id,
            dev.driver_name,
        );
    }

    devices
}

/// Get detailed information about a DRM device from sysfs.
fn get_drm_device_info(sysfs_path: &Path) -> Option<DrmDeviceInfo> {
    let name = sysfs_path.file_name()?.to_string_lossy();

    // Build render node path
    let render_node = PathBuf::from(format!("/dev/dri/{}", name));
    if !render_node.exists() {
        return None;
    }

    // Follow symlink to get device path
    let device_path = sysfs_path.join("device");
    let real_device = fs::read_link(&device_path).ok()?;

    // Get PCI slot from the path (last component is usually the slot)
    let pci_slot = real_device
        .file_name()
        .map(|s| s.to_string_lossy().into_owned());

    // Try to read vendor and device IDs
    let vendor_id = read_hex_file(&device_path.join("vendor"));
    let device_id = read_hex_file(&device_path.join("device"));

    // Try to get driver name
    let driver_name = fs::read_link(device_path.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));

    // Find corresponding card device
    let card_path = find_card_for_render(&name);

    Some(DrmDeviceInfo {
        render_node,
        card_path,
        pci_slot,
        vendor_id,
        device_id,
        driver_name,
    })
}

/// Read a hex value from a sysfs file (like vendor/device).
fn read_hex_file(path: &Path) -> Option<u32> {
    let contents = fs::read_to_string(path).ok()?;
    let trimmed = contents.trim().trim_start_matches("0x");
    u32::from_str_radix(trimmed, 16).ok()
}

/// Find the card device corresponding to a render device.
fn find_card_for_render(render_name: &str) -> Option<PathBuf> {
    // renderD128 corresponds to card0, renderD129 to card1, etc.
    // The number is 128 + card_number
    let render_num: u32 = render_name.trim_start_matches("renderD").parse().ok()?;
    let card_num = render_num.checked_sub(128)?;
    let card_path = PathBuf::from(format!("/dev/dri/card{}", card_num));
    if card_path.exists() {
        Some(card_path)
    } else {
        None
    }
}

/// Find a DRM render node matching the given wgpu adapter info.
///
/// This uses the PCI bus ID from wgpu's AdapterInfo to find the corresponding
/// DRM render node.
///
/// # Arguments
/// * `pci_bus_id` - PCI bus ID in format "domain:bus:device.function" (e.g., "0000:03:00.0")
/// * `vendor_id` - Vendor ID from adapter info
/// * `device_id` - Device ID from adapter info
pub fn find_render_node_for_adapter(
    pci_bus_id: Option<&str>,
    vendor_id: Option<u32>,
    device_id: Option<u32>,
) -> Option<PathBuf> {
    let devices = find_drm_render_nodes();

    // First try to match by PCI bus ID (most reliable)
    if let Some(pci_id) = pci_bus_id {
        // Normalize PCI ID format (wgpu uses "bus:device.function", we need "domain:bus:device.function")
        let normalized = if pci_id.matches(':').count() == 1 {
            // Format is "bus:device.function", add domain
            format!("0000:{}", pci_id)
        } else {
            pci_id.to_string()
        };

        tracing::info!("Looking for DRM device with PCI slot: {}", normalized);

        for dev in &devices {
            if let Some(ref slot) = dev.pci_slot
                && (slot == &normalized || slot.ends_with(&format!("/{}", normalized)))
            {
                tracing::info!(
                    "Found matching DRM device by PCI slot: {:?}",
                    dev.render_node
                );
                return Some(dev.render_node.clone());
            }
        }
    }

    // Fall back to matching by vendor/device ID
    if let (Some(v), Some(d)) = (vendor_id, device_id) {
        tracing::info!(
            "Looking for DRM device with vendor={:04x}, device={:04x}",
            v,
            d
        );

        for dev in &devices {
            if dev.vendor_id == Some(v) && dev.device_id == Some(d) {
                tracing::info!(
                    "Found matching DRM device by vendor/device ID: {:?}",
                    dev.render_node
                );
                return Some(dev.render_node.clone());
            }
        }
    }

    // If we still haven't found it, try to match by vendor only (useful for multi-GPU)
    if let Some(v) = vendor_id {
        for dev in &devices {
            if dev.vendor_id == Some(v) {
                tracing::info!(
                    "Found DRM device matching vendor {:04x}: {:?}",
                    v,
                    dev.render_node
                );
                return Some(dev.render_node.clone());
            }
        }
    }

    tracing::warn!("No matching DRM render node found");
    None
}

/// Get the DRM render node path from wgpu adapter info.
///
/// This is the main entry point for getting the GPU device path
/// to pass to WPE for zero-copy buffer sharing.
pub fn get_render_node_from_adapter_info(info: &wgpu::AdapterInfo) -> Option<PathBuf> {
    tracing::info!("wgpu adapter info:");
    tracing::info!("  name: {}", info.name);
    tracing::info!("  vendor: {:04x}", info.vendor);
    tracing::info!("  device: {:04x}", info.device);
    tracing::info!("  device_type: {:?}", info.device_type);
    tracing::info!("  driver: {}", info.driver);
    tracing::info!("  driver_info: {}", info.driver_info);
    tracing::info!("  backend: {:?}", info.backend);

    // Get PCI bus ID - wgpu provides this for Vulkan backend
    let pci_bus_id = if !info.driver_info.is_empty() {
        // Some drivers put PCI info in driver_info
        None // We'll rely on vendor/device ID matching
    } else {
        None
    };

    find_render_node_for_adapter(pci_bus_id, Some(info.vendor), Some(info.device))
}

#[cfg(test)]
#[path = "drm_device_test.rs"]
mod tests;
