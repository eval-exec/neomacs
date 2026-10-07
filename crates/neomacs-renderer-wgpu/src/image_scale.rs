//! Streaming Lanczos3 resampling: source rows in, raster rows out.
//!
//! A banded decode used to build the whole native image and only then scale it
//! onto the texture's raster. Four buffers that size coexisted while it did —
//! the encoded bytes, the decoded image, the conversion to RGBA and the resized
//! output — which for a 12000x700 PNG is about 50 MB of memory to look at one
//! 4096x238 texture. [`RasterTarget`] is the replacement: the decode pushes its
//! rows in as it reads them, each row is resampled as it arrives, and what the
//! target holds is the raster itself plus the few output rows still being
//! filtered. No buffer is ever sized by the native height.
//!
//! **The filter is Lanczos3**, the windowed sinc in [`kernel`]: the filter the
//! whole-image path has always resampled with, and the one this path used
//! before it streamed. It is *not* here because it costs nothing. A windowed
//! sinc keeps a thin line's contrast where a box filter averages it away, and
//! it pays for that with ringing — the overshoot [`scale_level`] has to clamp —
//! and with taps: a three-fold reduction takes eighteen weights per output
//! where the area average took three, and reads about a third again as long.
//! What it does not cost is a native-size buffer, which is what the streaming
//! target is for: the filter was never why that buffer existed, so putting the
//! filter back does not bring the buffer back. That claim is measured in the
//! commit that put this filter back, rather than assumed here.
//!
//! **A target only ever reduces.** A filter that had to *invent* samples would
//! be reading a value at a position no sample sits at, and the honest answer
//! there is an interpolating filter rather than a reconstruction one: a source
//! shown larger than it is declines the target ([`RasterTarget::new`] returns
//! `None`) and takes the whole-image path, which is where the interpolating
//! filter lives and which is what such a source did before banding existed.
//! Every axis this type admits has at least as many inputs as outputs.
//!
//! **The weights are fixed point.** Every weight is an integer in units of
//! [`WEIGHT_ONE`], one output's run of them summing to within a level of
//! `WEIGHT_ONE` after normalization, and each output is divided by *its own*
//! run's sum rather than by `WEIGHT_ONE`. Two things follow. The division is a
//! deterministic function of the input bytes with no floating-point
//! accumulation on the streaming path — which is what lets the tests compare it
//! to the definition bit for bit rather than within a tolerance. And a constant
//! source still reduces to that constant exactly, whichever taps the support
//! happens to land on, which is the property that makes the edge handling below
//! honest rather than a fudge.
//!
//! **The kernel is sampled with its support widened by the reduction.**
//! Output `o`'s window is three output texels either side of its centre, which
//! is `3 * n / m` input texels either side — five taps at scale one, and
//! proportionally more when the source is minified, so that a source reduced
//! three-fold is band-limited rather than sampled three-fold. A kernel
//! sampled at its own three-texel window instead is not a downscaler at all:
//! on a three-fold reduction of a one-texel grating every tap lands on a dark
//! texel and the output is uniformly black where the area average is uniformly
//! 85. Taps that would fall outside the source are dropped rather than folded
//! back in, so an edge is an edge; dropping them changes the run's total, and
//! normalizing by the run's own total is what keeps a constant source constant
//! across it.
//!
//! The result is rounded and then clamped to `[0, 255]`, **once**: the first
//! axis hands the second a mean rather than a level, so the clamp is where a
//! value becomes an 8-bit sample and not once per axis. The clamp is required
//! rather than defensive — a non-negative kernel can only produce a convex
//! combination of its inputs, but the sinc's negative lobes can push a channel
//! past either end of the source's own range at a sharp edge, and there is no
//! 8-bit value there to hold — but it is also the one place on this path where
//! a value is thrown away, and clamping between the axes throws away an
//! excursion the second axis was going to cancel. On the 12000x700 text
//! fixture, clamping per axis instead of once puts 1.4% of the texture's pixels
//! outside a level of the whole-image path's answer, by as much as 20 levels,
//! every one of them at a glyph edge; clamping once puts none of them there and
//! leaves a difference of at most one level anywhere. `image_scale/tests.rs`
//! pins the bound the single clamp can hide, so a normalization bug large
//! enough to matter shows up as a failed bound rather than as a silently
//! clipped pixel.
//!
//! **An axis that is not reduced is the identity, and is copied rather than
//! filtered.** This is the one place the target is deliberately not the kernel.
//! Lanczos3's zeros happen to make the filtered answer the same bytes here —
//! its taps at scale one sit at whole-texel offsets, where `sinc` is zero — but
//! that is a coincidence of where the samples land, not a property the type
//! promises, and the copy is what makes "an axis with nothing to reduce is not
//! blurred at all" true by construction instead of by rounding. It is also the
//! case an image shown at its own size takes, which is what makes the preview
//! during a decode the finished pixels rather than an approximation of them.
//! The consequence to know: an axis goes from an exact copy at `n` outputs to a
//! filtered one at `n - 1`, so a source reduced by a hair is filtered by the
//! full kernel and a source not reduced at all is not filtered at all.

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::Arc;

