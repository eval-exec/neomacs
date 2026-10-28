use crate::wayland_readiness::PointerTarget;
use std::{os::unix::net::UnixStream, path::Path};
use wayland_client::{
    Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_pointer, wl_registry},
};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

pub struct State;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
delegate_noop!(State: ignore ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ignore ZwlrVirtualPointerV1);

pub(super) struct Unmapped;

/// Keep one device alive throughout the gesture, just like a real trackpad.
/// Only a compositor-observed mapped editor enables native input injection.
pub(super) struct Trackpad<Readiness = PointerTarget> {
    pointer: ZwlrVirtualPointerV1,
    queue: EventQueue<State>,
    clock: u32,
    readiness: Readiness,
}
impl Trackpad<Unmapped> {
    pub fn connect(socket: &Path) -> Self {
        let connection = Connection::from_socket(UnixStream::connect(socket).unwrap()).unwrap();
        let (globals, mut queue) = registry_queue_init::<State>(&connection).unwrap();
        let qh = queue.handle();
        let manager: ZwlrVirtualPointerManagerV1 = globals.bind(&qh, 1..=2, ()).unwrap();
        let pointer = manager.create_virtual_pointer(None, &qh, ());
        queue.roundtrip(&mut State).unwrap();
        Self {
            pointer,
            queue,
            clock: 1,
            readiness: Unmapped,
        }
    }

    pub fn for_pointer_target(self, target: PointerTarget) -> Trackpad {
        Trackpad {
            pointer: self.pointer,
            queue: self.queue,
            clock: self.clock,
            readiness: target,
        }
    }
}

impl Trackpad {
    pub fn move_to_body(&mut self) {
        let (x, y) = self.readiness.coordinates();
        self.pointer.motion_absolute(self.clock, x, y, 1000, 700);
        self.pointer.frame();
        self.queue.roundtrip(&mut State).unwrap();
        self.clock += 16;
    }
    pub fn scroll(&mut self, pixels: f64) {
        self.pointer.axis_source(wl_pointer::AxisSource::Finger);
        self.pointer
            .axis(self.clock, wl_pointer::Axis::VerticalScroll, pixels);
        self.pointer.frame();
        self.queue.roundtrip(&mut State).unwrap();
        self.clock += 16;
    }
    pub fn wheel(&mut self) {
        self.pointer.axis_source(wl_pointer::AxisSource::Wheel);
        self.pointer
            .axis_discrete(self.clock, wl_pointer::Axis::VerticalScroll, 15.0, 1);
        self.pointer.frame();
        self.queue.roundtrip(&mut State).unwrap();
        self.clock += 16;
    }
}
