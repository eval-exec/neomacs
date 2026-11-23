use super::*;
use std::time::Duration;
use winit::{
    event::{Ime, Modifiers, MouseScrollDelta, TouchPhase},
    keyboard::ModifiersState,
};

#[test]
fn startup_input_preserves_focus_modifier_and_ime_order() {
    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    let events = vec![
        WindowEvent::Focused(true),
        WindowEvent::ModifiersChanged(ModifiersState::SHIFT.into()),
        WindowEvent::Ime(Ime::Enabled),
        WindowEvent::Ime(Ime::Commit("A".into())),
        WindowEvent::ModifiersChanged(Modifiers::default()),
        WindowEvent::Focused(false),
    ];
    for event in &events {
        input.defer_input(event.clone()).unwrap();
    }
    let mut replayed = Vec::new();
    while let Some(event) = input.pop_front() {
        replayed.push(event);
    }
    assert_eq!(replayed, events);
    assert!(input.is_empty());
}

#[test]
fn startup_input_keeps_pointer_and_newer_keyboard_in_native_order() {
    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    input.defer_input(WindowEvent::Focused(true)).unwrap();
    let wheel = WindowEvent::MouseWheel {
        device_id: None,
        delta: MouseScrollDelta::LineDelta(0.0, -1.0),
        phase: TouchPhase::Moved,
        timestamp: None,
    };
    input.defer_input(wheel.clone()).unwrap();
    let commit = WindowEvent::Ime(Ime::Commit("after wheel".into()));
    input.defer_input(commit.clone()).unwrap();

    assert_eq!(input.pop_front(), Some(WindowEvent::Focused(true)));
    // Input arriving after GPU completion cannot overtake the wheel.
    let release = WindowEvent::ModifiersChanged(Modifiers::default());
    input.defer_input(release.clone()).unwrap();
    assert_eq!(input.pop_front(), Some(wheel));
    assert_eq!(input.pop_front(), Some(commit));
    assert_eq!(input.pop_front(), Some(release));
    assert!(input.is_empty());
}

#[test]
fn startup_input_replays_default_quit_chord_before_first_scene() {
    use winit::{
        event::{ElementState, KeyEvent},
        keyboard::{Key, KeyCode, KeyLocation, PhysicalKey},
    };

    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    let events = vec![
        WindowEvent::Focused(true),
        WindowEvent::MouseWheel {
            device_id: None,
            delta: MouseScrollDelta::LineDelta(0.0, -1.0),
            phase: TouchPhase::Moved,
            timestamp: None,
        },
        WindowEvent::ModifiersChanged(ModifiersState::CONTROL.into()),
        WindowEvent::KeyboardInput {
            device_id: None,
            event: KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_key: Key::Character("g".into()),
                text: Some("g".into()),
                location: KeyLocation::Standard,
                state: ElementState::Pressed,
                repeat: false,
                text_with_all_modifiers: Some("\u{7}".into()),
                key_without_modifiers: Key::Character("g".into()),
            },
            is_synthetic: false,
            timestamp: None,
        },
        WindowEvent::ModifiersChanged(Modifiers::default()),
        WindowEvent::Focused(false),
    ];
    for event in &events {
        input.defer_input(event.clone()).unwrap();
    }

    // Native identity is ready; no scene has arrived. Ordinary pointer
    // delivery has an explicit Unpresented fallback, so it must not hold
    // the later Ctrl-G away from the input bridge's quit-request path.
    // Replaying the whole FIFO also cooks G after its actual Ctrl state
    // and preserves the subsequent release and focus transition.
    let mut replayed = Vec::new();
    while let Some(event) = input.pop_front() {
        replayed.push(event);
    }
    assert_eq!(replayed, events);
    assert!(input.is_empty());
}