use neomacs_display_protocol::{ImageNativeExtent, ImageRasterExtent};

use crate::image_bands::{BandPlacement, RasterBand, TextureRows};

/// Largest dimension either axis of a resample is defined for.
///
/// A PNG's own header caps a dimension at `2^31 - 1`, so anything at or beyond
/// that is declined: a dimension no decoder can produce should not be able to
/// reach the arithmetic below, and the boundary arithmetic is `i128` precisely
/// so that this limit is about what a decoder can say rather than about what
/// the sums can hold.
const MAX_AXIS: u32 = 1 << 31;

/// Largest number of taps one output's filter may cover.
///
/// A run is the kernel's window widened by the reduction and clamped to the
/// source, so this is either a reduction deeper than about 11000:1 on one axis
/// or a source wider than 65536 texels reduced to a single output. No picture
/// is shown at either, and a source asking for one declines the target and
/// takes the whole-image path — the same answer it got before banding existed.
/// What the cap buys is a bound on the run a single row's arithmetic walks,
/// which is the shape [`MAX_WEIGHT_SUM`] asserts something about.
const MAX_TAPS: u64 = 1 << 16;

/// Largest weight table one axis may hold, in entries.
///
/// The table is `output * taps`, which is at most `6 * input + 2 * output`, so
/// an axis holding a weight for every tap it could take is bounded by the
/// source's own length — and a source long enough to approach this (a third of
/// a million texels on one axis) is one the whole-image path could not
/// represent either. Bounding it here is what makes [`Axis::build`]'s `u32`
/// offsets and the table's own allocation safe to reason about rather than
/// merely small in practice.
///
/// The native-extent term is worth naming, because it is the one this filter
/// scales where the area average did not: the table is proportional to the
/// source's width rather than to the raster's, so it is the *only* thing on
/// this path that a bigger source grows without the raster growing with it.
/// At the eighteen weights a three-fold reduction asks for it is 297 KB on the
/// 12000x700 fixture and 726 KB on thirty megapixels, against the 25 MB and
/// 78 MB the decode itself peaks at — under 1%, but a real term, and a source
/// wide enough to reach this ceiling declines the target rather than
/// allocating for it.
const MAX_WEIGHTS: u64 = 1 << 21;

/// One unit of weight, as an integer.
///
/// A weight is stored as the fraction of an output sample's total that one tap
/// carries, in `1 / WEIGHT_ONE`ths. At `2^16` a single tap's quantization is
/// worth at most `255 / 2^17` of a level, and a run of them — their errors are
/// not correlated, but even summed they would be — stays under a level, which
/// is why the streaming result can be compared to the definition exactly. Big
/// enough to be exact, small enough that `i32` holds a weight and `i64` holds
/// what a whole row accumulates.
const WEIGHT_ONE: i64 = 1 << 16;

