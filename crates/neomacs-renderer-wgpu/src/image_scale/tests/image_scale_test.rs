//! What the streaming scaler promises.
//!
//! Five contracts. The pixels are the Lanczos3 resample of the source — checked
//! against the definition written out the obvious way, which shares no code
//! with the streaming target. The rows come out in order, once each, and the
//! bands cut from the target tile it. A target that has not been fed every row
//! is not a finished image. The weight table a source asks for is the size the
//! bound that admits it promised. And the two properties the filter is chosen
//! for — an unreduced axis is not filtered at all, and a reduced step edge
//! keeps its contrast without the clamp hiding an unbounded overshoot.

use super::*;
use neomacs_display_protocol::{ImageNativeExtent, ImageRasterExtent};

/// A source whose every channel varies with both coordinates, so a sample
/// taken from the wrong place cannot pass as the right one.
fn source(width: u32, height: u32) -> Vec<u8> {
    (0..height)
        .flat_map(|y| {
            (0..width).flat_map(move |x| {
                [
                    (x % 251) as u8,
                    (y % 253) as u8,
                    ((x + y) % 241) as u8,
                    0xff,
                ]
            })
        })
        .collect()
}

fn target(native: (u32, u32), raster: (u32, u32)) -> Option<RasterTarget> {
    RasterTarget::new(
        ImageNativeExtent::new(native.0, native.1),
        ImageRasterExtent::new(raster.0, raster.1),
    )
}

/// Every row pushed in order, which is how a decode feeds it.
fn filled(native: (u32, u32), raster: (u32, u32)) -> RasterTarget {
    let pixels = source(native.0, native.1);
    let mut target = target(native, raster).expect("a target for a reducible extent");
    for row in pixels.chunks_exact(native.0 as usize * 4) {
        target.push_row(row);
    }
    target
}

/// `sin(pi * t) / (pi * t)`, with the removable singularity at zero taken.
fn sinc(t: f64) -> f64 {
    if t == 0.0 {
        1.0
    } else {
        let a = t * std::f64::consts::PI;
        a.sin() / a
    }
}