#[test]
fn startup_input_keeps_lifecycle_events_synchronous() {
    let mut input = StartupInputReplay::new(WindowId::from_raw(1));
    let commit = WindowEvent::Ime(Ime::Commit("owned input".into()));
    input.defer_input(commit.clone()).unwrap();
    for event in [
        WindowEvent::CloseRequested,
        WindowEvent::Destroyed,
        WindowEvent::SurfaceResized(winit::dpi::PhysicalSize::new(0, 0)),
        WindowEvent::RedrawRequested,
    ] {
        assert_eq!(input.defer_input(event.clone()), Err(event));
    }
    assert_eq!(input.pop_front(), Some(commit));
    assert!(input.is_empty());
}

fn adapter_info(backend: wgpu::Backend, driver: &str, driver_info: &str) -> wgpu::AdapterInfo {
    let mut info = wgpu::AdapterInfo::new(wgpu::DeviceType::Cpu, backend);
    info.driver = driver.into();
    info.driver_info = driver_info.into();
    info
}

#[test]
fn mesa_version_reads_distribution_and_devel_strings() {
    assert_eq!(
        mesa_version("Mesa 23.2.1-1ubuntu3.1~22.04.4 (LLVM 15.0.7)"),
        Some((23, 2, 1))
    );
    assert_eq!(
        mesa_version("Mesa 24.1.0-devel (git-1a2b3c4d)"),
        Some((24, 1, 0))
    );
    assert_eq!(mesa_version("Mesa 24.0"), None);
    assert_eq!(mesa_version("NVIDIA 550.54.14"), None);
}

#[test]
fn lavapipe_drops_x11_frames_before_mesa_24_0_2() {
    use wgpu::Backend::{Gl, Vulkan};
    const UBUNTU_22_04: &str = "Mesa 23.2.1-1ubuntu3.1~22.04.4 (LLVM 15.0.7)";
    for (backend, driver, driver_info, drops) in [
        (Vulkan, "llvmpipe", UBUNTU_22_04, true),
        (Vulkan, "llvmpipe", "Mesa 24.0.1", true),
        (Vulkan, "llvmpipe", "Mesa 24.0.2", false),
        (Vulkan, "llvmpipe", "Mesa 25.1.4", false),
        // GL llvmpipe presents through GLX/EGL, the fallback this rule selects.
        (Gl, "llvmpipe", UBUNTU_22_04, false),
        // Hardware Mesa drivers present through DRI3, not the software path.
        (Vulkan, "Intel open-source Mesa driver", UBUNTU_22_04, false),
        // An unreadable version keeps the adapter.
        (Vulkan, "llvmpipe", "", false),
    ] {
        let info = adapter_info(backend, driver, driver_info);
        assert_eq!(
            lavapipe_drops_x11_frames(&info),
            drops,
            "{backend:?} {driver} {driver_info:?}"
        );
    }
}

fn candidate(
    name: &str,
    vendor: u32,
    device: u32,
    device_type: wgpu::DeviceType,
) -> StartupAdapterCandidate {
    StartupAdapterCandidate {
        identity: AdapterIdentity {
            backend: wgpu::Backend::Vulkan,
            vendor,
            device,
            name: name.to_string(),
        },
        device_type,
        surface_compatible: true,
    }
}

fn high_performance() -> wgpu::PowerPreference {
    wgpu::PowerPreference::HighPerformance
}

#[test]
fn excluded_discrete_yields_the_integrated_adapter() {
    let adapters = [
        candidate("Quadro P620", 0x10de, 0x1cb6, wgpu::DeviceType::DiscreteGpu),
        candidate("Intel UHD", 0x8086, 0x3e92, wgpu::DeviceType::IntegratedGpu),
    ];
    let pick = choose_startup_adapter(
        &adapters,
        high_performance(),
        &StartupAdapterChoice::SkipFailed {
            excluded: adapters[0].identity.clone(),
        },
    )
    .expect("an integrated adapter remains");
    assert_eq!(pick.index, 1);
    assert!(!pick.software);
    assert!(pick.warning.is_none());
}

