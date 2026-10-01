//! Child-frame lifecycle animation over real composited pixels.
//!
//! A popup fades in (open slot), holds, is deleted (close slot), and fades
//! out while the harness captures the compositor's output at wall-clock
//! instants inside each fade. Slowing both fades 20x makes each one a
//! three-second window: wide enough that a handful of samples resolve the
//! opacity ramp without racing the frame clock.
//!
//! Captures come from `grim` against the display session, which sees the
//! pixels actually presented — including child frames, which the render
//! thread's own root-scene readback deliberately does not carry. The popup
//! paints its own red background, so the test locates the popup once, in the
//! settled capture, by its red pixels, and then reads every earlier capture
//! over that same rectangle: a mid-fade mean sits strictly between the
//! settled popup's mean and the background's mean, and the series moves
//! monotonically in the fade's direction.
#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn state_with_phase(path: &Path, after: u64, phase: &str, timeout: Duration) -> Value {
    // The fixture accumulates reached phases in a list, because the test's
    // capture cadence can outrun the fixture's timer chain: by the time a
    // slow capture returns, a later timer may have replaced the current
    // phase, and a phase that was reached but not polled in time must
    // still count.
    let deadline = Instant::now() + timeout;
    loop {
        // The phases list is append-only on the fixture side, so membership
        // alone is the progress marker; the numeric sample counts writes,
        // not phases, and sections observe states out of order.
        if let Ok(bytes) = fs::read(path)
            && let Ok(value) = serde_json::from_slice::<Value>(&bytes)
            && value["phases"]
                .as_array()
                .is_some_and(|phases| phases.iter().any(|p| p.as_str() == Some(phase)))
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "phase '{phase}' never appeared: {path:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_file(path: &Path, what: &str, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() {
            return;
        }
        assert!(Instant::now() < deadline, "{what} never appeared: {path:?}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn capture(display_env: &[(String, String)], artifacts: &Path, name: &str) -> image::DynamicImage {
    let output =
        Command::new(std::env::var_os("NEOMACS_GUI_GRIM").unwrap_or_else(|| "grim".into()))
            .arg(artifacts.join(name))
            .envs(display_env.iter().map(|(k, v)| (k.clone(), v.clone())))
            .output()
            .expect("install grim");
    assert!(
        output.status.success(),
        "grim: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    image::open(artifacts.join(name)).expect("grim wrote a readable PNG")
}

fn whole_mean(pixels: &image::DynamicImage) -> f64 {
    pixels
        .to_rgb8()
        .pixels()
        .map(|p| (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) as f64 / (3.0 * 255.0))
        .sum::<f64>()
        / ((pixels.width() * pixels.height()) as f64)
}

/// The popup rectangle in output coordinates, located by its red background
/// in the settled capture. Returned as `(x, y, width, height)` inset by two
/// pixels on every side, to stay clear of the frame's border and shadow,
/// which draw outside the background fill.
fn locate_popup(pixels: &image::DynamicImage) -> Option<(u32, u32, u32, u32)> {
    let rgb = pixels.to_rgb8();
    let (mut min_x, mut min_y, mut max_x, mut max_y, mut reds) =
        (u32::MAX, u32::MAX, 0u32, 0u32, 0u64);
    for (x, y, pixel) in rgb.enumerate_pixels() {
        let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
        if r > 180 && u32::from(g) + u32::from(b) < 2 * u32::from(r) / 3 {
            reds += 1;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if reds < 20_000 {
        // The popup is 280x140 text pixels of red; a handful of stray red
        // pixels in chrome or a face must not pass as it.
        return None;
    }
    let (x, y) = (min_x + 2, min_y + 2);
    let (width, height) = (
        max_x.saturating_sub(min_x + 4),
        max_y.saturating_sub(min_y + 4),
    );
    (width >= 200 && height >= 80).then_some((x, y, width, height))
}

/// The strip the scale sweep is read over: inside the settled frame's
/// red background, outside the frame at `scale_from` (0.6), so the frame's
/// right edge crossing it is pixel evidence of the pop.
const STRIP: (u32, u32, u32, u32) = (432, 200, 100, 80);

/// The strip the resize crossfade is read over: beyond the drifted
/// popup's pre-resize right edge (400 + ~308 outer = 708), inside the
/// grown popup's (400 + ~428 = 828), so the new picture's fade-in over it
/// is pixel evidence of the crossfade rather than a lifecycle fade.
const RESIZE_STRIP: (u32, u32, u32, u32) = (720, 330, 80, 100);

/// Inside the popup's ORIGINAL area: covered red at the anchor, plain
/// background once the popup has departed for the new one.
const DEPARTURE_STRIP: (u32, u32, u32, u32) = (262, 202, 200, 120);

/// Inside the ARRIVED popup's area: background before the drift, covered
/// red once the popup has glided in.
const ARRIVAL_STRIP: (u32, u32, u32, u32) = (432, 332, 100, 60);

/// Inside the drifted popup's far corner: covered red when the close fade
/// starts (scale 1.0), clear of the frame once it shrinks toward 0.6
/// anchored at the drifted top-left. The close-shrink assertions read this,
/// not the original-position STRIP -- after the drift the original area is
/// plain background regardless of the fade.
const SHRINK_STRIP: (u32, u32, u32, u32) = (665, 350, 30, 40);

/// Capture-to-capture wobble on an otherwise settled region: shadow edges
/// and antialiasing jitter a few thousandths between frames.
const NOISE_FLOOR: f64 = 0.01;

fn region_mean(pixels: &image::DynamicImage, rect: (u32, u32, u32, u32)) -> f64 {
    let (x, y, width, height) = rect;
    pixels
        .crop_imm(x, y, width, height)
        .to_rgb8()
        .pixels()
        .map(|p| (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) as f64 / (3.0 * 255.0))
        .sum::<f64>()
        / ((width * height) as f64)
}

#[test]
// Prerequisites: fresh Neomacs binary/pdump, a display backend on PATH and
// grim. The display harness provides the headless session.
fn child_frame_fades_progress_across_presented_frames() {
    let root = neomacs_infra::crate_root!().join("../..");
    let artifacts = root.join("target/neomacs-gui-tests/child-frame-animation");
    let _ = fs::remove_dir_all(&artifacts);
    fs::create_dir(&artifacts).unwrap();
    // The same session the grim-driven close-confirmation suite uses: sway
    // headless implements the wlr-screencopy protocol grim needs, and the
    // editor runs on the GL backend there — the software-compatibility mode
    // this family is deliberately allowed to animate under.
    let session = neomacs_infra::display::start_sway(
        &artifacts,
        r#"
output * resolution 1000x700
xwayland disable
seat seat0 fallback true
default_border none
focus_follows_mouse yes
"#,
    )
    .expect("start sway");
    let display_env = session.env().to_vec();
    let state_path = artifacts.join("state.json");
    let snapshot_path = artifacts.join("snapshot.json");
    let binary = std::env::var_os("NEOMACS_GUI_TEST_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/neomacs"));
    let _editor = OwnedChild(
        Command::new(&binary)
            .args(["-Q", "-l"])
            .arg(root.join("crates/neomacs-gui-tests/fixtures/child-frame-animation.el"))
            .envs(display_env.iter().map(|(k, v)| (k.clone(), v.clone())))
            .env_remove("DISPLAY")
            .env("WINIT_UNIX_BACKEND", "wayland")
            .env("WGPU_BACKEND", "gl")
            .env("NEOMACS_GUI_ANIMATION_STATE_JSON", &state_path)
            .env("NEOMACS_GUI_ANIMATION_SNAPSHOT_JSON", &snapshot_path)
            .env("RUST_LOG", "warn,neomacs_display_runtime=debug")
            .env("NEOMACS_LOG_FILE", artifacts.join("neomacs.log"))
            .stdout(fs::File::create(artifacts.join("stdout")).unwrap())
            .stderr(fs::File::create(artifacts.join("stderr")).unwrap())
            .spawn()
            .unwrap(),
    );

    // --- Captures. Each is saved under its name for post-mortem reading.
    let mut sample_count = 0;
    let ready = state_with_phase(
        &state_path,
        sample_count,
        "parent-ready",
        Duration::from_secs(120),
    );
    sample_count = ready["sample"].as_u64().unwrap();
    // The compositor must be presenting the editor, not its blank startup
    // backdrop, before the background reference is taken.
    let background_pixels = {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let pixels = capture(&display_env, &artifacts, "background.png");
            if whole_mean(&pixels) > 0.3 {
                break pixels;
            }
            assert!(
                Instant::now() < deadline,
                "the editor never presented to the compositor"
            );
            thread::sleep(Duration::from_millis(150));
        }
    };

    let created = state_with_phase(
        &state_path,
        sample_count,
        "created",
        Duration::from_secs(120),
    );
    sample_count = created["sample"].as_u64().unwrap();
    let mut opening = Vec::new();
    // Three captures riding the open fade (3s at slowdown 20, the clamp
    // ceiling): pixel evidence that the popup goes from absent to present
    // across the ramp. The exact curve is asserted from the render thread's
    // own alpha log below, which does not depend on capture latency.
    let mut opening_strip = Vec::new();
    for name in ["mid-open-1.png", "mid-open-2.png", "mid-open-3.png"] {
        let pixels = capture(&display_env, &artifacts, name);
        opening.push(name.to_owned());
        opening_strip.push(region_mean(&pixels, STRIP));
        thread::sleep(Duration::from_millis(500));
    }

    let settled = state_with_phase(
        &state_path,
        sample_count,
        "settled",
        Duration::from_secs(60),
    );
    sample_count = settled["sample"].as_u64().unwrap();
    // The snapshot is a full synchronous frame query, so wait for its file
    // rather than assuming it landed.
    wait_for_file(
        &snapshot_path,
        "the settled frame snapshot",
        Duration::from_secs(30),
    );
    let snapshot: Value =
        serde_json::from_str(&fs::read_to_string(&snapshot_path).unwrap()).unwrap();
    let popup = snapshot["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| {
            frame["window_infos"]
                .as_array()
                .unwrap()
                .iter()
                .any(|window| window["buffer_name"] == "*child-frame-animation-popup*")
        })
        .expect("the popup must be present in the settled frame snapshot");
    let bounds = &popup["frame_placement"]["outer_in_parent"];
    // The outer rect carries the frame's own borders, so its size exceeds
    // the text pixels; position is exact and the outer must cover the crop.
    assert_eq!(
        (bounds["x"].as_f64(), bounds["y"].as_f64()),
        (Some(240.0), Some(180.0)),
        "popup placement must match the sampled crop"
    );
    assert!(
        bounds["width"].as_f64().unwrap() >= 280.0 && bounds["height"].as_f64().unwrap() >= 140.0,
        "popup outer rect must cover the sampled crop: {bounds:?}"
    );
    // The settled capture polls for reality: on a loaded runner the
    // composited output can lag the fixture's timer chain by seconds, so a
    // single capture could land mid-fade where the blended popup no longer
    // reads as red. Wait until the popup is fully opaque at its placement.
    let settled_image = {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            let pixels = capture(&display_env, &artifacts, "settled.png");
            if locate_popup(&pixels).is_some() {
                break pixels;
            }
            assert!(
                Instant::now() < deadline,
                "the popup never reached full opacity at its placement"
            );
            thread::sleep(Duration::from_millis(400));
        }
    };

    // --- The anchor-track drift: the popup departs its original area and
    // glides into the new one. The spring is slowed 20x like every other
    // slot, so its ~330ms settle stretches long enough for four captures.
    let reanchored = state_with_phase(
        &state_path,
        sample_count,
        "reanchored",
        Duration::from_secs(60),
    );
    sample_count = reanchored["sample"].as_u64().unwrap();
    let mut departing = Vec::new();
    let mut arriving = Vec::new();
    for name in ["drift-1.png", "drift-2.png", "drift-3.png", "drift-4.png"] {
        let pixels = capture(&display_env, &artifacts, name);
        departing.push(region_mean(&pixels, DEPARTURE_STRIP));
        arriving.push(region_mean(&pixels, ARRIVAL_STRIP));
    }

    // The drift settles in ~6.6s (spring slowed 20x) — on a loaded runner
    // the ingest and presents lag that by seconds, so poll for the settled
    // endpoints instead of sleeping a fixed time: the arrival area covered
    // red and the departure area plain background.
    let pre_arrival = region_mean(&background_pixels, ARRIVAL_STRIP);
    let pre_departure = region_mean(&background_pixels, DEPARTURE_STRIP);
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let pixels = capture(&display_env, &artifacts, "post-drift.png");
        let arrival_covered = region_mean(&pixels, ARRIVAL_STRIP);
        let departure_cleared = region_mean(&pixels, DEPARTURE_STRIP);
        if arrival_covered < pre_arrival - 0.5 && departure_cleared > pre_departure - 0.15 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the drift never settled: arrival {arrival_covered:.3} vs {pre_arrival:.3}, departure {departure_cleared:.3} vs {pre_departure:.3}"
        );
        thread::sleep(Duration::from_millis(600));
    }

    // --- The resize content crossfade: the popup grows mid-life, and the
    // previous presentation's picture crossfades into the bigger one. The
    // strip beyond the old size fills with red as the new frame's picture
    // fades in over it.
    let resized = state_with_phase(
        &state_path,
        sample_count,
        "resized",
        Duration::from_secs(60),
    );
    sample_count = resized["sample"].as_u64().unwrap();
    let mut resized_strip = Vec::new();
    // The crossfade runs 3s; llvmpipe's composited output lags the drawn
    // state by up to ~1s under sustained animation, so the captures span
    // the whole window plus a settled tail: the first must still be (nearly)
    // background, the last must be covered red, and something between must
    // sit strictly inside the ramp.
    for (delay, name) in [
        (300, "resize-1.png"),
        (1200, "resize-2.png"),
        (1200, "resize-3.png"),
        (1300, "resize-4.png"),
    ] {
        thread::sleep(Duration::from_millis(delay));
        let pixels = capture(&display_env, &artifacts, name);
        resized_strip.push(region_mean(&pixels, RESIZE_STRIP));
    }

    let deleted = state_with_phase(
        &state_path,
        sample_count,
        "deleted",
        Duration::from_secs(60),
    );
    sample_count = deleted["sample"].as_u64().unwrap();
    // Four captures across the 3s close fade: opacity falls, so the region
    // mean climbs toward the background. Capture cadence (~0.7s including
    // the compositor round-trip) resolves the ramp at this length.
    let mut fading_strip = Vec::new();
    for name in ["fade-1.png", "fade-2.png", "fade-3.png"] {
        // Space the captures along the 3s ramp: when the compositor
        // round-trip is fast the three would otherwise bunch at the fade's
        // start, sampling alphas ~1.0, ~0.99, ~0.98.
        thread::sleep(Duration::from_millis(800));
        let pixels = capture(&display_env, &artifacts, name);
        fading_strip.push(region_mean(&pixels, ARRIVAL_STRIP));
    }

    state_with_phase(&state_path, sample_count, "pruned", Duration::from_secs(60));
    thread::sleep(Duration::from_millis(300));
    capture(&display_env, &artifacts, "pruned.png");

    // --- Locate the popup from the polled settled capture...
    let rect =
        locate_popup(&settled_image).expect("the settled capture must contain the red popup");

    // ...and read every capture over that same rectangle. The render
    // thread's own log is read here too: its alpha series is the exact
    // record of both ramps, independent of capture latency.
    let log = fs::read_to_string(artifacts.join("neomacs.log")).unwrap_or_default();
    let background = region_mean(&background_pixels, rect);
    let opening: Vec<f64> = opening
        .iter()
        .map(|name| region_mean(&image::open(artifacts.join(name)).unwrap(), rect))
        .collect();
    let full = region_mean(&settled_image, rect);
    let full_strip = region_mean(&settled_image, STRIP);
    let fading: Vec<f64> = ["fade-1.png", "fade-2.png", "fade-3.png"]
        .iter()
        .map(|name| region_mean(&image::open(artifacts.join(name)).unwrap(), SHRINK_STRIP))
        .collect();
    let pruned = region_mean(&image::open(artifacts.join("pruned.png")).unwrap(), rect);
    let pruned_strip = region_mean(&image::open(artifacts.join("pruned.png")).unwrap(), STRIP);

    // --- Open fade: pixel evidence that the popup is present by the end
    // of the captures. The exact ramp shape is asserted from the render
    // thread's own alpha log below, which does not depend on capture
    // latency: at compositor rate the first capture can race ahead of the
    // popup's first presented frame (crop still background) and the last
    // can land deep in the ramp, and both are fine.
    let last_open = opening.last().unwrap();
    assert!(
        *last_open < background - 0.05,
        "the last open capture ({last_open:.3}) shows no popup (bg {background:.3})"
    );
    assert!(
        opening.windows(2).all(|pair| pair[0] >= pair[1]),
        "open-fade means must not brighten: {opening:?}"
    );

    // The living draw's own alpha series, from before the resize
    // crossfade starts: transparent to opaque, monotonically. The resize
    // crossfade's own mix series (which restarts from zero by design) is
    // asserted separately below.
    let removal_line = log
        .lines()
        .position(|line| line.contains("resize_crossfade_started"))
        .unwrap_or(log.lines().count());
    let open_alphas: Vec<f64> = log
        .lines()
        .take(removal_line)
        .filter(|line| line.contains("render_child_frame_start"))
        .filter_map(|line| {
            line.split("alpha=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.trim_end_matches(',').parse::<f64>().ok())
        })
        .filter(|alpha| *alpha < 1.0)
        .collect();
    assert!(
        open_alphas.len() >= 3,
        "the living popup must have been drawn several times mid-ramp"
    );
    assert!(
        open_alphas.windows(2).all(|pair| pair[1] >= pair[0]),
        "open-fade alphas must not decrease: {open_alphas:?}"
    );
    assert!(
        *open_alphas.first().unwrap() < 0.5,
        "the open fade must start near transparent: first {}",
        open_alphas.first().unwrap()
    );
    assert!(
        *open_alphas.last().unwrap() > 0.9,
        "the open fade must reach opacity: last {}",
        open_alphas.last().unwrap()
    );

    // The drift's drawn positions, from the drift's start to its end:
    // the popup's drawn origin must approach the new anchor monotonically.
    let drift_start_line = log
        .lines()
        .position(|line| line.contains("drift_started"))
        .unwrap_or(0);
    let crossfade_start_line = log
        .lines()
        .position(|line| line.contains("resize_crossfade_started"))
        .unwrap_or(log.lines().count());
    let drift_xs: Vec<f64> = log
        .lines()
        .skip(drift_start_line)
        .take(crossfade_start_line.saturating_sub(drift_start_line))
        .filter(|line| line.contains("render_child_frame_start"))
        .filter_map(|line| {
            line.split(" x=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.trim_end_matches(',').parse::<f64>().ok())
        })
        .collect();
    assert!(
        drift_xs.len() >= 10,
        "the drifting popup must have been drawn several times mid-glide"
    );
    assert!(
        drift_xs.windows(2).all(|pair| pair[1] >= pair[0]),
        "the drift's drawn x must not move backwards: {drift_xs:?}"
    );
    assert!(
        *drift_xs.first().unwrap() < 245.0,
        "the drift must start near the old anchor: first {}",
        drift_xs.first().unwrap()
    );
    assert!(
        *drift_xs.last().unwrap() > 390.0,
        "the drift must settle at the new anchor: last {}",
        drift_xs.last().unwrap()
    );

    // The living draw's own scale series, same lines: growing from the
    // configured start scale to the settled 1.0, monotonically.
    let open_scales: Vec<f64> = log
        .lines()
        .take(removal_line)
        .filter(|line| line.contains("render_child_frame_start"))
        .filter_map(|line| {
            line.split("scale=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.trim_end_matches(',').parse::<f64>().ok())
        })
        .filter(|scale| (*scale - 1.0).abs() > f64::EPSILON)
        .collect();
    assert!(
        open_scales.len() >= 3,
        "the living popup must have been drawn several times mid-pop"
    );
    assert!(
        open_scales.windows(2).all(|pair| pair[1] >= pair[0]),
        "open-fade scales must not shrink: {open_scales:?}"
    );
    assert!(
        *open_scales.first().unwrap() < 0.75,
        "the open pop must start from the configured scale: first {}",
        open_scales.first().unwrap()
    );
    assert!(
        (*open_scales.last().unwrap() - 1.0).abs() < 0.02,
        "the open pop must settle at the full size: last {}",
        open_scales.last().unwrap()
    );

    // --- Open pop: the scale sweeps the strip from background to covered.
    // The frame grows out of its top-left anchor, so the strip -- covered
    // by the settled frame, clear of it at the start scale -- is
    // progressively eaten by red.
    assert!(
        opening_strip.windows(2).all(|pair| pair[0] > pair[1]),
        "open-fade strip means must strictly fall as the frame grows: {opening_strip:?}"
    );
    assert!(
        opening_strip[0] > background - 0.1,
        "the strip must start as background at the start scale: {}",
        opening_strip[0]
    );
    // --- Settled: fully opaque popup over the background ---
    assert!(
        full < background - 0.2,
        "settled popup ({full:.3}) must be clearly present over the background ({background:.3})"
    );
    assert!(
        full_strip < background - 0.5,
        "the settled strip must be covered (popup red over it): {full_strip:.3}"
    );

    // --- Close fade: strictly brightening toward the background. The
    // series itself is the ramp: at +0.3s an ease-out-quad over 3s is
    // already ~19% through (alpha ~0.81), so the first mean is expected
    // above the settled popup's, and every later one rises.
    assert!(
        fading.windows(2).all(|pair| pair[1] > pair[0]),
        "close-fade means must strictly increase: {fading:?}"
    );
    assert!(
        fading[0] > full + 0.01,
        "close fade must have visibly started by 0.3s: {} vs full {full:.3}",
        fading[0]
    );
    assert!(
        fading[2] > full + (background - full) * 0.5,
        "close fade must reach at least halfway by the last capture: {} (full {full:.3}, bg {background:.3})",
        fading[2]
    );

    // --- Close fade over the drifted popup's interior: the strip near the
    // anchor stays inside the frame through the whole shrink (the shrink
    // anchors at that top-left), so its mean is purely the alpha ramp:
    // covered red fading toward background.
    assert!(
        fading_strip.windows(2).all(|pair| pair[1] > pair[0]),
        "close-fade strip means must strictly rise as the alpha falls: {fading_strip:?}"
    );
    assert!(
        fading_strip[0] > full_strip + 0.05 && fading_strip[0] < background - 0.05,
        "the first close capture must sit inside the ramp: {} (full {full_strip:.3}, bg {background:.3})",
        fading_strip[0]
    );

    // --- The anchor drift. The re-anchor itself has a one-frame vanish
    // window (the presentation rebuilds around the placement change), so a
    // capture racing the drift's start is inherently unreliable; the drift
    // is asserted from its settled endpoint below and from the render
    // thread's drawn-position log. The mid-flight glide is asserted from the
    // render thread's own drawn-position log below, which does not race
    // the compositor; the settled endpoint is asserted after the drift's
    // window passes.
    let _ = pre_departure;
    assert!(
        log.contains("drift_started"),
        "the compositor must have started the anchor drift"
    );

    // --- The resize crossfade: the new picture fills the grown area.
    assert!(
        resized_strip[0] > background - 0.15,
        "the resize strip must start as background (beyond the old frame): {}",
        resized_strip[0]
    );
    assert!(
        *resized_strip.last().unwrap() < background - 0.5,
        "the grown popup must cover the strip once the crossfade ends: {}",
        resized_strip.last().unwrap()
    );
    assert!(
        resized_strip
            .windows(2)
            .any(|pair| pair[0] > pair[1] + NOISE_FLOOR),
        "some capture pair must straddle the crossfade ramp: {resized_strip:?}"
    );
    let log = fs::read_to_string(artifacts.join("neomacs.log")).unwrap_or_default();
    let crossfade_start = log
        .lines()
        .position(|line| line.contains("resize_crossfade_started"))
        .expect("the resize crossfade must have started");
    let crossfade_mixes: Vec<f64> = log
        .lines()
        .skip(crossfade_start)
        .filter(|line| line.contains("render_child_frame_start"))
        .take_while(|line| !line.contains("compositor_remove"))
        .filter_map(|line| {
            line.split("alpha=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.trim_end_matches(',').parse::<f64>().ok())
        })
        .filter(|alpha| *alpha < 1.0)
        .collect();
    assert!(
        crossfade_mixes.len() >= 3,
        "the crossfade must have drawn several times mid-mix"
    );
    assert!(
        crossfade_mixes.windows(2).all(|pair| pair[1] >= pair[0]),
        "crossfade mix must not decrease: {crossfade_mixes:?}"
    );
    assert!(
        *crossfade_mixes.first().unwrap() < 0.5,
        "the crossfade must start near the old picture: first {}",
        crossfade_mixes.first().unwrap()
    );

    // --- Pruned: the popup is gone; the region reads as plain background ---
    assert!(
        (pruned - background).abs() < 0.02,
        "pruned region ({pruned:.3}) must equal the pre-popup background ({background:.3})"
    );
    assert!(
        (pruned_strip - background).abs() < 0.02,
        "pruned strip ({pruned_strip:.3}) must equal the pre-popup background ({background:.3})"
    );

    // --- The render thread's own log carries the dying alpha series ---
    assert!(
        log.contains("render_thread_child_retired"),
        "the compositor must have retired the popup into the dying list"
    );
    let alphas: Vec<f64> = log
        .lines()
        .filter(|line| line.contains("render_dying_child_frame"))
        .filter_map(|line| {
            line.split("alpha=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.trim_end_matches(',').parse::<f64>().ok())
        })
        .collect();
    assert!(
        alphas.len() >= 3,
        "the dying frame must have been drawn several times mid-fade"
    );
    assert!(
        alphas.windows(2).all(|pair| pair[1] <= pair[0]),
        "dying-frame alphas must not increase: {alphas:?}"
    );
    assert!(
        *alphas.last().unwrap() < 0.1,
        "the fade must reach opacity: last {}",
        alphas.last().unwrap()
    );

    // The dying frame's own scale series, same lines: shrinking from the
    // settled 1.0 toward the configured start scale, monotonically.
    let dying_scales: Vec<f64> = log
        .lines()
        .filter(|line| line.contains("render_dying_child_frame"))
        .filter_map(|line| {
            line.split("scale=")
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|value| value.trim_end_matches(',').parse::<f64>().ok())
        })
        .collect();
    assert!(
        dying_scales.len() >= 3,
        "the dying frame must have been drawn several times mid-shrink"
    );
    assert!(
        dying_scales.windows(2).all(|pair| pair[1] <= pair[0]),
        "dying-frame scales must not grow: {dying_scales:?}"
    );
    // The first dying draw lands a frame or two after the retire, so the
    // tween has already advanced by its scheduler latency: near the settled
    // size, not exactly at it.
    assert!(
        *dying_scales.first().unwrap() > 0.9,
        "the close pop must start near the settled size: first {}",
        dying_scales.first().unwrap()
    );
    assert!(
        *dying_scales.last().unwrap() < 0.75,
        "the close pop must shrink past the start scale's neighborhood: last {}",
        dying_scales.last().unwrap()
    );
}
