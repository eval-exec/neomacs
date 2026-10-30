//! Linear-space region diff, adapted from GNU gnulib's lib/diffseq.h.
//!
//! Copyright (C) 1988-1989, 1992-1995, 2001-2004, 2006-2026 Free Software
//! Foundation, Inc. GNU GPL version 3 or later; see the repository COPYING.
//! Reference: https://github.com/coreutils/gnulib/blob/master/lib/diffseq.h
//!
//! Keep GNU's diagonal traversal, midpoint ties, long-snake heuristic and
//! too_expensive partition policy: equal-cost scripts can move markers and
//! record undo differently. Two reusable frontiers and two change bitsets
//! replace the full O(D * (N + M)) backtracking trace.

use super::ReplaceRegionChangeRun;
use std::time::Instant;

pub(super) const DEFAULT_MAX_COSTS: usize = 1_000_000;
const SNAKE_LIMIT: isize = 20;

fn filled<T: Clone>(len: usize, value: T) -> Option<Vec<T>> {
    let mut result = Vec::new();
    result.try_reserve_exact(len).ok()?;
    result.resize(len, value);
    Some(result)
}

struct Changes(Vec<u8>);
impl Changes {
    fn new(len: usize) -> Option<Self> {
        Some(Self(filled(len / 8 + 1, 0)?))
    }
    fn set(&mut self, pos: isize) {
        let pos = pos as usize;
        self.0[pos / 8] |= 1 << (pos % 8);
    }
    fn get(&self, pos: usize) -> bool {
        self.0[pos / 8] & (1 << (pos % 8)) != 0
    }
}

#[derive(Clone, Copy)]
struct Region {
    xoff: isize,
    xlim: isize,
    yoff: isize,
    ylim: isize,
    minimal: bool,
}

struct Partition {
    x: isize,
    y: isize,
    lo_minimal: bool,
    hi_minimal: bool,
}

struct Diff<'a> {
    a: &'a [u32],
    b: &'a [u32],
    forward: Vec<isize>,
    backward: Vec<isize>,
    shift: isize,
    deletions: Changes,
    insertions: Changes,
    max_costs: usize,
    deadline: Option<Instant>,
    work: usize,
}

