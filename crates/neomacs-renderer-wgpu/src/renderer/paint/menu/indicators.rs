//! Scale-independent menu marks. No font is required for control geometry.
use crate::vertex::RectVertex;
use neomacs_display_protocol::menu::{MenuCheckState, MenuIndicator};

fn triangle(out: &mut Vec<RectVertex>, points: [[f32; 2]; 3], color: [f32; 4]) {
    out.extend(points.map(|position| RectVertex { position, color }));
}

pub(super) fn submenu_arrow(
    out: &mut Vec<RectVertex>,
    right: f32,
    y: f32,
    row: f32,
    color: [f32; 4],
) {
    let size = row * 0.35;
    let top = y + (row - size) * 0.5;
    triangle(
        out,
        [
            [right - size, top],
            [right, top + size * 0.5],
            [right - size, top + size],
        ],
        color,
    );
}

fn stroke(out: &mut Vec<RectVertex>, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 4]) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy);
    if length == 0.0 {
        return;
    }
    let [nx, ny] = [-dy / length * width * 0.5, dx / length * width * 0.5];
    let p = [a[0] + nx, a[1] + ny];
    let q = [a[0] - nx, a[1] - ny];
    let r = [b[0] + nx, b[1] + ny];
    let s = [b[0] - nx, b[1] - ny];
    triangle(out, [p, q, r], color);
    triangle(out, [q, s, r], color);
}

pub(super) fn paint(
    out: &mut Vec<RectVertex>,
    indicator: MenuIndicator,
    x: f32,
    y: f32,
    row: f32,
    color: [f32; 4],
) {
    let size = row * 0.58;
    let y = y + (row - size) * 0.5;
    let width = (size * 0.1).max(1.0);
    match indicator {
        MenuIndicator::None => {}
        MenuIndicator::Toggle(state) => {
            let corners = [[x, y], [x + size, y], [x + size, y + size], [x, y + size]];
            for i in 0..4 {
                stroke(out, corners[i], corners[(i + 1) % 4], width, color);
            }
            if state == MenuCheckState::On {
                let elbow = [x + size * 0.43, y + size * 0.73];
                stroke(
                    out,
                    [x + size * 0.18, y + size * 0.48],
                    elbow,
                    width * 1.4,
                    color,
                );
                stroke(
                    out,
                    elbow,
                    [x + size * 0.84, y + size * 0.22],
                    width * 1.4,
                    color,
                );
            }
        }
        MenuIndicator::Radio(state) => {
            let center = [x + size * 0.5, y + size * 0.5];
            let radius = size * 0.5;
            for i in 0..24 {
                let a = i as f32 * std::f32::consts::TAU / 24.0;
                let b = (i + 1) as f32 * std::f32::consts::TAU / 24.0;
                let point =
                    |angle: f32, r: f32| [center[0] + angle.cos() * r, center[1] + angle.sin() * r];
                stroke(out, point(a, radius), point(b, radius), width, color);
                if state == MenuCheckState::On {
                    triangle(
                        out,
                        [center, point(a, radius * 0.5), point(b, radius * 0.5)],
                        color,
                    );
                }
            }
        }
    }
}