#[test]
fn excluded_sole_discrete_yields_none() {
    let adapters = [candidate(
        "Quadro P620",
        0x10de,
        0x1cb6,
        wgpu::DeviceType::DiscreteGpu,
    )];
    let pick = choose_startup_adapter(
        &adapters,
        high_performance(),
        &StartupAdapterChoice::SkipFailed {
            excluded: adapters[0].identity.clone(),
        },
    );
    assert!(
        pick.is_none(),
        "the only adapter already failed; do not select it again"
    );
}

#[test]
fn no_hardware_left_yields_cpu_with_a_warning() {
    let adapters = [
        candidate("Quadro P620", 0x10de, 0x1cb6, wgpu::DeviceType::DiscreteGpu),
        candidate("llvmpipe", 0x10005, 0, wgpu::DeviceType::Cpu),
    ];
    let pick = choose_startup_adapter(
        &adapters,
        high_performance(),
        &StartupAdapterChoice::SkipFailed {
            excluded: adapters[0].identity.clone(),
        },
    )
    .expect("a software adapter remains");
    assert_eq!(pick.index, 1);
    assert!(pick.software);
    let warning = pick.warning.expect("software fallback warns");
    assert!(
        warning.contains("llvmpipe"),
        "warning should name the software adapter, got {warning}"
    );
}

#[test]
fn cpu_is_not_chosen_while_hardware_remains() {
    let adapters = [
        candidate("Quadro P620", 0x10de, 0x1cb6, wgpu::DeviceType::DiscreteGpu),
        candidate("Intel UHD", 0x8086, 0x3e92, wgpu::DeviceType::IntegratedGpu),
        candidate("llvmpipe", 0x10005, 0, wgpu::DeviceType::Cpu),
    ];
    let pick = choose_startup_adapter(&adapters, high_performance(), &StartupAdapterChoice::Ranked)
        .expect("hardware remains");
    assert_eq!(pick.index, 0);
    assert!(!pick.software);
    assert!(pick.warning.is_none());
}

fn intel(
    name: &str,
    backend: wgpu::Backend,
    device_type: wgpu::DeviceType,
) -> StartupAdapterCandidate {
    let mut adapter = candidate("placeholder", 0x8086, 0x8a52, device_type);
    adapter.identity.backend = backend;
    adapter.identity.name = name.to_string();
    adapter
}

#[test]
fn windows_prefers_dx12_over_vulkan_for_the_same_intel_device() {
    let adapters = [
        intel(
            "Intel Iris Plus (Vulkan)",
            wgpu::Backend::Vulkan,
            wgpu::DeviceType::IntegratedGpu,
        ),
        intel(
            "Microsoft Basic Render Driver",
            wgpu::Backend::Dx12,
            wgpu::DeviceType::Cpu,
        ),
        intel(
            "Intel Iris Plus (DX12)",
            wgpu::Backend::Dx12,
            wgpu::DeviceType::IntegratedGpu,
        ),
    ];
    let pick = choose_startup_adapter_on(
        &adapters,
        high_performance(),
        &StartupAdapterChoice::Ranked,
        StartupOs::Windows,
        StartupBackendPolicy::Default,
    )
    .expect("DX12 hardware remains");
    assert_eq!(pick.index, 2, "DX12 wins over Vulkan for the same device");
    assert!(!pick.software);
    assert!(pick.warning.is_none());
}

#[test]
fn windows_default_startup_instance_is_dx12_only() {
    let plan = instance_backend_plan(
        StartupOs::Windows,
        StartupBackendPolicy::Default,
        wgpu::Backends::all(),
    );
    assert_eq!(plan[0].backends, wgpu::Backends::DX12);
    assert!(
        !plan[0].backends.contains(wgpu::Backends::GL),
        "GL stays off the default Windows instance"
    );
    assert!(
        !plan[0].backends.contains(wgpu::Backends::VULKAN),
        "Vulkan stays off the default Windows instance"
    );
    assert!(!plan[0].fallback);
    assert_eq!(
        startup_display_slot(plan[0].backends),
        StartupDisplaySlot::Omit
    );
}

