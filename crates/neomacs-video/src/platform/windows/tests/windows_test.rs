use super::*;

#[test]
fn media_foundation_defaults_to_limited_bt709() {
    assert_eq!(
        media_foundation_colorimetry(MediaFoundationColorMetadata::default()),
        VideoColorimetry::BT709_LIMITED
    );
}

#[test]
fn media_foundation_maps_hdr10_metadata() {
    assert_eq!(
        media_foundation_colorimetry(MediaFoundationColorMetadata {
            primaries: Some(MFVideoPrimaries_BT2020.0 as u32),
            transfer: Some(MFVideoTransFunc_2084.0 as u32),
            matrix: Some(MFVideoTransferMatrix_BT2020_10.0 as u32),
            range: Some(MFNominalRange_0_255.0 as u32),
            chroma_siting: Some(MFVideoChromaSubsampling_DV_PAL.0 as u32),
        }),
        VideoColorimetry {
            primaries: VideoColorPrimaries::Bt2020,
            transfer: VideoTransferCharacteristic::Pq,
            matrix: VideoMatrixCoefficients::Bt2020NonConstantLuminance,
            range: VideoColorRange::Full,
            chroma_location: VideoChromaLocation::TopLeft,
        }
    );
}

#[test]
fn output_format_keeps_native_and_packed_types_consistent() {
    assert_eq!(
        WindowsOutputFormat::Nv12.frame(),
        VideoFrameFormat::BiPlanar420(BiPlanarVideoFormat::Nv12)
    );
    assert_eq!(WindowsOutputFormat::Nv12.wgpu(), wgpu::TextureFormat::NV12);
    assert_eq!(
        WindowsOutputFormat::Nv12.candidates(),
        [WindowsOutputFormat::Nv12, WindowsOutputFormat::Bgra8]
    );
    assert_eq!(
        WindowsOutputFormat::Nv12.fallback_after_rejection(),
        Some(WindowsOutputFormat::Bgra8)
    );
    assert_eq!(WindowsOutputFormat::Bgra8.fallback_after_rejection(), None);
    assert_eq!(
        WindowsOutputFormat::Bgra8.frame(),
        VideoFrameFormat::Packed(PackedVideoFormat::Bgra8)
    );
    assert_eq!(
        WindowsOutputFormat::Bgra8.media_engine_dxgi(),
        DXGI_FORMAT_B8G8R8A8_UNORM
    );
    assert_eq!(
        WindowsOutputFormat::Bgra8.resource_dxgi(),
        DXGI_FORMAT_B8G8R8A8_TYPELESS
    );
    assert_eq!(
        WindowsOutputFormat::Bgra8.wgpu(),
        wgpu::TextureFormat::Bgra8UnormSrgb
    );
    assert_eq!(
        WindowsOutputFormat::Nv12.completed_import(),
        CompletedFrameImport::GpuBlit {
            reported_bytes: None
        }
    );
}

#[test]
fn format_change_event_invalidates_cached_stream_metadata() {
    assert_eq!(
        media_engine_event_flag(MF_MEDIA_ENGINE_EVENT_FORMATCHANGE.0 as u32),
        EVENT_FORMAT_CHANGED
    );
}

#[test]
fn runtime_scoped_drops_resources_before_runtime_on_drop_and_unwind() {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct DropRecorder {
        label: &'static str,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for DropRecorder {
        fn drop(&mut self) {
            self.log.borrow_mut().push(self.label);
        }
    }

    struct Resources {
        _session: DropRecorder,
        _bridge: DropRecorder,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for Resources {
        fn drop(&mut self) {
            self.log.borrow_mut().push("resources_drop");
        }
    }

    let log = Rc::new(RefCell::new(Vec::new()));
    {
        let _scoped = RuntimeScoped::new(
            Resources {
                _session: DropRecorder {
                    label: "session",
                    log: Rc::clone(&log),
                },
                _bridge: DropRecorder {
                    label: "bridge",
                    log: Rc::clone(&log),
                },
                log: Rc::clone(&log),
            },
            DropRecorder {
                label: "runtime",
                log: Rc::clone(&log),
            },
        );
    }
    assert_eq!(
        *log.borrow(),
        vec!["resources_drop", "session", "bridge", "runtime"]
    );

    log.borrow_mut().clear();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scoped = RuntimeScoped::new(
            Resources {
                _session: DropRecorder {
                    label: "session",
                    log: Rc::clone(&log),
                },
                _bridge: DropRecorder {
                    label: "bridge",
                    log: Rc::clone(&log),
                },
                log: Rc::clone(&log),
            },
            DropRecorder {
                label: "runtime",
                log: Rc::clone(&log),
            },
        );
        panic!("simulated unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(
        *log.borrow(),
        vec!["resources_drop", "session", "bridge", "runtime"]
    );
}