/// A bound the sum of one run's weight magnitudes is asserted against.
///
/// The magnitudes of a normalized run sum to the kernel's own `L1` norm, which
/// for Lanczos3 is about `1.2` whatever the reduction — not to the tap count,
/// because the weights of a deeper reduction are individually smaller. This
/// bound is `16 * WEIGHT_ONE`, an order of magnitude above that, so it is a
/// statement that a kernel whose lobes grew would fail loudly instead of
/// wrapping rather than a budget in the way of a picture.
const MAX_WEIGHT_SUM: i64 = 1 << 20;

/// `sin(pi * t) / (pi * t)`, with the removable singularity at zero taken.
fn sinc(t: f64) -> f64 {
    if t == 0.0 {
        1.0
    } else {
        let a = t * std::f64::consts::PI;
        a.sin() / a
    }
}

/// Lanczos's windowed sinc with a window of three, at distance `x`:
///
/// ```text
/// W(x) = sinc(x) * sinc(x / 3)      |x| < 3
///        0                          |x| >= 3
/// ```
///
/// The window is what makes this a filter rather than an interpolation: a sinc
/// alone decays too slowly to sample. Three is the usual choice, and it is the
/// filter the whole-image path resamples with, so both paths to a texture agree
/// about what a reduced image looks like.
///
/// The distance is in units the caller has already divided by the reduction, so
/// the window is three *output* texels widened to `3 * n / m` input ones. The
/// zeros at the non-zero whole numbers are the reason an axis with nothing to
/// reduce would come out unchanged if it were filtered at all; see the module
/// docs for why it is copied anyway.
fn kernel(x: f64) -> f64 {
    const WINDOW: f64 = 3.0;
    if x.abs() >= WINDOW {
        return 0.0;
    }
    sinc(x) * sinc(x / WINDOW)
}

/// The run of input texels one output's filter covers, and what it holds.
#[derive(Clone, Copy, Debug)]
struct Run {
    /// First input texel of the run.
    first: u32,
    /// Number of texels in it.
    len: u32,
    /// Where the run starts in [`Axis::weights`].
    at: u32,
    /// The sum of the run's weights.
    total: i64,
}

/// One axis of a resample: `output` samples, each a weighted mean of `input`.
///
/// `output <= input` is what makes the mean a mean of samples that exist; a
/// longer axis has no definition here, and `None` is how that is said. The
/// weights are computed once, when the axis is built, and read once per source
/// row after that: they are a function of the ratio alone, and a decode asks
/// for the same ratio on every row.
#[derive(Clone, Debug)]
struct Axis {
    input: u32,
    output: u32,
    /// One entry per output, in increasing order of `first` and of its end.
    runs: Box<[Run]>,
    /// Every run's weights, one after another.
    weights: Box<[i32]>,
}

/// The most taps the widest output's filter can cover, for `n` inputs and `m`
/// outputs.
///
/// The taps of one output are the input texels whose centres lie strictly
/// inside a window `6n/m` wide. Texel centres are one apart, so an open window
/// that wide holds at most `floor(6n/m) + 1` of them, and this is that with a
/// row of margin against the boundary arithmetic. No output covers more texels
/// than the source has, which is the floor taken here: a source reduced to a
/// single output column takes every texel it has, and that is a long run rather
/// than a deep window.
///
/// [`Axis::new`] decides whether a reduction is one this type can hold from
/// this, and `image_scale/tests.rs` checks it against every run the axes
/// actually build, so it is a bound rather than a guess.
fn max_run(input: u32, output: u32) -> u64 {
    (6 * u64::from(input) / u64::from(output) + 2).min(u64::from(input))
}

impl Axis {
    fn new(input: u32, output: u32) -> Option<Self> {
        if input == 0 || output == 0 || output > input || input >= MAX_AXIS {
            return None;
        }
        // Taps, and the table that holds one run of them per output. Both are
        // properties of the ratio alone, so both are decided here rather than
        // discovered once the table has been built.
        let taps = max_run(input, output);
        let entries = taps * u64::from(output);
        (taps <= MAX_TAPS && entries <= MAX_WEIGHTS).then(|| Self::build(input, output))
    }