#[test]
fn windows_startup_instance_honors_wgpu_backend() {
    let vulkan = instance_backend_plan(
        StartupOs::Windows,
        StartupBackendPolicy::Explicit(wgpu::Backend::Vulkan),
        wgpu::Backends::all(),
    );
    assert_eq!(
        vulkan,
        vec![InstanceBackendAttempt {
            backends: wgpu::Backends::VULKAN,
            fallback: false,
        }]
    );

    let gl = instance_backend_plan(
        StartupOs::Windows,
        StartupBackendPolicy::Explicit(wgpu::Backend::Gl),
        wgpu::Backends::all(),
    );
    assert_eq!(
        gl,
        vec![InstanceBackendAttempt {
            backends: wgpu::Backends::GL,
            fallback: false,
        }]
    );
    assert_eq!(
        startup_display_slot(gl[0].backends),
        StartupDisplaySlot::Attach
    );

    let limited = instance_backend_plan(
        StartupOs::Windows,
        StartupBackendPolicy::Limited(wgpu::Backends::DX12 | wgpu::Backends::VULKAN),
        wgpu::Backends::all(),
    );
    assert_eq!(
        limited,
        vec![InstanceBackendAttempt {
            backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
            fallback: false,
        }]
    );
}

#[test]
fn windows_startup_instance_falls_back_gl_then_vulkan() {
    let plan = instance_backend_plan(
        StartupOs::Windows,
        StartupBackendPolicy::Default,
        wgpu::Backends::all(),
    );
    assert_eq!(
        plan,
        vec![
            InstanceBackendAttempt {
                backends: wgpu::Backends::DX12,
                fallback: false,
            },
            InstanceBackendAttempt {
                backends: wgpu::Backends::GL,
                fallback: true,
            },
            InstanceBackendAttempt {
                backends: wgpu::Backends::VULKAN,
                fallback: true,
            },
        ]
    );
    assert_eq!(
        startup_display_slot(plan[1].backends),
        StartupDisplaySlot::Attach
    );
    assert_eq!(
        startup_display_slot(plan[2].backends),
        StartupDisplaySlot::Omit
    );

    // Microsoft Basic Render is Cpu. It is not a usable adapter, so GL is next.
    assert_eq!(
        choose_startup_instance(&[
            vec![wgpu::DeviceType::Cpu],
            vec![wgpu::DeviceType::IntegratedGpu],
            vec![wgpu::DeviceType::DiscreteGpu],
        ]),
        Some(1)
    );
    assert_eq!(
        choose_startup_instance(&[vec![], vec![], vec![wgpu::DeviceType::IntegratedGpu],]),
        Some(2)
    );
    assert_eq!(
        choose_startup_instance(&[
            vec![wgpu::DeviceType::IntegratedGpu, wgpu::DeviceType::Cpu],
            vec![wgpu::DeviceType::DiscreteGpu],
        ]),
        Some(0)
    );
    // No hardware anywhere: keep the first software adapter.
    assert_eq!(
        choose_startup_instance(&[
            vec![wgpu::DeviceType::Cpu],
            vec![],
            vec![wgpu::DeviceType::Cpu],
        ]),
        Some(0)
    );
    assert_eq!(choose_startup_instance(&[vec![], vec![], vec![]]), None);
}

#[test]
fn other_os_startup_instance_keeps_descriptor_backends() {
    let backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
    let plan = instance_backend_plan(StartupOs::Other, StartupBackendPolicy::Default, backends);
    assert_eq!(
        plan,
        vec![InstanceBackendAttempt {
            backends,
            fallback: false,
        }]
    );
}

#[test]
fn startup_watch_is_silent_before_ten_seconds() {
    let phase = StartupWatchPhase::Device {
        name: "Quadro P620".to_string(),
        driver: "NVK".to_string(),
    };
    assert!(startup_watch_warning(Duration::from_millis(9_900), false, &phase).is_none());
}

