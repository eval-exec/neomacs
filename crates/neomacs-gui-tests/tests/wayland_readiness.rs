//! Native target readiness, independent of Lisp timers and GPU readback.
#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::{fs::FileTypeExt, net::UnixStream},
    path::Path,
    process::Child,
    thread,
    time::{Duration, Instant},
};

/// A visible native view of this editor exists in the owned compositor.
/// Private construction prevents injecting into a merely Lisp-ready frame.
pub(crate) struct MappedEditor {
    _node_id: u64,
    rect: NativeRect,
}

/// The mapped editor also covers the chosen absolute pointer position.
pub(crate) struct PointerTarget {
    _editor: MappedEditor,
    x: u32,
    y: u32,
}

impl PointerTarget {
    pub(crate) fn coordinates(&self) -> (u32, u32) {
        (self.x, self.y)
    }
}

impl MappedEditor {
    fn pointer_target(self, x: u32, y: u32) -> Option<PointerTarget> {
        self.rect.contains(x, y).then_some(PointerTarget {
            _editor: self,
            x,
            y,
        })
    }
}

#[derive(Clone, Copy)]
enum EditorOwner {
    ExactPid(u32),
    DirectChildOf(u32),
}

impl EditorOwner {
    fn contains(self, pid: u32) -> bool {
        match self {
            Self::ExactPid(expected) => pid == expected,
            Self::DirectChildOf(parent) => {
                fs::read_to_string(format!("/proc/{pid}/status"))
                    .ok()
                    .and_then(|status| {
                        status.lines().find_map(|line| {
                            line.strip_prefix("PPid:")
                                .and_then(|value| value.trim().parse::<u32>().ok())
                        })
                    })
                    == Some(parent)
            }
        }
    }
}

/// GuiTestPlan's runner owns the Child privately. In this private compositor,
/// require the mapped native view to belong to a direct child of this test;
/// never accept a title, an ambient desktop window, or another process's PID.
pub(crate) fn wait_for_mapped_child(runtime: &Path, artifacts: &Path) -> MappedEditor {
    wait_for_target(
        runtime,
        EditorOwner::DirectChildOf(std::process::id()),
        artifacts,
        || true,
        Some,
    )
}

pub(crate) fn wait_for_pointer_target(
    runtime: &Path,
    editor: &mut Child,
    artifacts: &Path,
    x: u32,
    y: u32,
) -> PointerTarget {
    let owner = EditorOwner::ExactPid(editor.id());
    wait_for_target(
        runtime,
        owner,
        artifacts,
        || editor.try_wait().unwrap().is_none(),
        |mapped| mapped.pointer_target(x, y),
    )
}