/// Lanczos's windowed sinc with a window of three, written from the definition
/// rather than from the constants the implementation folds it into.
fn lanczos3(distance: f64) -> f64 {
    let x = distance.abs();
    if x < 3.0 {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

/// One axis of the definition: the quantized weights of every tap that falls
/// inside the source, and their sum.
///
/// The taps are found by asking each input in turn whether its centre lies
/// within the kernel's support of the output's centre, which is the rule
/// stated as a rule rather than as the pair of inequalities an implementation
/// solves to get at it.
fn taps(centre: f64, scale: f64, input: u32) -> (Vec<(usize, i64)>, i64) {
    let weighted: Vec<(usize, f64)> = (0..input as usize)
        .map(|i| (i, ((i as f64 + 0.5) - centre).abs() / scale))
        .filter(|(_, distance)| *distance < 3.0)
        .map(|(i, distance)| (i, lanczos3(distance)))
        .collect();
    let sum: f64 = weighted.iter().map(|(_, weight)| weight).sum();
    let mut total = 0_i64;
    let quantized = weighted
        .into_iter()
        .map(|(i, weight)| {
            let weight = (weight * WEIGHT_ONE as f64 / sum).round() as i64;
            total += weight;
            (i, weight)
        })
        .collect();
    (quantized, total)
}

/// The resample the definition asks for, written the obvious way: every output
/// sample walks the inputs it covers and weights each by the kernel, one axis
/// at a time, holding the resampled rows in between.
///
/// Nothing here streams — no cursor, no accumulator, no buffer reused between
/// rows — so a target that filters the wrong row, drops a weight or counts one
/// twice disagrees with it.
///
/// Three rules belong to the definition rather than to the implementation that
/// follows them. Weights are normalized by the sum of the taps that actually
/// fall inside the source, which is what makes an edge an edge and what keeps a
/// constant source constant across one. An axis with as many outputs as inputs
/// is a **copy**: Lanczos3's taps at scale one happen to weigh `0, 0, 1, 0, 0`,
/// so filtering there would land on the same bytes *today*, but only while the
/// zeros survive quantization, and an image shown at its own size is exactly
/// the case this path has to keep exact by construction. And the first axis
/// hands the second a **mean**, not a level: the clamp that turns a mean into
/// an 8-bit sample is applied once, at the end, because applying it per axis
/// discards an excursion the second axis was going to cancel.
fn lanczos_resample(
    source: &[u8],
    (in_w, in_h): (u32, u32),
    (out_w, out_h): (u32, u32),
) -> Vec<u8> {
    let mean = |sum: i64, total: i64| (sum + total / 2).div_euclid(total) as i32;
    let round = |sum: i64, total: i64| (sum + total / 2).div_euclid(total).clamp(0, 255) as u8;

    let mut lines: Vec<i32> = Vec::with_capacity(in_h as usize * out_w as usize * 4);
    let x_scale = f64::from(in_w) / f64::from(out_w);
    for row in source.chunks_exact(in_w as usize * 4) {
        if in_w == out_w {
            lines.extend(row.iter().map(|sample| i32::from(*sample)));
            continue;
        }
        for o in 0..out_w {
            let (taps, total) = taps((o as f64 + 0.5) * x_scale, x_scale, in_w);
            let mut sums = [0_i64; 4];
            for (i, weight) in taps {
                for (c, sum) in sums.iter_mut().enumerate() {
                    *sum += weight * i64::from(row[i * 4 + c]);
                }
            }
            for sum in sums {
                lines.push(mean(sum, total));
            }
        }
    }

    let y_scale = f64::from(in_h) / f64::from(out_h);
    let mut out = vec![0_u8; out_w as usize * out_h as usize * 4];
    for column in 0..out_w as usize {
        for o in 0..out_h {
            let at = (o as usize * out_w as usize + column) * 4;
            if in_h == out_h {
                for c in 0..4 {
                    out[at + c] = u8::try_from(lines[at + c].clamp(0, 255)).expect("a level");
                }
                continue;
            }
            let (taps, total) = taps((f64::from(o) + 0.5) * y_scale, y_scale, in_h);
            let mut sums = [0_i64; 4];
            for (i, weight) in taps {
                for (c, sum) in sums.iter_mut().enumerate() {
                    *sum += weight * i64::from(lines[(i * out_w as usize + column) * 4 + c]);
                }
            }
            for (channel, sum) in out[at..at + 4].iter_mut().zip(sums) {
                *channel = round(sum, total);
            }
        }
    }
    out
}

/// The whole point: what comes out is the resample of what went in.
#[test]
fn a_target_holds_the_lanczos_resample_of_the_rows_it_was_given() {
    // Ratios that divide evenly, ratios that do not, a reduction on one axis
    // only, and a reduction that is nearly the identity.
    let cases = [
        ((37, 23), (11, 7)),
        ((64, 64), (16, 16)),
        ((100, 3), (33, 3)),
        ((3, 100), (3, 33)),
        ((50, 40), (49, 39)),
        ((17, 11), (1, 1)),
    ];
    for (native, raster) in cases {
        let target = filled(native, raster);
        assert_eq!(target.built(), raster.1, "{native:?} -> {raster:?}");
        let got = target
            .into_pixels()
            .expect("every row was pushed, so the target is whole");
        assert_eq!(
            got,
            lanczos_resample(&source(native.0, native.1), native, raster),
            "{native:?} -> {raster:?}: the target must be the filter the decode defines"
        );
    }
}

/// A floor of one output row per band means a source can be reduced far enough
/// that one output covers all of it, and an output row is closed by the last
/// source row inside its support — the one that was pushed last, which is
/// exactly the row a target that closed it early would lose.
///
/// The source makes that unmistakable: every row but the last is black, so an
/// output that dropped its final tap is zero where the filter says otherwise.
#[test]
fn a_whole_source_can_reduce_to_a_single_pixel() {
    let native = (8, 8);
    let mut pixels = vec![0_u8; native.0 as usize * native.1 as usize * 4];
    for x in 0..native.0 as usize {
        let at = ((native.1 as usize - 1) * native.0 as usize + x) * 4;
        pixels[at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    let mut target = target(native, (1, 1)).expect("a reducible extent");
    for row in pixels.chunks_exact(native.0 as usize * 4) {
        target.push_row(row);
    }
    let got = target.into_pixels().expect("a whole target");
    assert_eq!(
        got,
        lanczos_resample(&pixels, native, (1, 1)),
        "one output is the filter over every row, the last one included"
    );
    assert!(
        got[0] > 0,
        "an output that lost the last row's contribution would be black: got {got:?}"
    );
}

/// A constant source reduces to that constant, whatever the ratio: each output
/// is divided by the sum of the weights it actually used, so a filter that lost
/// or double-counted a weight would drift instead. The ratios include the
/// identity, a reduction on each axis alone, and a reduction to one pixel —
/// which is where the taps are most heavily clamped by the source's own edges.
#[test]
fn a_constant_source_reduces_to_the_same_constant() {
    let native = (97, 61);
    for raster in [(97, 61), (40, 40), (13, 7), (1, 1)] {
        let pixels: Vec<u8> = (0..native.0 * native.1)
            .flat_map(|_| [7u8, 200, 33, 255])
            .collect();
        let mut target = target(native, raster).expect("a reducible extent");
        for row in pixels.chunks_exact(native.0 as usize * 4) {
            target.push_row(row);
        }
        let got = target.into_pixels().expect("a whole target");
        assert!(
            got.chunks_exact(4).all(|texel| texel == [7, 200, 33, 255]),
            "{native:?} -> {raster:?}: a constant must reduce to itself"
        );
    }
}

/// The case of an image shown at its own size: the raster is the source, so the
/// preview during a decode is not an approximation of the finished pixels but
/// the finished pixels.
#[test]
fn a_target_the_size_of_its_source_reproduces_it_exactly() {
    let native = (40, 30);
    let target = filled(native, native);
    assert_eq!(target.built(), native.1);
    assert_eq!(
        target.into_pixels().expect("a whole target"),
        source(native.0, native.1),
        "a target the size of its source is its source"
    );
}

/// An axis with nothing to reduce is copied, not filtered.
///
/// Lanczos3's taps at scale one land on whole-texel offsets, where two of its
/// three sinc factors are zero, so filtering there would come out the same —
/// but the copy is what makes that a property of this type rather than of
/// rounding, and it is the case a preview during a decode has to get exactly
/// right. Each source row here is one flat colour and no two rows share one, so
/// a vertical filter of any weight at all would mix them, and a horizontal one
/// would not survive a flat row either.
#[test]
fn an_axis_that_is_not_reduced_is_copied_rather_than_filtered() {
    let native = (40, 30);
    let raster = (20, 30);
    let pixels: Vec<u8> = (0..native.1)
        .flat_map(|y| {
            let level = (y * 5) as u8;
            (0..native.0).flat_map(move |_| [level, level, level, 255])
        })
        .collect();
    let mut identity = target(native, raster).expect("a reducible extent");
    for row in pixels.chunks_exact(native.0 as usize * 4) {
        identity.push_row(row);
    }
    let got = identity.into_pixels().expect("a whole target");
    assert_eq!(
        got,
        lanczos_resample(&pixels, native, raster),
        "the definition copies an axis with nothing to reduce"
    );
    for (y, row) in got.chunks_exact(raster.0 as usize * 4).enumerate() {
        assert!(
            row.chunks_exact(4)
                .all(|texel| texel == [y as u8 * 5, y as u8 * 5, y as u8 * 5, 255]),
            "row {y} of a source whose rows are flat colours must still be that colour"
        );
    }

    // The control, and it has to be a different source: flat rows a hair apart
    // are what a near-identity kernel leaves alone even when it filters them,
    // so they cannot tell the copy from the filter. These rows alternate, and
    // the same target with one row fewer on the vertical axis comes back mixed
    // — which is the branch above being a branch rather than a coincidence.
    let striped: Vec<u8> = (0..native.1)
        .flat_map(|y| {
            let level = if y % 2 == 0 { 0_u8 } else { 255 };
            (0..native.0).flat_map(move |_| [level, level, level, 255])
        })
        .collect();
    let mut reduced = target(native, (native.0, raster.1 - 1)).expect("a reducible extent");
    for row in striped.chunks_exact(native.0 as usize * 4) {
        reduced.push_row(row);
    }
    let filtered = reduced.into_pixels().expect("a whole target");
    let copied: Vec<u8> = striped
        .chunks_exact(native.0 as usize * 4)
        .take(raster.1 as usize - 1)
        .flatten()
        .copied()
        .collect();
    assert_ne!(
        filtered, copied,
        "the control: a reduced axis is filtered, and this source shows it"
    );
}

/// One raster row of a 64-wide, 4-tall step edge reduced by half, with the
/// edge itself off the reduction's own boundary so a box filter has to average
/// the two levels rather than land between them.
fn reduced_step(dark: u8, bright: u8) -> Vec<u8> {
    let native = (64, 4);
    let pixels: Vec<u8> = (0..native.1)
        .flat_map(|_| {
            (0..native.0).flat_map(move |x| {
                let level = if x < 33 { dark } else { bright };
                [level, level, level, 255]
            })
        })
        .collect();
    let mut target = target(native, (32, 4)).expect("a reducible extent");
    for row in pixels.chunks_exact(native.0 as usize * 4) {
        target.push_row(row);
    }
    let got = target.into_pixels().expect("a whole target");
    assert_eq!(
        got,
        lanczos_resample(&pixels, native, (32, 4)),
        "the streaming target and the definition agree on an edge too"
    );
    got[..32 * 4]
        .chunks_exact(4)
        .map(|texel| texel[0])
        .collect()
}

/// A reduced step edge is what the filter is for, and the price it charges.
///
/// A box filter can only average the step's two levels, so it never leaves the
/// range they span: a thin bright line on a dark ground comes out dimmer than
/// it is, by roughly its own coverage. Lanczos3's negative lobes push the bright
/// side past the step and the dark side below it, which is where the contrast
/// comes back from — and it is also why `scale_level` clamps at all, since
/// there is no 8-bit sample below zero or above 255.
///
/// Both halves are checked. With the dark level above the floor both excursions
/// are visible — measured, thirteen levels either side of a step of a hundred
/// and sixty, so the bound asserted here is under twice what the kernel does —
/// and the row's mean is the source's exactly, which is the filter preserving
/// its input's total ink. With the dark level *at* the floor the undershoot has
/// nowhere to go, so the bottom of the range is what the dark side holds and
/// the row's mean is still within a couple of levels of the source's, which is
/// what says the clamp is not hiding an unbounded error. A normalization bug
/// large enough to matter fails these bounds instead of being quietly clipped.
#[test]
fn a_reduced_step_edge_keeps_its_contrast_within_a_bounded_overshoot() {
    let mean = |levels: &[u8]| levels.iter().map(|l| u32::from(*l)).sum::<u32>() as f64 / 32.0;
    let source_mean =
        |dark: u8, bright: u8| (33.0 * f64::from(dark) + 31.0 * f64::from(bright)) / 64.0;

    let (dark, bright) = (40_u8, 200_u8);
    let levels = reduced_step(dark, bright);
    let highest = *levels.iter().max().expect("a raster row has pixels");
    let lowest = *levels.iter().min().expect("a raster row has pixels");
    assert!(
        highest > bright && lowest < dark,
        "a reduced edge rings past both of its levels: {levels:?}"
    );
    assert!(
        i16::from(highest) - i16::from(bright) <= 24,
        "the overshoot cannot be more than a windowed sinc's: {levels:?}"
    );
    assert!(
        i16::from(dark) - i16::from(lowest) <= 24,
        "and the undershoot is its mirror image: {levels:?}"
    );
    assert!(
        (mean(&levels) - source_mean(dark, bright)).abs() < 2.0,
        "the filter must keep the row's total ink: got {:?}, expected {}",
        mean(&levels),
        source_mean(dark, bright)
    );

    let levels = reduced_step(0, bright);
    let highest = *levels.iter().max().expect("a raster row has pixels");
    assert!(
        levels.contains(&0),
        "an undershoot below the floor has to clamp to it: {levels:?}"
    );
    assert!(
        highest > bright && i16::from(highest) - i16::from(bright) <= 24,
        "clamping the dark side must not cost the bright side its overshoot: {levels:?}"
    );
    assert!(
        (mean(&levels) - source_mean(0, bright)).abs() < 2.0,
        "clipping the undershoot must not move the row's mean far: got {:?}, expected {}",
        mean(&levels),
        source_mean(0, bright)
    );
}

/// An axis with more outputs than inputs would have to invent samples, so it is
/// declined rather than approximated: the source that asked for one takes the
/// whole-image path, where the interpolating filter lives.
#[test]
fn a_target_larger_than_its_source_is_declined() {
    assert!(target((100, 100), (101, 100)).is_none(), "a wider raster");
    assert!(target((100, 100), (100, 101)).is_none(), "a taller raster");
    assert!(target((100, 100), (200, 50)).is_none(), "wider on one axis");
    assert!(target((100, 100), (100, 100)).is_some(), "the same size");
    assert!(target((100, 100), (50, 50)).is_some(), "smaller");
    assert!(target((0, 100), (0, 100)).is_none(), "an empty source");
}

/// The number of taps one output takes is what [`max_run`] promises, whatever
/// the ratio, so the table's size is a property of the source rather than of
/// where an output's centre happens to fall. The bound is what admits a
/// reduction at all, so an undercount here would be a table the axis wrote past
/// rather than a picture that came out wrong.
#[test]
fn the_weight_table_is_the_size_the_bound_promised() {
    for input in [
        1_u32, 2, 3, 7, 13, 64, 97, 100, 251, 1000, 4096, 5000, 12000,
    ] {
        for output in [1_u32, 2, 3, 7, 13, 32, 64, 97, 100, 251, 1000, 4096] {
            if output > input || input >= MAX_AXIS {
                continue;
            }
            let bound = max_run(input, output);
            let axis = Axis::new(input, output).unwrap_or_else(|| {
                panic!("{input} -> {output}: a table inside the limit ({bound})")
            });
            for run in axis.runs.iter() {
                assert!(
                    u64::from(run.len) <= bound,
                    "{input} -> {output}: a run of {} taps exceeds the {bound} the \
                     bound promised",
                    run.len
                );
            }
            assert!(
                axis.weights.len() as u64 <= bound * u64::from(output),
                "{input} -> {output}: the table is longer than the bound promised"
            );
            // Running the bound over the ratios an axis is built for is only
            // half the claim; the other half is that the table itself is the
            // runs end to end and nothing else.
            assert_eq!(
                axis.runs.iter().map(|run| u64::from(run.len)).sum::<u64>(),
                axis.weights.len() as u64,
                "{input} -> {output}: the runs are the table"
            );
        }
    }
}

/// A source long enough that the weights its reduction asks for would not be
/// bounded is declined for the same reason a magnified one is: the source takes
/// the whole-image path. The two sizes here sit either side of the table's
/// limit, so this is a bound rather than a size that happens to be large.
///
/// The table is proportional to the source's *width*, which is the one term on
/// this path the raster does not bound — a wider kernel costs proportionally
/// more of it, and this is where that shows up as a refusal rather than as an
/// allocation.
#[test]
fn a_reduction_too_deep_for_its_weight_table_is_declined() {
    let at = |width| {
        RasterTarget::new(
            ImageNativeExtent::new(width, 1),
            ImageRasterExtent::new(4096, 1),
        )
    };
    assert!(at(340_000).is_some(), "a table inside the limit is built");
    assert!(at(360_000).is_none(), "a table past it is declined");
}

/// The rows a band hands over are the rows built since the caller's mark:
/// in order, contiguous, and never the same row twice.
#[test]
fn each_band_takes_the_rows_built_since_the_last_one() {
    let native = (60, 40);
    let raster = (20, 13);
    let pixels = source(native.0, native.1);
    let mut target = target(native, raster).expect("a reducible extent");
    let mut written = 0;
    let mut bands = 0;
    // Feed in bands of three source rows, the way a decode does. A band of
    // three builds nothing at first: an output row is opened by the first row
    // its support reaches and there are more than three of those.
    for rows in pixels.chunks(native.0 as usize * 4 * 3) {
        for row in rows.chunks_exact(native.0 as usize * 4) {
            target.push_row(row);
        }
        if let Some(band) = target.band_since(written) {
            let placement = band.placement();
            assert_eq!(placement.raster(), target.raster());
            assert_eq!(
                placement.rows().start(),
                written,
                "a band starts where the last one ended"
            );
            assert_eq!(
                band.pixels().len(),
                raster.0 as usize * placement.rows().len().get() as usize * 4,
                "a band carries exactly the rows it fills"
            );
            written = placement.rows().end();
            bands += 1;
        }
    }
    assert_eq!(written, raster.1, "the bands cover the whole raster");
    assert!(bands > 1, "a 40-row source bands into more than one piece");
    assert_eq!(target.built(), raster.1);
    assert!(
        target.band_since(written).is_none(),
        "a band with no rows is not a band"
    );
}

/// A target nobody has finished feeding is not an image: the only way to get
/// the pixels out is to have pushed every row.
#[test]
fn a_partially_fed_target_is_not_a_finished_image() {
    let native = (16, 8);
    let pixels = source(native.0, native.1);
    let mut half = target(native, (8, 4)).expect("a reducible extent");
    for row in pixels.chunks_exact(native.0 as usize * 4).take(4) {
        half.push_row(row);
    }
    assert!(
        half.into_pixels().is_none(),
        "half a source is not an image"
    );

    let mut whole = target(native, (8, 4)).expect("a reducible extent");
    for row in pixels.chunks_exact(native.0 as usize * 4) {
        whole.push_row(row);
    }
    assert!(whole.into_pixels().is_some(), "a whole source is");
}