#[test]
fn startup_watch_warns_once_at_ten_seconds() {
    let phase = StartupWatchPhase::Device {
        name: "Quadro P620".to_string(),
        driver: "NVK".to_string(),
    };
    let warning =
        startup_watch_warning(Duration::from_secs(10), false, &phase).expect("one warning at 10s");
    assert!(warning.contains("Quadro P620"), "{warning}");
    assert!(warning.contains("NVK"), "{warning}");
    assert!(warning.contains("WGPU_BACKEND=gl"), "{warning}");
    assert!(warning.contains("NEOMACS_GPU=integrated"), "{warning}");
    assert!(startup_watch_warning(Duration::from_secs(30), true, &phase).is_none());
}

#[test]
fn startup_watch_names_enumeration_before_an_adapter_exists() {
    let warning = startup_watch_warning(
        Duration::from_secs(10),
        false,
        &StartupWatchPhase::Enumeration,
    )
    .expect("enumeration warning at 10s");
    assert!(warning.contains("enumeration"), "{warning}");
    assert!(warning.contains("WGPU_BACKEND=gl"), "{warning}");
    assert!(warning.contains("NEOMACS_GPU=integrated"), "{warning}");
}

#[test]
fn integrated_scope_drops_discrete_quadro_and_keeps_intel() {
    let adapters = [
        candidate("Quadro P620", 0x10de, 0x1cb6, wgpu::DeviceType::DiscreteGpu),
        candidate("Intel UHD", 0x8086, 0x3e92, wgpu::DeviceType::IntegratedGpu),
    ];
    let decision = integrated_startup_scope(&adapters, StartupGpuScope::Integrated);
    assert_eq!(decision.indexes, vec![1]);
    assert_eq!(decision.skipped_names, vec!["Quadro P620".to_string()]);
    assert!(!decision.restored_discrete);
}

#[test]
fn integrated_scope_restores_discrete_when_nothing_else_supports_the_surface() {
    let mut adapters = [
        candidate("Quadro P620", 0x10de, 0x1cb6, wgpu::DeviceType::DiscreteGpu),
        candidate("Intel UHD", 0x8086, 0x3e92, wgpu::DeviceType::IntegratedGpu),
    ];
    adapters[1].surface_compatible = false;
    let decision = integrated_startup_scope(&adapters, StartupGpuScope::Integrated);
    assert!(decision.restored_discrete);
    assert_eq!(decision.indexes, vec![0, 1]);
    assert_eq!(decision.skipped_names, vec!["Quadro P620".to_string()]);
}

#[test]
fn any_scope_keeps_discrete_and_integrated() {
    let adapters = [
        candidate("Quadro P620", 0x10de, 0x1cb6, wgpu::DeviceType::DiscreteGpu),
        candidate("Intel UHD", 0x8086, 0x3e92, wgpu::DeviceType::IntegratedGpu),
    ];
    let decision = integrated_startup_scope(&adapters, StartupGpuScope::Any);
    assert_eq!(decision.indexes, vec![0, 1]);
    assert!(decision.skipped_names.is_empty());
    assert!(!decision.restored_discrete);
}

#[test]
fn explicit_vulkan_backend_keeps_vulkan_on_windows() {
    let adapters = [
        intel(
            "Intel Iris Plus (DX12)",
            wgpu::Backend::Dx12,
            wgpu::DeviceType::IntegratedGpu,
        ),
        intel(
            "Intel Iris Plus (Vulkan)",
            wgpu::Backend::Vulkan,
            wgpu::DeviceType::IntegratedGpu,
        ),
    ];
    let pick = choose_startup_adapter_on(
        &adapters,
        high_performance(),
        &StartupAdapterChoice::Ranked,
        StartupOs::Windows,
        StartupBackendPolicy::Explicit(wgpu::Backend::Vulkan),
    )
    .expect("explicit Vulkan remains");
    assert_eq!(pick.index, 1);
    assert_eq!(adapters[pick.index].identity.backend, wgpu::Backend::Vulkan);
}