impl Diff<'_> {
    fn alive(&self) -> Option<()> {
        if self.deadline.is_some_and(|limit| Instant::now() >= limit) {
            None
        } else {
            Some(())
        }
    }
    fn tick(&mut self) -> Option<()> {
        self.work = self.work.wrapping_add(1);
        if self.work % 1024 == 0 {
            self.alive()
        } else {
            Some(())
        }
    }
    fn index(&self, diagonal: isize) -> usize {
        (diagonal + self.shift) as usize
    }
    fn equal(&self, x: isize, y: isize) -> bool {
        self.a[x as usize] == self.b[y as usize]
    }

    fn midpoint(&mut self, r: Region) -> Option<Partition> {
        let Region {
            xoff,
            xlim,
            yoff,
            ylim,
            minimal,
        } = r;
        let dmin = xoff - ylim;
        let dmax = xlim - yoff;
        let fmid = xoff - yoff;
        let bmid = xlim - ylim;
        let (mut fmin, mut fmax, mut bmin, mut bmax) = (fmid, fmid, bmid, bmid);
        let odd = (fmid - bmid) & 1 != 0;
        let fi = self.index(fmid);
        let bi = self.index(bmid);
        self.forward[fi] = xoff;
        self.backward[bi] = xlim;
        for cost in 1usize.. {
            self.alive()?;
            let mut big_snake = false;
            if fmin > dmin {
                fmin -= 1;
                let i = self.index(fmin - 1);
                self.forward[i] = -1;
            } else {
                fmin += 1;
            }
            if fmax < dmax {
                fmax += 1;
                let i = self.index(fmax + 1);
                self.forward[i] = -1;
            } else {
                fmax -= 1;
            }
            let mut d = fmax;
            while d >= fmin {
                self.tick()?;
                let i = self.index(d);
                let lo = self.forward[i - 1];
                let hi = self.forward[i + 1];
                let x0 = if lo < hi { hi } else { lo + 1 };
                let (mut x, mut y) = (x0, x0 - d);
                while x < xlim && y < ylim && self.equal(x, y) {
                    self.tick()?;
                    x += 1;
                    y += 1;
                }
                big_snake |= x - x0 > SNAKE_LIMIT;
                self.forward[i] = x;
                if odd && bmin <= d && d <= bmax && self.backward[i] <= x {
                    return Some(Partition {
                        x,
                        y,
                        lo_minimal: true,
                        hi_minimal: true,
                    });
                }
                d -= 2;
            }
            if bmin > dmin {
                bmin -= 1;
                let i = self.index(bmin - 1);
                self.backward[i] = isize::MAX;
            } else {
                bmin += 1;
            }
            if bmax < dmax {
                bmax += 1;
                let i = self.index(bmax + 1);
                self.backward[i] = isize::MAX;
            } else {
                bmax -= 1;
            }
            let mut d = bmax;
            while d >= bmin {
                self.tick()?;
                let i = self.index(d);
                let lo = self.backward[i - 1];
                let hi = self.backward[i + 1];
                let x0 = if lo < hi { lo } else { hi - 1 };
                let (mut x, mut y) = (x0, x0 - d);
                while xoff < x && yoff < y && self.equal(x - 1, y - 1) {
                    self.tick()?;
                    x -= 1;
                    y -= 1;
                }
                big_snake |= x0 - x > SNAKE_LIMIT;
                self.backward[i] = x;
                if !odd && fmin <= d && d <= fmax && x <= self.forward[i] {
                    return Some(Partition {
                        x,
                        y,
                        lo_minimal: true,
                        hi_minimal: true,
                    });
                }
                d -= 2;
            }
            if minimal {
                continue;
            }

            // GNU's speed-large-files heuristic, enabled by editfns.c.
            if cost > 200 && big_snake {
                let mut best = 0;
                let mut found = None;
                let mut d = fmax;
                while d >= fmin {
                    self.tick()?;
                    let dd = d - fmid;
                    let x = self.forward[self.index(d)];
                    let y = x - d;
                    let progress = (x - xoff) * 2 - dd;
                    if progress > 12 * (cost as isize + dd.abs())
                        && progress > best
                        && xoff + SNAKE_LIMIT <= x
                        && x < xlim
                        && yoff + SNAKE_LIMIT <= y
                        && y < ylim
                        && (1..=SNAKE_LIMIT).all(|k| self.equal(x - k, y - k))
                    {
                        best = progress;
                        found = Some(Partition {
                            x,
                            y,
                            lo_minimal: true,
                            hi_minimal: false,
                        });
                    }
                    d -= 2;
                }
                if found.is_some() {
                    return found;
                }
                let mut d = bmax;
                while d >= bmin {
                    self.tick()?;
                    let dd = d - bmid;
                    let x = self.backward[self.index(d)];
                    let y = x - d;
                    let progress = (xlim - x) * 2 + dd;
                    if progress > 12 * (cost as isize + dd.abs())
                        && progress > best
                        && xoff < x
                        && x <= xlim - SNAKE_LIMIT
                        && yoff < y
                        && y <= ylim - SNAKE_LIMIT
                        && (0..SNAKE_LIMIT).all(|k| self.equal(x + k, y + k))
                    {
                        best = progress;
                        found = Some(Partition {
                            x,
                            y,
                            lo_minimal: false,
                            hi_minimal: true,
                        });
                    }
                    d -= 2;
                }
                if found.is_some() {
                    return found;
                }
            }
            if cost >= self.max_costs {
                let (mut fxy, mut fx) = (-1, 0);
                let mut d = fmax;
                while d >= fmin {
                    self.tick()?;
                    let mut x = self.forward[self.index(d)].min(xlim);
                    let mut y = x - d;
                    if ylim < y {
                        x = ylim + d;
                        y = ylim;
                    }
                    if fxy < x + y {
                        fxy = x + y;
                        fx = x;
                    }
                    d -= 2;
                }
                let (mut bxy, mut bx) = (isize::MAX, 0);
                let mut d = bmax;
                while d >= bmin {
                    self.tick()?;
                    let mut x = self.backward[self.index(d)].max(xoff);
                    let mut y = x - d;
                    if y < yoff {
                        x = yoff + d;
                        y = yoff;
                    }
                    if x + y < bxy {
                        bxy = x + y;
                        bx = x;
                    }
                    d -= 2;
                }
                return Some(if (xlim + ylim) - bxy < fxy - (xoff + yoff) {
                    Partition {
                        x: fx,
                        y: fxy - fx,
                        lo_minimal: true,
                        hi_minimal: false,
                    }
                } else {
                    Partition {
                        x: bx,
                        y: bxy - bx,
                        lo_minimal: false,
                        hi_minimal: true,
                    }
                });
            }
        }
        unreachable!()
    }

    fn compare(&mut self, mut r: Region) -> Option<()> {
        loop {
            while r.xoff < r.xlim && r.yoff < r.ylim && self.equal(r.xoff, r.yoff) {
                self.tick()?;
                r.xoff += 1;
                r.yoff += 1;
            }
            while r.xoff < r.xlim && r.yoff < r.ylim && self.equal(r.xlim - 1, r.ylim - 1) {
                self.tick()?;
                r.xlim -= 1;
                r.ylim -= 1;
            }
            if r.xoff == r.xlim || r.yoff == r.ylim {
                if r.xoff != r.xlim || r.yoff != r.ylim {
                    self.alive()?;
                }
                for x in r.xoff..r.xlim {
                    self.tick()?;
                    self.deletions.set(x);
                }
                for y in r.yoff..r.ylim {
                    self.tick()?;
                    self.insertions.set(y);
                }
                return Some(());
            }
            let part = self.midpoint(r)?;
            let lo = Region {
                xlim: part.x,
                ylim: part.y,
                minimal: part.lo_minimal,
                ..r
            };
            let hi = Region {
                xoff: part.x,
                yoff: part.y,
                minimal: part.hi_minimal,
                ..r
            };
            // Recurse only into the smaller half and iterate the larger one,
            // keeping stack depth logarithmic even with a very low MAX-COSTS.
            if (r.xlim + r.ylim) - (part.x + part.y) < (part.x + part.y) - (r.xoff + r.yoff) {
                self.compare(hi)?;
                r = lo;
            } else {
                self.compare(lo)?;
                r = hi;
            }
        }
    }
}