    /// Whether every output is one input, which the target copies rather than
    /// filters.
    ///
    /// Lanczos3 would land on the same bytes here — its taps at scale one are
    /// at whole-texel offsets, where two of the three sinc factors are zero —
    /// but only as long as the zeros survive quantization, and this is the one
    /// axis whose output has to be the input rather than close to it. The copy
    /// is what this axis means.
    fn is_identity(&self) -> bool {
        self.input == self.output
    }

    /// The weights one output distributes over the inputs it covers.
    fn build(input: u32, output: u32) -> Self {
        let scale = f64::from(input) / f64::from(output);
        let mut runs = Vec::with_capacity(output as usize);
        let mut weights: Vec<i32> = Vec::new();
        for o in 0..output {
            let (low, high) = window(input, output, o);
            let first = u32::try_from(low.max(0)).expect("the low tap was clamped into the source");
            let last = u32::try_from(high.min(i64::from(input) - 1))
                .expect("the high tap was clamped into the source");
            assert!(first <= last, "an output covers at least one input");
            // The centre of output `o` in input coordinates, where input texel
            // `i` spans `[i, i + 1)` and is read at its centre `i + 0.5`.
            let centre = (f64::from(o) + 0.5) * scale;
            let distance = |i: u32| ((f64::from(i) + 0.5) - centre) / scale;
            let sum: f64 = (first..=last).map(|i| kernel(distance(i))).sum();
            assert!(sum > 0.0, "the kernel is positive somewhere in its window");
            let at = u32::try_from(weights.len()).expect("the table fits a u32");
            let mut total = 0_i64;
            let mut magnitudes = 0_i64;
            for i in first..=last {
                let weight = (kernel(distance(i)) * WEIGHT_ONE as f64 / sum).round() as i32;
                weights.push(weight);
                total += i64::from(weight);
                magnitudes += i64::from(weight).abs();
            }
            assert!(
                magnitudes <= MAX_WEIGHT_SUM,
                "a run's weights stay inside the budget the sums rely on"
            );
            assert!(total != 0, "a run's weights cannot cancel to nothing");
            runs.push(Run {
                first,
                len: last - first + 1,
                at,
                total,
            });
        }
        assert!(
            weights.len() as u64 <= MAX_WEIGHTS,
            "the table is the size the bound promised"
        );
        Self {
            input,
            output,
            runs: runs.into_boxed_slice(),
            weights: weights.into_boxed_slice(),
        }
    }

    /// The inputs one output covers, and the weight of each tap.
    fn run(&self, output: u32) -> (Run, &[i32]) {
        let run = self.runs[output as usize];
        let at = run.at as usize;
        (run, &self.weights[at..at + run.len as usize])
    }

    /// Every output `input` contributes to, in increasing order, with the
    /// weight of that contribution.
    ///
    /// A run's `first` is non-decreasing across outputs, and so is its end, so
    /// the runs that contain `input` are exactly a slice of them: the ones that
    /// have not ended yet begin at the first run whose end is past `input`, and
    /// the ones that have begun end at the first run whose start is past it.
    /// Two binary searches rather than a scan, because this is called once per
    /// source row and a scan would make the whole decode quadratic in the
    /// raster's height. A row whose outputs are all ahead of it or behind it
    /// yields nothing, which is how the last source row can still be the one
    /// that finishes the last output.
    fn contributions(&self, input: u32) -> impl Iterator<Item = (u32, i64)> + '_ {
        let begun = self
            .runs
            .partition_point(|run| run.first + run.len <= input) as u32;
        let open = self.runs.partition_point(|run| run.first <= input) as u32;
        (begun..open).map(move |output| {
            let (run, weights) = self.run(output);
            (output, i64::from(weights[(input - run.first) as usize]))
        })
    }

    /// The last input that contributes anything to output `o`.
    fn last_input(&self, o: u32) -> u32 {
        let run = self.runs[o as usize];
        run.first + run.len - 1
    }
}

