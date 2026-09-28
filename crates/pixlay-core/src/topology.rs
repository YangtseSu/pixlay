// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The global topology of a template's slots: no overlap, no interior hole.
//!
//! The library's own invariants (S2) say what a layout has to be — pairwise zero
//! overlap, no interior hole in the union, cut-type areas summing to exactly 1.0 —
//! and until S15g they lived in `crates/pixlay-core/tests/templates.rs` alone,
//! which checks the geometry this build *ships*. A `.pixlay` embeds its own
//! geometry, though, and a hand-authored or script-generated file can say anything;
//! so the loader checks the same rules about the file's own slots (PIX-007, ruled
//! 2026-09-24: the file's geometry is used and validated, not fingerprinted).
//!
//! # What is checked, and why those are the rules
//!
//! * **No overlap.** `draw` paints cells in index order, so the highest index wins
//!   the pixel, while [`Template::slot_at`] answers with the *first* containing
//!   slot: an overlap makes the painted pixels and the hit test disagree about who
//!   owns a region.
//! * **No interior hole.** An uncovered region that cannot reach the canvas border
//!   is a pocket no cell can show and no gutter explains. A gutter (the library's
//!   `g` layouts) is uncovered too, and it is *not* a hole: it reaches the border,
//!   and the frame's own gap reaches it by construction.
//! * **No more than the canvas.** The declared areas are cross-checked against
//!   their own outlines before this runs (`Template::validate`), so a sum above 1.0
//!   can only be geometry that overlaps — refused with the sum named, because the
//!   sum is the number a caller can compare against the canvas. The rest of the
//!   clause — a cut template's areas summing to exactly 1.0 — needs no rule here:
//!   with no overlap, full coverage forces that sum, which is the identity the
//!   library's invariant test asserts as an equality.
//!
//! # The algorithm
//!
//! The canvas is cut into vertical *slabs*. The boundaries are every slot vertex's
//! x, plus every x at which an edge of one slot crosses an edge of another. Inside
//! a slab there is no vertex and no crossing, so the vertical order of the boundary
//! edges cannot change: the y-intervals a vertical line sees through each slot —
//! and therefore their overlaps and their union — keep the same shape, and every
//! endpoint moves linearly with x. One sample x in the middle of a slab is
//! therefore the whole truth about that slab, and no sampling *resolution* is
//! involved: a feature that exists somewhere in a slab is measured at whatever it
//! measures in the middle of it.
//!
//! Connectivity between neighbouring slabs is a one-dimensional question with the
//! same property: an uncovered region's slice in one slab reaches the next when the
//! two slices share a y-range, and the tests below use a strictly positive overlap
//! for it — the safe direction, because a missed connection would report a gutter
//! as a hole while an extra one can only reach the border and hide a pocket.

use crate::error::CoreError;
use crate::geometry::{EPSILON, Point, Polygon};
use crate::template::{AREA_TOLERANCE, Template};

/// One vertical slab of the arrangement, as the middle of it sees it.
struct Slab {
    /// The sample x, strictly between two consecutive boundaries.
    x: f64,
    /// Every slot's own y-intervals at `x`, for the overlap check.
    per_slot: Vec<Vec<(f64, f64)>>,
    /// The y-intervals at `x` that no slot covers, ascending, disjoint, and each
    /// one longer than [`EPSILON`].
    uncovered: Vec<(f64, f64)>,
}

/// Checks the template's slots against each other: see the module comment.
pub(crate) fn validate(template: &Template) -> Result<(), CoreError> {
    let covered: f64 = template.slots.iter().map(|slot| slot.area).sum();
    if covered > 1.0 + AREA_TOLERANCE {
        return Err(CoreError::SlotAreasOverCanvas { sum: covered });
    }
    let slabs = slabs(template);
    for slab in &slabs {
        for a in 0..slab.per_slot.len() {
            for b in (a + 1)..slab.per_slot.len() {
                if let Some(y) = shared_span(&slab.per_slot[a], &slab.per_slot[b]) {
                    return Err(CoreError::SlotsOverlap { a, b, x: slab.x, y });
                }
            }
        }
    }
    if let Some((x, y)) = hole(&slabs) {
        return Err(CoreError::InteriorHole { x, y });
    }
    Ok(())
}