fn wait_for_target<T>(
    runtime: &Path,
    owner: EditorOwner,
    artifacts: &Path,
    mut alive: impl FnMut() -> bool,
    proof: impl Fn(MappedEditor) -> Option<T>,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_tree = None;
    loop {
        assert!(
            alive(),
            "editor exited before its native view mapped: {artifacts:?}"
        );
        let observation = match get_tree(runtime) {
            Ok(tree) => {
                if let Some(target) = find_target(&tree, owner, &proof) {
                    fs::write(
                        artifacts.join("mapped-editor.json"),
                        serde_json::to_vec_pretty(&tree).unwrap(),
                    )
                    .unwrap();
                    return target;
                }
                last_tree = Some(tree);
                "no visible owned native view satisfies the input target".to_owned()
            }
            Err(error) => error.to_string(),
        };
        if Instant::now() >= deadline {
            if let Some(tree) = last_tree {
                let _ = fs::write(
                    artifacts.join("unmapped-editor.json"),
                    serde_json::to_vec_pretty(&tree).unwrap(),
                );
            }
            panic!("native input target not ready: {observation}; {artifacts:?}");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn get_tree(runtime: &Path) -> io::Result<Value> {
    // DisplaySession deliberately exports no ambient SWAYSOCK. Discover only
    // the IPC endpoint inside this session's held private runtime directory.
    let socket = fs::read_dir(runtime)?
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_name().to_string_lossy().starts_with("sway-ipc.")
                && entry.file_type().is_ok_and(|kind| kind.is_socket())
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "owned Sway IPC socket absent"))?;
    let mut stream = UnixStream::connect(runtime.join(socket.file_name()))?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_millis(250)))?;
    // sway-ipc(7): native-endian length/type, GET_TREE=4, empty request body.
    let mut request = Vec::from(*b"i3-ipc");
    request.extend_from_slice(&0_u32.to_ne_bytes());
    request.extend_from_slice(&4_u32.to_ne_bytes());
    stream.write_all(&request)?;
    let mut header = [0_u8; 14];
    stream.read_exact(&mut header)?;
    let length = u32::from_ne_bytes(header[6..10].try_into().unwrap());
    let kind = u32::from_ne_bytes(header[10..14].try_into().unwrap());
    if &header[..6] != b"i3-ipc" || kind != 4 || length > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Sway GET_TREE reply",
        ));
    }
    let mut body = vec![0_u8; length as usize];
    stream.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn find_target<T>(
    node: &Value,
    owner: EditorOwner,
    proof: &impl Fn(MappedEditor) -> Option<T>,
) -> Option<T> {
    // Sway populates pid and creates a view's container in view_map, after
    // its native surface maps. GET_TREE's pid/visible are window-only fields:
    // https://man.archlinux.org/man/sway-ipc.7.en#4._GET_TREE
    if let Some(pid) = node["pid"].as_u64().and_then(|pid| u32::try_from(pid).ok())
        && owner.contains(pid)
        && node["visible"].as_bool() == Some(true)
        && node["shell"].as_str() == Some("xdg_shell")
        && let Some(id) = node["id"].as_u64()
        && let Some(rect) = NativeRect::decode(&node["rect"])
        && let Some(target) = proof(MappedEditor { _node_id: id, rect })
    {
        return Some(target);
    }
    ["nodes", "floating_nodes"]
        .into_iter()
        .find_map(|children| {
            node[children]
                .as_array()?
                .iter()
                .find_map(|child| find_target(child, owner, proof))
        })
}

struct NativeRect {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

impl NativeRect {
    fn decode(rect: &Value) -> Option<Self> {
        let value = Self {
            x: rect["x"].as_i64()?,
            y: rect["y"].as_i64()?,
            width: rect["width"].as_i64()?,
            height: rect["height"].as_i64()?,
        };
        (value.width > 0 && value.height > 0).then_some(value)
    }

    fn contains(&self, x: u32, y: u32) -> bool {
        let (x, y) = (i64::from(x), i64::from(y));
        self.x <= x
            && self.y <= y
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|right| right > x)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom > y)
    }
}

#[test]
fn mapped_input_target_requires_a_visible_owned_native_window() {
    let mut tree = serde_json::json!({
        "nodes": [{"pid": 99, "id": 1, "visible": true, "shell": "xdg_shell",
                   "rect": {"x": 0, "y": 0, "width": 1000, "height": 700}}],
        "floating_nodes": [{"pid": 42, "id": 2, "visible": false, "shell": "xdg_shell",
                            "rect": {"x": 0, "y": 0, "width": 1000, "height": 700}}]
    });
    let owner = EditorOwner::ExactPid(42);
    assert!(find_target(&tree, owner, &Some).is_none());
    tree["floating_nodes"][0]["visible"] = Value::Bool(true);
    tree["floating_nodes"][0]["shell"] = Value::String("xwayland".to_owned());
    assert!(find_target(&tree, owner, &Some).is_none());
    tree["floating_nodes"][0]["shell"] = Value::String("xdg_shell".to_owned());
    assert!(find_target(&tree, owner, &Some).is_some());
}

#[test]
fn keyboard_mapping_and_pointer_coverage_are_separate_native_proofs() {
    let tree = serde_json::json!({"pid": 42, "id": 1, "visible": true, "shell": "xdg_shell",
                                "rect": {"x": 0, "y": 0, "width": 100, "height": 100}});
    let owner = EditorOwner::ExactPid(42);
    assert!(
        find_target(&tree, owner, &Some).is_some(),
        "a small mapped window accepts keyboard input"
    );
    assert!(find_target(&tree, owner, &|mapped| mapped.pointer_target(200, 200)).is_none());
    let target = find_target(&tree, owner, &|mapped| mapped.pointer_target(20, 30)).unwrap();
    assert_eq!(target.coordinates(), (20, 30));
}