/// The run of input texels output `o`'s filter covers, before it is clamped to
/// the source.
///
/// Output `o` is centred at `c = (o + 0.5) * n / m` in input coordinates, where
/// input texel `i` spans `[i, i + 1)`. The kernel's support is three *output*
/// texels — widened by the reduction, so it is `3n/m` input texels either side,
/// which is what makes this a band-limiting filter rather than a resampling one
/// that aliases. The taps are therefore the texels whose centre lies in
/// `(c - 3n/m, c + 3n/m)` — the support is open, because the windowed sinc is
/// zero at its edge — and multiplying that through by `2m` gives the pair of
/// integer inequalities solved here exactly:
///
/// ```text
///     n * (2o - 5)  <  m * (2i + 1)  <  n * (2o + 7)
/// ```
///
/// `i128` because `n` reaches `2^31` and `2o` does too; the two divisions are
/// the only ones on this path and they happen once per output, when the axis is
/// built.
fn window(n: u32, m: u32, o: u32) -> (i64, i64) {
    let n = i128::from(n);
    let m = i128::from(m);
    let o = i128::from(o);
    let low = n * (2 * o - 5);
    let high = n * (2 * o + 7);
    // `2i + 1 > low / m` for integral `i` is `i >= ceil(floor(low / m) / 2)`,
    // and `2i + 1 < high / m` is `i <= (ceil(high / m) - 2) / 2` rounded down.
    // Rounding towards positive infinity is spelled out because `div_ceil` on
    // the signed types is still unstable, and `m > 0`, so `+ m - 1` is it.
    let first = (low.div_euclid(m) + 1).div_euclid(2);
    let last = ((high + m - 1).div_euclid(m) - 2).div_euclid(2);
    (
        i64::try_from(first).expect("a tap index fits an i64"),
        i64::try_from(last).expect("a tap index fits an i64"),
    )
}

/// One output row's running weighted total, in flight.
///
/// The sums are `i64` and the weights are not: a run's weight magnitudes sum to
/// at most [`MAX_WEIGHT_SUM`] and a sample is at most 255, so the largest
/// magnitude an output's sum can hold is `255 * MAX_WEIGHT_SUM`, which is three
/// orders of magnitude inside `i64` — and inside nothing smaller, since
/// negative lobes make the partial sums sign-agnostic.
struct Accum {
    output: u32,
    sums: Vec<i64>,
}

impl Accum {
    fn new(output: u32, sums: Vec<i64>) -> Self {
        Self { output, sums }
    }

    /// Add one input row, `weight` times over.
    fn add(&mut self, row: &[i32], weight: i64) {
        for (sum, sample) in self.sums.iter_mut().zip(row) {
            *sum += weight * i64::from(*sample);
        }
    }
}

/// A raster-sized buffer that source rows are written into as they arrive.
///
/// Built for one native extent and one raster, it is filled from the top: each
/// [`Self::push_row`] takes one source row, and the output rows that row
/// completes are written where they belong. [`Self::band_since`] hands back
/// the rows built since a mark, in order and without gaps, which is exactly
/// what one band of the decode is allowed to fill.
pub(crate) struct RasterTarget {
    native: ImageNativeExtent,
    raster: ImageRasterExtent,
    columns: Axis,
    rows: Axis,
    /// The raster, row-major from row zero.
    pixels: Vec<u8>,
    /// Source rows pushed so far.
    fed: u32,
    /// Output rows written so far.
    built: u32,
    /// Output rows still being filtered, lowest first.
    ///
    /// A few, not a raster's worth: an output row is opened by the first source
    /// row its support reaches and closed by the last, and both of those move
    /// one raster row per `n / m` source rows, so the open set stays around
    /// seven rows however deep the reduction is — one more than the taps either
    /// side of the centre, because the first and last of a run overlap.
    pending: VecDeque<Accum>,
    /// Accumulator buffers to reuse, so a long raster does not allocate one
    /// per output row.
    spare: Vec<Vec<i64>>,
    /// One source row, resampled to the raster's width — and *not* yet a row
    /// of 8-bit samples.
    ///
    /// `i32` rather than `u8`, because a kernel with negative lobes pushes the
    /// resampled row outside the range its own samples span and the second axis
    /// has to see that excursion to cancel it. A row stored as levels is
    /// clamped once per axis; the finished sample is clamped once, at the end.
    /// The difference is real rather than theoretical — on a reduced text
    /// fixture the two agree on 99.2% of pixels and differ by up to 20 levels
    /// on the rest, all of them at the glyph edges the clamp cut into. The
    /// bound on what this holds is `255 * MAX_WEIGHT_SUM`, three orders inside
    /// `i32`, and [`mean`] asserts it rather than assuming it.
    line: Vec<i32>,
}