/// The sample x of every slab, ascending.
fn slabs(template: &Template) -> Vec<Slab> {
    sample_xs(&boundaries(template))
        .into_iter()
        .map(|x| {
            let per_slot: Vec<Vec<(f64, f64)>> = template
                .slots
                .iter()
                .map(|slot| intervals(&slot.outline, x))
                .collect();
            let mut crossings: Vec<(f64, f64)> = per_slot.iter().flatten().copied().collect();
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            Slab {
                x,
                uncovered: complement(&merge(crossings)),
                per_slot,
            }
        })
        .collect()
}

/// The x-coordinates the canvas is cut at: every vertex, and every crossing between
/// two slots' edges.
///
/// `0.0` and `1.0` are in the list so that the outermost strips have a slab of
/// their own: an uncovered region there reaches the canvas's left or right edge.
fn boundaries(template: &Template) -> Vec<f64> {
    let mut xs = vec![0.0, 1.0];
    for slot in &template.slots {
        for point in &slot.outline.points {
            xs.push(point.x);
        }
    }
    for a in 0..template.slots.len() {
        for b in (a + 1)..template.slots.len() {
            for (a0, a1) in template.slots[a].outline.edges() {
                for (b0, b1) in template.slots[b].outline.edges() {
                    if let Some(x) = crossing_x(a0, a1, b0, b1) {
                        xs.push(x);
                    }
                }
            }
        }
    }
    // `total_cmp` rather than `partial_cmp`: every value is finite, and a sort that
    // cannot panic is worth the borrow.
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() <= EPSILON);
    xs
}

/// One sample per slab: the middle of each stretch between consecutive boundaries.
fn sample_xs(boundaries: &[f64]) -> Vec<f64> {
    boundaries
        .windows(2)
        .filter(|pair| pair[1] - pair[0] > EPSILON)
        .map(|pair| 0.5 * (pair[0] + pair[1]))
        .collect()
}

/// The y-intervals a vertical line at `x` runs inside `polygon`, by the even-odd
/// rule — the rule [`Polygon::contains`] applies, so the slab and the hit test tell
/// the same story about a point.
///
/// `x` is never a vertex's x (the caller samples between boundaries), so every edge
/// with an endpoint on each side crosses the line exactly once, and the crossings
/// pair up in sorted order.
fn intervals(polygon: &Polygon, x: f64) -> Vec<(f64, f64)> {
    let mut ys: Vec<f64> = Vec::new();
    for (a, b) in polygon.edges() {
        if (a.x > x) != (b.x > x) {
            let t = (x - a.x) / (b.x - a.x);
            ys.push(a.y + t * (b.y - a.y));
        }
    }
    ys.sort_by(f64::total_cmp);
    let mut intervals = Vec::with_capacity(ys.len() / 2);
    let mut index = 0;
    while index + 1 < ys.len() {
        intervals.push((ys[index], ys[index + 1]));
        index += 2;
    }
    intervals
}

/// Merges intervals that touch or overlap into the union's own list.
///
/// Without this the complement of two neighbouring slots' y-ranges would be the
/// sliver between the two floating-point spellings of one shared edge, which is
/// noise rather than a region.
fn merge(intervals: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in intervals {
        match merged.last_mut() {
            Some(last) if lo <= last.1 + EPSILON => last.1 = last.1.max(hi),
            _ => merged.push((lo, hi)),
        }
    }
    merged
}

/// The y-intervals of `[0, 1]` the merged coverage leaves, dropping the slivers
/// thinner than [`EPSILON`]: a shared edge two slots compute from different
/// endpoints leaves one, and it is not a region anything can see.
fn complement(merged: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut uncovered = Vec::new();
    let mut cursor = 0.0f64;
    for &(lo, hi) in merged {
        if lo - cursor > EPSILON {
            uncovered.push((cursor, lo));
        }
        cursor = cursor.max(hi);
    }
    if 1.0 - cursor > EPSILON {
        uncovered.push((cursor, 1.0));
    }
    uncovered
}