/// Return the GNU change runs, or `None` when the deadline expires or the
/// O(N+M) comparison workspace cannot be reserved. The caller treats both as
/// "comparison unavailable" and falls back to a whole-region replacement.
pub(super) fn change_runs(
    a: &[u32],
    b: &[u32],
    deadline: Option<Instant>,
    max_costs: usize,
) -> Option<Vec<ReplaceRegionChangeRun>> {
    let size = a.len().checked_add(b.len())?.checked_add(3)?;
    if size > isize::MAX as usize {
        return None;
    }
    let mut diff = Diff {
        a,
        b,
        forward: filled(size, 0)?,
        backward: filled(size, 0)?,
        shift: b.len() as isize + 1,
        deletions: Changes::new(a.len())?,
        insertions: Changes::new(b.len())?,
        max_costs,
        deadline,
        work: 0,
    };
    diff.compare(Region {
        xoff: 0,
        xlim: a.len() as isize,
        yoff: 0,
        ylim: b.len() as isize,
        minimal: false,
    })?;
    let (mut x, mut y) = (a.len(), b.len());
    let mut runs = Vec::new();
    // GNU scans the change bitsets backwards and applies one replace_range
    // per combined deletion/insertion run. The caller wants ascending order.
    // This walk is O(N+M) and GNU performs no early-abort here: once the
    // comparison itself succeeded, a budget expiring during this final scan
    // must still yield the diff (and `t`) rather than a whole-region fallback.
    while x > 0 || y > 0 {
        if (x > 0 && diff.deletions.get(x - 1)) || (y > 0 && diff.insertions.get(y - 1)) {
            let (end_x, end_y) = (x, y);
            while x > 0 && diff.deletions.get(x - 1) {
                x -= 1;
            }
            while y > 0 && diff.insertions.get(y - 1) {
                y -= 1;
            }
            runs.try_reserve(1).ok()?;
            runs.push(ReplaceRegionChangeRun {
                a_start: x,
                a_end: end_x,
                b_start: y,
                b_end: end_y,
            });
        }
        x = x.saturating_sub(1);
        y = y.saturating_sub(1);
    }
    runs.reverse();
    Some(runs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(a: &[u32], b: &[u32], runs: &[ReplaceRegionChangeRun]) -> Vec<u32> {
        let mut actual = a.to_vec();
        for r in runs.iter().rev() {
            actual.splice(r.a_start..r.a_end, b[r.b_start..r.b_end].iter().copied());
        }
        actual
    }

    #[test]
    fn gnu_midpoint_ties_and_cost_partition_are_observable() {
        let a: Vec<_> = "abaacdab".chars().map(u32::from).collect();
        let b: Vec<_> = "badcaba".chars().map(u32::from).collect();
        let positions = |cost| {
            change_runs(&a, &b, None, cost)
                .unwrap()
                .into_iter()
                .map(|r| (r.a_start, r.a_end, r.b_start, r.b_end))
                .collect::<Vec<_>>()
        };
        // GNU diffseq oracle; cost 1 retains a different interior 'a'.
        assert_eq!(
            positions(DEFAULT_MAX_COSTS),
            vec![
                (0, 1, 0, 0),
                (2, 3, 1, 1),
                (4, 5, 2, 2),
                (6, 6, 3, 4),
                (8, 8, 6, 7)
            ]
        );
        assert_eq!(
            positions(1),
            vec![(0, 1, 0, 0), (3, 5, 2, 2), (6, 6, 3, 4), (8, 8, 6, 7)]
        );
    }

    #[test]
    fn partitions_reconstruct_small_inputs_at_all_costs() {
        // Exhaustive small inputs include empty sides, repeated characters,
        // odd/even diagonal overlaps and zero-cost heuristic partitions.
        let sequences: Vec<Vec<u32>> = (0..5)
            .flat_map(|len| {
                (0..1usize << len)
                    .map(move |bits| (0..len).map(|i| ((bits >> i) & 1) as u32).collect())
            })
            .collect();
        for a in &sequences {
            for b in &sequences {
                for cost in [0, 1, 2, DEFAULT_MAX_COSTS] {
                    let runs = change_runs(a, b, None, cost).unwrap();
                    assert_eq!(replay(a, b, &runs), *b, "{a:?} -> {b:?}, cost {cost}");
                }
            }
        }
    }

    #[test]
    fn long_snakes_and_low_cost_unrelated_text_reconstruct() {
        let a: Vec<u32> = (0..14_000).collect();
        let mut b = a.clone();
        for i in (0..b.len()).step_by(35) {
            b[i] = 30_000;
        }
        for cost in [1, 64, DEFAULT_MAX_COSTS] {
            let runs = change_runs(&a, &b, None, cost).unwrap();
            assert_eq!(replay(&a, &b, &runs), b);
        }
        let a = vec![1; 20_000];
        let b = vec![2; 20_000];
        let runs = change_runs(&a, &b, None, 32).unwrap();
        assert_eq!(replay(&a, &b, &runs), b);
    }
}