impl RasterTarget {
    /// A target realizing `native` onto `raster`.
    ///
    /// `None` for an empty extent on either side, which is a source or a raster
    /// with no pixels rather than a target that cannot be built, and for a
    /// raster larger than the source on either axis — a target that would have
    /// to invent pixels is not one this filter can be.
    pub(crate) fn new(native: ImageNativeExtent, raster: ImageRasterExtent) -> Option<Self> {
        let columns = Axis::new(native.width(), raster.width())?;
        let rows = Axis::new(native.height(), raster.height())?;
        Some(Self {
            native,
            raster,
            columns,
            rows,
            pixels: vec![0; raster.width() as usize * raster.height() as usize * 4],
            fed: 0,
            built: 0,
            pending: VecDeque::new(),
            spare: Vec::new(),
            line: vec![0; raster.width() as usize * 4],
        })
    }

    /// The raster this target fills.
    pub(crate) fn raster(&self) -> ImageRasterExtent {
        self.raster
    }

    /// How many output rows have been written; the fill level of the raster.
    pub(crate) fn built(&self) -> u32 {
        self.built
    }

    /// Take one source row, in order.
    ///
    /// The row must be the native width in RGBA, which is what the decode
    /// expands each row to; anything else is a decode whose rows are not the
    /// extent its header named, and the raster it would produce would not be
    /// the raster its texture holds.
    pub(crate) fn push_row(&mut self, row: &[u8]) {
        assert_eq!(
            row.len(),
            self.native.width() as usize * 4,
            "a source row is the native width in RGBA"
        );
        assert!(
            self.fed < self.native.height(),
            "no more rows than the height"
        );
        match self.columns.is_identity() {
            true => self
                .line
                .iter_mut()
                .zip(row)
                .for_each(|(level, sample)| *level = i32::from(*sample)),
            false => resample_row(&self.columns, row, &mut self.line),
        }
        let index = self.fed;
        self.fed += 1;
        if self.rows.is_identity() {
            let stride = self.raster.width() as usize * 4;
            let at = index as usize * stride;
            for (channel, level) in self.pixels[at..at + stride].iter_mut().zip(&self.line) {
                *channel = scale_level(*level);
            }
            self.built = self.fed;
            return;
        }
        let stride = self.raster.width() as usize * 4;
        let Self {
            rows,
            line,
            pending,
            spare,
            pixels,
            built,
            ..
        } = self;
        for (output, weight) in rows.contributions(index) {
            let slot = match pending.iter().position(|accum| accum.output == output) {
                Some(slot) => slot,
                None => {
                    // Outputs are visited in increasing order, and an output is
                    // opened by the first row its support reaches, which is the
                    // same order — so a new one is always the last.
                    let sums = spare.pop().unwrap_or_else(|| vec![0; line.len()]);
                    pending.push_back(Accum::new(output, sums));
                    pending.len() - 1
                }
            };
            // Indexed rather than `get_mut`: the slot is one this loop just
            // produced, so an invalid one is a bug in this function, and a
            // contribution that vanished silently would be a wrong picture
            // rather than a loud failure. A panic here is caught by the
            // decoder thread and turns into a failed decode.
            pending[slot].add(line, weight);
        }
        // An output row is complete once every input that contributes to it
        // has been pushed, which is a property of the axis, not of the band.
        while let Some(front) = pending.front() {
            if rows.last_input(front.output) > index {
                break;
            }
            let done = pending.pop_front().expect("the front was just checked");
            let at = done.output as usize * stride;
            for (channel, sum) in pixels[at..at + stride].iter_mut().zip(&done.sums) {
                *channel = scale_channel(*sum, rows.runs[done.output as usize].total);
            }
            let mut sums = done.sums;
            sums.iter_mut().for_each(|sum| *sum = 0);
            spare.push(sums);
            *built = done.output + 1;
        }
    }