/// The middle y of the first span two slots' interval lists share, or `None` when
/// they are disjoint or merely touch.
///
/// The lists are two slots at one slab's sample x, so a shared span is a region the
/// two of them both cover.
fn shared_span(a: &[(f64, f64)], b: &[(f64, f64)]) -> Option<f64> {
    for &(a0, a1) in a {
        for &(b0, b1) in b {
            let (lo, hi) = (a0.max(b0), a1.min(b1));
            if hi - lo > EPSILON {
                return Some(0.5 * (lo + hi));
            }
        }
    }
    None
}

/// The sample point of the first uncovered interval that cannot reach the canvas
/// border, or `None` when every one of them can.
///
/// "Reach the border" is `y = 0`, `y = 1`, or the canvas's left and right edges —
/// and a slice in the first or the last slab has those edges behind it by
/// construction, because no boundary lies between it and the edge.
fn hole(slabs: &[Slab]) -> Option<(f64, f64)> {
    let mut offsets = Vec::with_capacity(slabs.len());
    let mut total = 0usize;
    for slab in slabs {
        offsets.push(total);
        total += slab.uncovered.len();
    }
    let mut reached = vec![false; total];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let visit =
        |slab: usize, interval: usize, reached: &mut Vec<bool>, stack: &mut Vec<(usize, usize)>| {
            let node = offsets[slab] + interval;
            if !reached[node] {
                reached[node] = true;
                stack.push((slab, interval));
            }
        };
    for (index, slab) in slabs.iter().enumerate() {
        let outer = index == 0 || index + 1 == slabs.len();
        for (interval, &(lo, hi)) in slab.uncovered.iter().enumerate() {
            if outer || lo <= EPSILON || hi >= 1.0 - EPSILON {
                visit(index, interval, &mut reached, &mut stack);
            }
        }
    }
    while let Some((index, interval)) = stack.pop() {
        let span = slabs[index].uncovered[interval];
        for neighbour in [
            index.checked_sub(1),
            (index + 1 < slabs.len()).then_some(index + 1),
        ] {
            let Some(neighbour) = neighbour else {
                continue;
            };
            for (other, &candidate) in slabs[neighbour].uncovered.iter().enumerate() {
                // A strictly positive overlap: the two slices share a y-range, so
                // the region crosses the boundary between the slabs.
                if span.0.max(candidate.0) < span.1.min(candidate.1) {
                    visit(neighbour, other, &mut reached, &mut stack);
                }
            }
        }
    }
    for (index, slab) in slabs.iter().enumerate() {
        for (interval, &(lo, hi)) in slab.uncovered.iter().enumerate() {
            if !reached[offsets[index] + interval] {
                return Some((slab.x, 0.5 * (lo + hi)));
            }
        }
    }
    None
}

/// The x at which two edges cross, when they do.
///
/// Parallel and collinear pairs answer `None`: a collinear contact does not change
/// the vertical order of anything inside a slab, and its own ends are vertices,
/// which are boundaries already.
fn crossing_x(a0: Point, a1: Point, b0: Point, b1: Point) -> Option<f64> {
    let (ax, ay) = (a1.x - a0.x, a1.y - a0.y);
    let (bx, by) = (b1.x - b0.x, b1.y - b0.y);
    let (alen, blen) = (ax.hypot(ay), bx.hypot(by));
    if alen <= 0.0 || blen <= 0.0 {
        return None;
    }
    // Sine of the angle between the edges: 0 means parallel, and the division by
    // both lengths makes the tolerance a statement about the angle rather than
    // about the coordinate magnitudes.
    let denominator = ax * by - ay * bx;
    if (denominator / (alen * blen)).abs() <= EPSILON {
        return None;
    }
    let (dx, dy) = (b0.x - a0.x, b0.y - a0.y);
    let t = (dx * by - dy * bx) / denominator;
    let u = (dx * ay - dy * ax) / denominator;
    if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
        return None;
    }
    Some(a0.x + t * ax)
}
