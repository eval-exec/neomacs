//! Native target readiness, independent of Lisp timers and GPU readback.

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
pub(super) struct MappedEditor {
    _node_id: u64,
}

pub(super) fn wait_for_mapped_editor(
    runtime: &Path,
    editor: &mut Child,
    artifacts: &Path,
) -> MappedEditor {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut last_tree = None;
    loop {
        assert!(
            editor.try_wait().unwrap().is_none(),
            "editor exited before its native view mapped: {artifacts:?}"
        );
        let observation = match get_tree(runtime) {
            Ok(tree) => {
                if let Some(mapped) = mapped_editor(&tree, editor.id()) {
                    fs::write(
                        artifacts.join("mapped-editor.json"),
                        serde_json::to_vec_pretty(&tree).unwrap(),
                    )
                    .unwrap();
                    return mapped;
                }
                last_tree = Some(tree);
                "no visible owned native view covers the injection point".to_owned()
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

fn mapped_editor(node: &Value, pid: u32) -> Option<MappedEditor> {
    // Sway creates a view's container and populates its pid in view_map,
    // after the native surface maps. GET_TREE reports pid/visible only for
    // windows: https://man.archlinux.org/man/sway-ipc.7.en#4._GET_TREE
    // Require our native Wayland view and the actual absolute injection point.
    if node["pid"].as_u64() == Some(u64::from(pid))
        && node["visible"].as_bool() == Some(true)
        && node["shell"].as_str() == Some("xdg_shell")
        && covers_injection_point(&node["rect"])
        && let Some(id) = node["id"].as_u64()
    {
        return Some(MappedEditor { _node_id: id });
    }
    ["nodes", "floating_nodes"]
        .into_iter()
        .find_map(|children| {
            node[children]
                .as_array()?
                .iter()
                .find_map(|child| mapped_editor(child, pid))
        })
}

fn covers_injection_point(rect: &Value) -> bool {
    let Some((x, y, width, height)) =
        rect["x"]
            .as_i64()
            .zip(rect["y"].as_i64())
            .and_then(|(x, y)| {
                rect["width"]
                    .as_i64()
                    .zip(rect["height"].as_i64())
                    .map(|(width, height)| (x, y, width, height))
            })
    else {
        return false;
    };
    width > 0
        && height > 0
        && x <= 200
        && y <= 200
        && x.checked_add(width).is_some_and(|right| right > 200)
        && y.checked_add(height).is_some_and(|bottom| bottom > 200)
}

#[test]
fn mapped_input_target_requires_a_visible_owned_native_window() {
    // A complete GPU PNG and fresh Lisp samples cannot substitute for this
    // external witness. Other processes and hidden/non-native views do not
    // authorize injection, including when nested among floating containers.
    let mut tree = serde_json::json!({
        "nodes": [{"pid": 99, "id": 1, "visible": true, "shell": "xdg_shell",
                   "rect": {"x": 0, "y": 0, "width": 1000, "height": 700}}],
        "floating_nodes": [{"pid": 42, "id": 2, "visible": false, "shell": "xdg_shell",
                            "rect": {"x": 0, "y": 0, "width": 1000, "height": 700}}]
    });
    assert!(mapped_editor(&tree, 42).is_none());
    let own = &mut tree["floating_nodes"][0];
    own["visible"] = Value::Bool(true);
    own["shell"] = Value::String("xwayland".to_owned());
    assert!(mapped_editor(&tree, 42).is_none());
    tree["floating_nodes"][0]["shell"] = Value::String("xdg_shell".to_owned());
    assert!(mapped_editor(&tree, 42).is_some());
    tree["floating_nodes"][0]["rect"]["width"] = Value::from(100);
    assert!(mapped_editor(&tree, 42).is_none());
}