    /// The output rows built since `from`, with their pixels.
    ///
    /// `None` when no row was built since then, which a decode paused between
    /// two of them reports; the band that follows starts where this one would
    /// have. `from` is a [`Self::built`] the caller read earlier, so the range
    /// is inside the raster by construction.
    pub(crate) fn band_since(&self, from: u32) -> Option<RasterBand> {
        let len = NonZeroU32::new(self.built.checked_sub(from)?)?;
        let rows = TextureRows::new(from, len);
        let stride = self.raster.width() as usize * 4;
        let pixels: Arc<[u8]> =
            self.pixels[from as usize * stride..self.built as usize * stride].into();
        Some(RasterBand::new(
            BandPlacement::new(self.raster, rows),
            pixels,
        ))
    }

    /// The raster, once every source row has been pushed.
    ///
    /// `None` before then, so a caller cannot publish a partial target as a
    /// finished image.
    pub(crate) fn into_pixels(self) -> Option<Vec<u8>> {
        (self.fed == self.native.height() && self.built == self.raster.height())
            .then_some(self.pixels)
    }
}

/// Resample one row of `axis.input` RGBA texels into `axis.output` of them.
///
/// The result is a mean and not yet a sample: an axis that is not the last one
/// hands what it computed to the next, and clamping it to `[0, 255]` here is
/// what would make the two axes clamp twice where the finished image clamps
/// once.
fn resample_row(axis: &Axis, source: &[u8], target: &mut [i32]) {
    for (output, texel) in target.chunks_exact_mut(4).enumerate() {
        let (run, weights) = axis.run(output as u32);
        let at = run.first as usize * 4;
        let mut sums = [0_i64; 4];
        for (tap, weight) in weights.iter().enumerate() {
            let sample = &source[at + tap * 4..at + tap * 4 + 4];
            let weight = i64::from(*weight);
            for (sum, channel) in sums.iter_mut().zip(sample) {
                *sum += weight * i64::from(*channel);
            }
        }
        for (channel, sum) in texel.iter_mut().zip(sums) {
            *channel = mean(sum, run.total);
        }
    }
}

/// One weighted mean, rounded to the nearest whole level.
///
/// Normalized by the run's own total, so a constant source divides back out to
/// itself exactly — the rounding adds `total / 2` before a truncating division,
/// which maps `c * total` to `c` whatever `total` is. Half up rather than half
/// away from zero, because a negative sum is a real value here and one rounding
/// rule for both signs is one rule to reason about.
///
/// The mean is not clamped, and the assertion is what says the type is wide
/// enough for the values the kernel can produce: a run's weight magnitudes sum
/// to at most [`MAX_WEIGHT_SUM`] and a sample to at most 255, so the quotient
/// is at most `255 * MAX_WEIGHT_SUM` — and it is a magnitude and not a range,
/// since negative lobes put it on both sides of zero.
fn mean(sum: i64, total: i64) -> i32 {
    i32::try_from((sum + total / 2).div_euclid(total))
        .expect("a mean inside the bound the weights put on it")
}

/// One 8-bit sample: the level a mean is, clamped to what a byte can hold.
///
/// The clamp is not defensive. The sinc's negative lobes put an edge's value
/// outside the range its own inputs span, and there is no 8-bit sample there;
/// this is the single place a value becomes one. The bound the clamp can hide
/// is pinned by a test rather than left implicit.
fn scale_level(level: i32) -> u8 {
    u8::try_from(level.clamp(0, 255)).expect("a clamped level is a level")
}

/// One weighted mean, rounded and clamped to the nearest 8-bit sample.
fn scale_channel(sum: i64, total: i64) -> u8 {
    scale_level(mean(sum, total))
}

#[cfg(test)]
#[path = "image_scale/tests/image_scale_test.rs"]
mod tests;
