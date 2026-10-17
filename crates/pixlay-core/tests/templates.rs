// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The template library's invariants.
//!
//! `AGENTS.md` requires the geometry to hold: zero overlap between slots, no
//! interior hole in their union, and — for a cut template — areas summing to
//! exactly 1.0. S2's exit criteria add determinism (the frozen data must be
//! exactly what the recipes produce) and coverage of every slot count from
//! `MIN_SLOTS` to `MAX_SLOTS`.
//!
//! Two things make those checks decidable rather than approximate, and both are
//! asserted here rather than assumed:
//!
//! * **The lattice.** Every coordinate is an integer multiple of the arrangement's
//!   own grid — 1/128 for the collage library, 1/2560 for the ID photo sheets (S40)
//!   — so every vertex lies on a line of that lattice and an axis-parallel slot is a
//!   union of whole cells of it. Areas are then sums of exactly representable terms,
//!   so "sums to exactly 1.0" is an equality, not a tolerance. (2560 is not dyadic,
//!   so a coordinate written on it is the nearest double to `k/2560` and the check
//!   below allows the division's own round-off.)
//! * **The sampling grid.** `SAMPLES = 512` is a multiple of 32, and the sample
//!   points sit at cell *centers* (`(i + 0.5) / 512`, i.e. odd/1024) while every
//!   edge lies on an even/1024 line, so no sample ever lands on a boundary and no
//!   feature is thinner than one step. A covered or uncovered sample is therefore
//!   the whole truth, not a guess at it.

use std::process::Command;

use pixlay_core::templates::{SMOKE_TEMPLATE, TEMPLATE_VERSION, generator};
use pixlay_core::{
    ASPECT_TOLERANCE, CoreError, MAX_PHOTOS, MAX_SLOTS, MAX_TEMPLATE_SLOTS, MIN_SLOTS, Point,
    Polygon, Slot, Template, layout_for, templates,
};

/// Points sampled per axis when rasterizing a template. 512 per axis is 262144
/// samples: a multiple of the 32-cell lattice, so the coverage verdict is exact,
/// and small enough to stay instantaneous.
const SAMPLES: usize = 512;

/// The lattice the geometry is authored on: every coordinate is a multiple of
/// this. See the module comment for why the tests depend on it. 32 was the finest
/// grid until S38, 128 until S40 (the even nine-grid and the 2:3 splits), and the
/// **ID photo sheets** are authored on 2560 — 1/2560 of the 4R page's width, which
/// is what lands a 25 mm cell within 0.03 mm of its size (`generator::ID_GRID`).
/// So the checked lattice is the finest one the library uses.
///
/// The value is not a dyadic rational (2560 = 2^9 * 5), so a coordinate written on
/// it is the *nearest* double to `k/2560` rather than an exact one, and the check
/// below allows the representation's own round-off — 1e-9, which is 1e-5 of a
/// lattice step and far below anything geometry can be about. The sampling grid
/// stays exact on both lattices: a coordinate `k/2560` can never equal a sample
/// `(2i+1)/1024`, which would need `2k = 5(2i+1)` — an even number equal to an odd
/// one.
const LATTICE: f64 = 2560.0;

/// How far a coordinate may sit from the nearest multiple of [`LATTICE`]: the
/// representation's own error, not a tolerance on the geometry.
const LATTICE_EPSILON: f64 = 1e-9;

fn templates_under_test() -> Vec<Template> {
    let templates = templates::all();
    assert!(!templates.is_empty(), "the library ships no template");
    templates
}

fn inside(template: &Template, x: f64, y: f64) -> Vec<usize> {
    template
        .slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.outline.contains(Point::new(x, y)))
        .map(|(index, _)| index)
        .collect()
}

/// The rasterized checks share one pass over the canvas.
fn rasterize(template: &Template) -> (u64, Vec<(usize, usize)>) {
    let mut uncovered = 0u64;
    let mut overlaps: Vec<(usize, usize)> = Vec::new();
    for iy in 0..SAMPLES {
        for ix in 0..SAMPLES {
            let point = (
                (ix as f64 + 0.5) / SAMPLES as f64,
                (iy as f64 + 0.5) / SAMPLES as f64,
            );
            match inside(template, point.0, point.1).as_slice() {
                [] => uncovered += 1,
                [_] => {}
                many => {
                    let pair = (many[0], many[1]);
                    if !overlaps.contains(&pair) {
                        overlaps.push(pair);
                    }
                }
            }
        }
    }
    (uncovered, overlaps)
}

/// True for a cut template: the slots tile the canvas.
fn is_cut(template: &Template) -> bool {
    template.slots.iter().map(|slot| slot.area).sum::<f64>() == 1.0
}

#[test]
fn every_coordinate_lies_on_the_lattice_and_areas_are_exact() {
    // This is the precondition the rest of the file rests on. Without it, "sums
    // to exactly 1.0" and "no sample is ambiguous" both become tolerances that
    // nobody has chosen deliberately.
    for template in templates_under_test() {
        for (index, slot) in template.slots.iter().enumerate() {
            for point in &slot.outline.points {
                for (axis, value) in [("x", point.x), ("y", point.y)] {
                    // Written `k/LATTICE` in the generator and read back here, so the
                    // claim is "this is a lattice coordinate" and the epsilon is only
                    // the division's own rounding.
                    let steps = value * LATTICE;
                    assert!(
                        (steps - steps.round()).abs() <= LATTICE_EPSILON,
                        "{} slot {index}: {axis} = {value} is off the 1/{LATTICE} lattice",
                        template.name
                    );
                }
            }
            // The declared area is the outline's own area, computed from exactly
            // representable coordinates, so this is an equality.
            assert_eq!(
                slot.area,
                slot.outline.area(),
                "{} slot {index}: declared area drifted from the outline",
                template.name
            );
            assert!(slot.area > 0.0, "{} slot {index}: empty", template.name);
        }
    }
}

#[test]
fn slots_never_overlap_and_leave_no_hole() {
    for template in templates_under_test() {
        template.validate().expect("template is valid");
        let (uncovered, overlaps) = rasterize(&template);
        assert!(
            overlaps.is_empty(),
            "{}: slots {overlaps:?} overlap",
            template.name
        );
        if is_cut(&template) {
            // Cut template: the slots must tile the canvas, so an uncovered
            // sample is an interior hole.
            assert_eq!(
                uncovered, 0,
                "{}: {uncovered} uncovered samples in a cut template",
                template.name
            );
        } else if template.one_photo {
            // An ID photo sheet's uncovered area is its cut gaps and its margins
            // (S40): a 2x2 two-inch sheet is 52% white by design, so the "mostly
            // covered" sanity check below does not apply to it. What does is the
            // flood fill — every uncovered sample has to reach the border, which is
            // the next test's own check and the loader's (`topology`).
            assert!(uncovered > 0, "{}: a sheet with no gaps", template.name);
        } else {
            // Not a cut template: only the interior must be covered, which is
            // what "no hole" means for a layout with a gutter. A gutter is fine;
            // a gap between two slots that does not reach the border is not, and
            // that is what the border check below rules out.
            assert!(
                uncovered < (SAMPLES * SAMPLES) as u64 / 2,
                "{}: {uncovered} uncovered samples look like a hole, not a gutter",
                template.name
            );
        }
    }
}

/// Uncovered samples the canvas border cannot reach: the raster's own test for an
/// interior hole, and the independent side of the check the *loader* makes for an
/// embedded geometry (S15g, `pixlay_core::topology`). A flood fill from the border
/// over the uncovered samples proves it — a gap sealed between slots cannot be
/// reached.
fn sealed_uncovered_samples(template: &Template) -> u64 {
    let covered = |ix: usize, iy: usize| {
        let point = (
            (ix as f64 + 0.5) / SAMPLES as f64,
            (iy as f64 + 0.5) / SAMPLES as f64,
        );
        !inside(template, point.0, point.1).is_empty()
    };
    let mut reached = vec![false; SAMPLES * SAMPLES];
    let mut stack = Vec::new();
    let push = |index: usize, reached: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !reached[index] {
            reached[index] = true;
            stack.push(index);
        }
    };
    for axis in 0..SAMPLES {
        for (ix, iy) in [
            (axis, 0),
            (axis, SAMPLES - 1),
            (0, axis),
            (SAMPLES - 1, axis),
        ] {
            if !covered(ix, iy) {
                push(iy * SAMPLES + ix, &mut reached, &mut stack);
            }
        }
    }
    while let Some(index) = stack.pop() {
        let (ix, iy) = (index % SAMPLES, index / SAMPLES);
        let mut neighbours = Vec::new();
        if ix > 0 {
            neighbours.push((ix - 1, iy));
        }
        if ix + 1 < SAMPLES {
            neighbours.push((ix + 1, iy));
        }
        if iy > 0 {
            neighbours.push((ix, iy - 1));
        }
        if iy + 1 < SAMPLES {
            neighbours.push((ix, iy + 1));
        }
        for (nx, ny) in neighbours {
            if !covered(nx, ny) {
                push(ny * SAMPLES + nx, &mut reached, &mut stack);
            }
        }
    }
    (0..SAMPLES * SAMPLES)
        .filter(|&index| !covered(index % SAMPLES, index / SAMPLES) && !reached[index])
        .count() as u64
}

#[test]
fn a_non_cut_template_is_a_gutter_not_an_interior_hole() {
    // For a non-cut layout, "no interior hole" needs its own check: an uncovered
    // sample must be able to reach the canvas border without crossing a slot.
    for template in templates_under_test() {
        if is_cut(&template) {
            continue;
        }
        let sealed = sealed_uncovered_samples(&template);
        assert_eq!(
            sealed,
            0,
            "{}: {sealed} uncovered samples (about {:.4} of the canvas) are sealed off \
             from the border: that is an interior hole, not a gutter",
            template.name,
            sealed as f64 / (SAMPLES * SAMPLES) as f64
        );
    }
}

// ---------------------------------------------------------------------------
// Hand-authored geometry (S15g, PIX-007)
// ---------------------------------------------------------------------------
//
// A `.pixlay` embeds its own template, so its slots may be what a person or a
// script wrote rather than what this build's library ships. Until S15g the global
// invariants held for the library alone (the tests above); these are the same
// invariants asked of a file, and the raster oracle is here to disagree with the
// loader's own validator if it is wrong (`pixlay_core::topology`).

/// A template from hand-written outlines: one vertex list per slot, and each
/// declared area is its outline's (the format cross-checks the two).
fn template(name: &str, slots: &[&[(f64, f64)]]) -> Template {
    let slots = slots
        .iter()
        .map(|points| {
            let outline = Polygon {
                points: points.iter().map(|&(x, y)| Point::new(x, y)).collect(),
            };
            Slot {
                area: outline.area(),
                outline,
            }
        })
        .collect();
    Template {
        name: name.to_string(),
        version: 1,
        aspect: 1.0,
        one_photo: false,
        slots,
    }
}

/// The four corners of `(x0, y0, x1, y1)`, in the order the library's own
/// rectangles use.
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// The hand-authored cases the loader has to **refuse**.
fn refusals() -> Vec<Template> {
    vec![
        // Overlapping, while still inside the canvas between them.
        template(
            "hand-overlap",
            &[&rect(0.0, 0.0, 0.6, 0.6), &rect(0.4, 0.2, 0.8, 0.8)],
        ),
        // Overlapping and over the canvas between them: the areas cannot fit.
        template(
            "hand-overflow",
            &[&rect(0.0, 0.0, 0.6, 1.0), &rect(0.4, 0.0, 1.0, 1.0)],
        ),
        // Four bands around a sealed centre.
        template(
            "hand-ring",
            &[
                &rect(0.0, 0.0, 1.0, 0.4),
                &rect(0.0, 0.6, 1.0, 1.0),
                &rect(0.0, 0.4, 0.4, 0.6),
                &rect(0.6, 0.4, 1.0, 0.6),
            ],
        ),
    ]
}

/// The hand-authored cases the loader has to **accept**. They are the load-bearing
/// half: a refusal that also fired on a gutter (uncovered, but reaching the border)
/// or on a shared edge two slots spell with different arithmetic (a slanted cut off
/// the library's lattice) would make hand-authored templates unusable.
fn acceptances() -> Vec<Template> {
    vec![
        template(
            "hand-gutter",
            &[&rect(0.0, 0.0, 1.0, 0.45), &rect(0.0, 0.55, 1.0, 1.0)],
        ),
        template(
            "hand-slant",
            &[
                &[(0.0, 0.0), (0.3, 0.0), (0.7, 1.0), (0.0, 1.0)],
                &[(0.3, 0.0), (1.0, 0.0), (1.0, 1.0), (0.7, 1.0)],
            ],
        ),
    ]
}

/// One hand-authored case by name, so a test and the equivalence sweep below work
/// on one definition rather than two copies of the same geometry.
fn hand_case(name: &str) -> Template {
    refusals()
        .into_iter()
        .chain(acceptances())
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("no hand-authored case named {name}"))
}

/// The raster's verdict about a template's topology: the overlapping slot pairs it
/// found and the uncovered samples that cannot reach the border.
fn oracle_verdict(template: &Template) -> (Vec<(usize, usize)>, u64) {
    let (_, overlaps) = rasterize(template);
    (overlaps, sealed_uncovered_samples(template))
}

#[test]
fn the_loaders_topology_verdict_matches_the_raster_oracle() {
    // The loader's validator (`pixlay_core::topology`) and the sampling oracle in
    // this file are two different algorithms: a slab decomposition over the exact
    // arrangement, and a 512x512 raster with a flood fill. A template the product
    // ships and every hand-authored case either is refused by both or accepted by
    // both.
    let hand_made = refusals().into_iter().chain(acceptances());
    for case in templates_under_test().into_iter().chain(hand_made) {
        let (overlaps, sealed) = oracle_verdict(&case);
        let refuses = case.validate().is_err();
        assert_eq!(
            refuses,
            !overlaps.is_empty() || sealed > 0,
            "{}: the loader {} it while the raster finds {overlaps:?} overlapping pairs and \
             {sealed} sealed samples",
            case.name,
            if refuses { "refuses" } else { "accepts" }
        );
    }
}

#[test]
fn overlapping_slots_are_refused_naming_both() {
    match hand_case("hand-overlap").validate() {
        Err(CoreError::SlotsOverlap { a, b, x, y }) => {
            assert_eq!((a, b), (0, 1), "the pair is named in order");
            // The witness is a point inside the region they share, so it is
            // something a person can go and look at.
            assert!(
                (0.4..=0.6).contains(&x) && (0.2..=0.6).contains(&y),
                "witness ({x}, {y}) is not in the shared region"
            );
        }
        other => panic!("overlapping slots must be refused, got {other:?}"),
    }
}

#[test]
fn a_geometry_that_takes_more_canvas_than_there_is_is_refused() {
    match hand_case("hand-overflow").validate() {
        Err(CoreError::SlotAreasOverCanvas { sum }) => {
            assert!((sum - 1.2).abs() < 1e-9, "the sum is 1.2, not {sum}");
        }
        other => panic!("a geometry over the canvas must be refused, got {other:?}"),
    }
}

#[test]
fn a_region_sealed_off_from_the_border_is_refused() {
    let case = hand_case("hand-ring");
    match case.validate() {
        Err(CoreError::InteriorHole { x, y }) => assert!(
            (0.4..=0.6).contains(&x) && (0.4..=0.6).contains(&y),
            "witness ({x}, {y}) is not in the sealed centre"
        ),
        other => panic!("a sealed region must be refused, got {other:?}"),
    }
    // The raster agrees, which is the claim that the loader is not the only thing
    // that can see the hole.
    let (overlaps, sealed) = oracle_verdict(&case);
    assert!(overlaps.is_empty(), "the ring does not overlap");
    assert!(sealed > 0, "the raster does not see the sealed centre");
}

#[test]
fn a_gutter_and_a_slanted_cut_are_accepted() {
    for case in acceptances() {
        case.validate().unwrap_or_else(|error| {
            panic!(
                "{} is a template a person may write, refused: {error}",
                case.name
            )
        });
        let (overlaps, sealed) = oracle_verdict(&case);
        assert!(
            overlaps.is_empty() && sealed == 0,
            "{}: the raster disagrees ({overlaps:?}, {sealed} sealed)",
            case.name
        );
    }
}

#[test]
fn an_outline_that_crosses_itself_is_refused() {
    // A bowtie: its two halves both answer `contains`, so as a slot it would claim
    // two regions. The area is positive, so it reaches the simplicity check rather
    // than the zero-area one.
    let bowtie = template(
        "hand-bowtie",
        &[
            &[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.5)],
            &rect(0.6, 0.1, 1.0, 0.3),
        ],
    );
    match bowtie.validate() {
        Err(CoreError::InvalidSlot { slot, reason }) => {
            assert_eq!(slot, 0);
            assert_eq!(reason, "outline crosses itself");
        }
        other => panic!("a self-crossing outline must be refused, got {other:?}"),
    }
    // A spike — consecutive edges doubling back along each other — is the same
    // rule seen from the other side: its area is positive too.
    let spike = template(
        "hand-spike",
        &[
            &[(0.0, 0.0), (1.0, 0.0), (0.5, 0.0), (1.0, 1.0)],
            &rect(0.8, 0.9, 1.0, 1.0),
        ],
    );
    match spike.validate() {
        Err(CoreError::InvalidSlot { slot, reason }) => {
            assert_eq!(slot, 0);
            assert_eq!(reason, "outline crosses itself");
        }
        other => panic!("a doubling-back outline must be refused, got {other:?}"),
    }
}

#[test]
fn cut_templates_declare_areas_summing_to_exactly_one() {
    let mut cut = 0;
    for template in templates_under_test() {
        if !is_cut(&template) {
            continue;
        }
        cut += 1;
        let sum: f64 = template.slots.iter().map(|slot| slot.area).sum();
        // Binary-exact: template coordinates are dyadic rationals by construction
        // (a power-of-two lattice), so this is an equality, not a tolerance.
        assert_eq!(
            sum, 1.0,
            "{}: declared areas sum to {sum}, not exactly 1.0",
            template.name
        );
    }
    assert!(
        cut > 0,
        "no cut template: the exactness claim went unexercised"
    );
}

#[test]
fn every_slot_shares_an_edge_with_its_neighbours_or_meets_the_border() {
    // A template has no isolated island: every slot either touches another slot
    // along a shared edge or touches the canvas border. Without this, a template
    // could satisfy "no hole" by accident of sampling while containing a slot
    // nothing borders.
    for template in templates_under_test() {
        if template.one_photo {
            // An ID photo sheet's copies are separated by their cut gaps (S40), so an
            // interior cell touches neither a neighbour nor the border — which is the
            // layout rather than a defect. What this test guards — a slot sealed off
            // from the sheet, with nothing bordering it — is what the sheet's own
            // grid test measures instead (`an_id_sheet_is_a_grid_of_congruent_cells`).
            continue;
        }
        let seams = template.shared_edges();
        for (index, slot) in template.slots.iter().enumerate() {
            let shares = seams.iter().any(|seam| seam.a == index || seam.b == index);
            let on_border = slot.outline.points.iter().any(|point| {
                point.x <= f64::EPSILON
                    || point.y <= f64::EPSILON
                    || point.x >= 1.0 - f64::EPSILON
                    || point.y >= 1.0 - f64::EPSILON
            });
            assert!(
                shares || on_border,
                "{}: slot {index} neither shares an edge nor touches the border",
                template.name
            );
        }
    }
}

#[test]
fn every_slot_count_in_the_range_is_covered() {
    // S2's exit criterion was "covers 2-10 slots, at least one template each"; the
    // range has moved twice since (S12c dropped ten, S19 added one) and the bound
    // comes from the constants, not from a literal, so widening the product range
    // fails here rather than silently shipping a gap.
    let templates = templates_under_test();
    // The product's counts, which are the **collage's**: a photo count, so the ID
    // photo sheets (whose slots are copies, S40) are not what covers one.
    for count in MIN_SLOTS..=MAX_SLOTS {
        let found: Vec<&str> = templates
            .iter()
            .filter(|template| template.slots.len() == count && !template.one_photo)
            .map(|template| template.name.as_str())
            .collect();
        assert!(
            !found.is_empty(),
            "no collage template with {count} slots; the library has: {found:?}"
        );
    }
    for template in &templates {
        let max = if template.one_photo {
            MAX_TEMPLATE_SLOTS
        } else {
            MAX_SLOTS
        };
        assert!(
            (MIN_SLOTS..=max).contains(&template.slots.len()),
            "{}: {} slots is outside its own range (1..={max})",
            template.name,
            template.slots.len()
        );
    }
}

#[test]
fn every_count_from_two_up_carries_four_layouts_over_four_aspects() {
    // S10's rule (ruling 10) was "at least three layouts in at least two aspect
    // families", because a gallery that offers one candidate is not a choice. S35
    // deepened it: every count from two carries **at least four layouts over at
    // least four aspect ratios, including the album page (4:3), the square (1:1) and
    // a portrait member (2:3 or 3:4)** — the layout stage's ratio row (S36) has
    // something to offer at every count, and the sheet a document was opened on can
    // keep its shape while the collage grows. **Count 1 stays the exception**: it has
    // exactly one layout — the whole sheet, added by S19 — because three one-photo
    // layouts would be three names for one geometry (ruling 34, a single photo is a
    // legal collage). Nine is the ceiling, since S12c removed the ten-slot recipe.
    let templates = templates_under_test();
    let ratio_is = |aspect: f64, other: f64| (aspect - other).abs() <= ASPECT_TOLERANCE;
    let mut histogram = Vec::new();
    for count in 2..=MAX_PHOTOS {
        let layouts: Vec<&Template> = templates
            .iter()
            .filter(|template| template.slots.len() == count)
            .collect();
        let mut aspects: Vec<f64> = layouts.iter().map(|template| template.aspect).collect();
        aspects.sort_by(f64::total_cmp);
        aspects.dedup();
        histogram.push((
            count,
            layouts.len(),
            aspects.len(),
            aspects.iter().any(|&aspect| ratio_is(aspect, 4.0 / 3.0)),
            aspects.iter().any(|&aspect| ratio_is(aspect, 1.0)),
            aspects
                .iter()
                .any(|&aspect| ratio_is(aspect, 2.0 / 3.0) || ratio_is(aspect, 3.0 / 4.0)),
        ));
    }
    // The histogram as (photos, layouts, ratios, has 4:3, has 1:1, has a portrait
    // member). One assertion over it, so a failure reports the shape of the whole
    // library rather than one row, and a reader can check the same numbers against
    // `pixlay-render templates`.
    assert!(
        histogram.iter().all(
            |&(_, layouts, ratios, album, square, portrait)| layouts >= 4
                && ratios >= 4
                && album
                && square
                && portrait
        ),
        "every count from 2 to {MAX_PHOTOS} needs at least four layouts over at least four \
         ratios, carrying 4:3, 1:1 and a portrait member; the histogram as (photos, layouts, \
         ratios, 4:3, 1:1, portrait) is {histogram:?}"
    );

    // 3:4 joined as the library's sixth ratio in S35 and is not a single member
    // standing in for a shape: it is the portrait page, the shape a direction asks
    // for (`docs/ROADMAP.md`), and a ratio with one member has nothing to choose
    // between.
    let portrait_page = templates
        .iter()
        .filter(|template| ratio_is(template.aspect, 3.0 / 4.0))
        .count();
    assert!(
        portrait_page >= 2,
        "3:4 has {portrait_page} members; a ratio that is a shape of its own needs a choice"
    );

    let one: Vec<&str> = templates
        .iter()
        .filter(|template| template.slots.len() == 1)
        .map(|template| template.name.as_str())
        .collect();
    // One photo is the sheet and the sheet alone (S19, ruling 34) — and since S39
    // there is one such sheet per shape the library has, because the sheet's shape
    // *is* the canvas. They are the library's first six members, in the row's order
    // (the 4:3 member is the one whose own name predates the tagging rule).
    assert_eq!(
        one,
        [
            "grid-1-1x1-3x4",
            "grid-1-1x1-1x1",
            "grid-1-1x1-2x3",
            "grid-1-1x1",
            "grid-1-1x1-3x2",
            "grid-1-1x1-16x9",
        ],
        "one photo is the sheet and the sheet alone (S19, ruling 34; S39 gave every \
         sheet its member)"
    );
}

/// A template's cell areas, largest first.
fn areas_largest_first(template: &Template) -> Vec<f64> {
    let mut areas: Vec<f64> = template.slots.iter().map(|slot| slot.area).collect();
    areas.sort_by(|one, other| other.total_cmp(one));
    areas
}

/// S37's measured definition of a **poster**: the largest cell is at least **twice
/// the median cell** — one photo with a mass of cells half its size or less around
/// it, which is Xiaomi's 海报 reading and what the `-hero` variant is the library's
/// name for. The median is the cell at index `count / 2` in descending order (the
/// lower of the two middle cells where the count is even, which is the *looser* of
/// the two readings — no member's verdict differs between them).
///
/// Measured over the library as S36 left it, which is what the step's own note
/// asked for. The property holds for
///
/// * the hero arrangement: `mosaic-3-hero` 0.5 against 0.25 (**2.0x**), `-4-hero`
///   0.5 against 0.15625 (**3.2x**), `-5-hero` 0.5 against 0.125 (**4.0x**),
///   `-6-hero` 0.25 against 0.125 (**2.0x**, its two equal panels counting as one
///   dominant half), `-9-hero` 0.5 against 0.0625 (**8.0x**);
/// * its **transcribed twins** (`-3-t1b2`, `-4-t1b3`, `-5-t1b4`), which carry the
///   same areas by construction — a twin *is* the same arrangement on a rotated
///   sheet, which is why the pair is told apart by its name and why no property can
///   separate one from the other; and
/// * `mosaic-8-s14`, whose L is 0.296875 against a median of 0.09375 (**3.2x**) —
///   the step's own question about count 8, answered by the number, so count 8
///   needs no second member;
///
/// and for **nothing else**: the highest any other member of S36's library reaches
/// is `strip-7-7x1`'s **1.5x**, and `mosaic-7-hero` — the count's only member named
/// for the arrangement — is **1.2x**. Its two largest cells tie at 0.1875 and its
/// median is 0.15625: a three-column mosaic with a large middle, not a poster,
/// whatever its name says. The name is frozen data and stays (`AGENTS.md`), which
/// is why the census below is the property's and not the name's. The gap between
/// the posters (2.0x at the tightest) and the rest (1.5x at the loosest) is what
/// makes the definition worth asserting rather than a knife edge.
fn is_poster(template: &Template) -> bool {
    let areas = areas_largest_first(template);
    match areas.split_first() {
        Some((largest, rest)) => !rest.is_empty() && *largest >= 2.0 * rest[rest.len() / 2],
        None => false,
    }
}

#[test]
fn every_count_from_two_up_carries_a_poster() {
    // S37's rule: every count from two carries a **poster** — the arrangement the
    // `-hero` variant names — asserted through [`is_poster`] rather than through the
    // name, because the measurement above is what the step settled on. Count 2 had
    // none (`strip-2-2x1` and `grid-2-2x1` are equal panes, `strip-2-2x1g` a
    // near-equal pair, `strip-2-1x2` two rows of one size) and count 7's only
    // `-hero` is not one, so both counts gained a member.
    let templates = templates_under_test();
    let posters = |count: usize| -> Vec<String> {
        templates
            .iter()
            .filter(|template| template.slots.len() == count && is_poster(template))
            .map(|template| template.name.clone())
            .collect()
    };
    let mut census = Vec::new();
    for count in 2..=MAX_PHOTOS {
        let found = posters(count);
        assert!(
            !found.is_empty(),
            "{count} cells carry no poster: no layout of that count gives one photo the \
             dominant place (the largest cell twice the median). The library's posters are \
             {:?}",
            templates
                .iter()
                .filter(|template| is_poster(template))
                .map(|template| (template.name.as_str(), template.slots.len()))
                .collect::<Vec<_>>()
        );
        census.push((count, found));
    }
    // The members the step added for the counts that lacked one, named so a rename
    // or a removal cannot pass as a stray census row.
    for (count, name) in [(2, "mosaic-2-hero"), (7, "mosaic-7-l1r6")] {
        assert!(
            census[count - 2].1.iter().any(|found| found == name),
            "{count} cells: {name} is not the poster the census found ({:?})",
            census[count - 2].1
        );
    }

    // The `-hero` variant is the designer's name for the arrangement, and measured
    // it is the property for every member that carries it but one. The exception is
    // pinned, so a later `-hero` that is not a poster cannot join it unnoticed.
    let exceptions: Vec<&str> = templates
        .iter()
        .filter(|template| template.name.ends_with("-hero"))
        .filter(|template| !is_poster(template))
        .map(|template| template.name.as_str())
        .collect();
    assert_eq!(
        exceptions,
        ["mosaic-7-hero"],
        "the `-hero` members that are not posters by the measured property"
    );
}

#[test]
fn the_library_carries_irregular_cells_across_several_counts() {
    // S37's other kind of depth: a slot whose outline is not a rectangle. From S1
    // until S37 the library had exactly **one** such slot — the smoke template's L,
    // which S2's invariants were written against and which `the_smoke_template_…`
    // pins — so everything a concave cell needs (the clip path, the clamp's
    // vertex test, the sketch's outline stroke, the hit test's centroid premise) was
    // exercised by one template. Three members joined it in S37, at three counts,
    // and no two of them repeat a shape: an elbow (an L) at counts 3 and 7, drawn on
    // the album page and on the portrait page, and a tee (a T) at count 5.
    let templates = templates_under_test();
    let irregular: Vec<(usize, String)> = templates
        .iter()
        .filter(|template| {
            template
                .slots
                .iter()
                .any(|slot| slot.outline.points.len() > 4)
        })
        .map(|template| (template.slots.len(), template.name.clone()))
        .collect();
    let counts: Vec<usize> = irregular.iter().map(|(count, _)| *count).collect();
    let mut distinct = counts.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        irregular.len() >= 3 && distinct.len() >= 3,
        "the library carries {} irregular members over {} counts: {irregular:?}",
        irregular.len(),
        distinct.len()
    );
    assert_eq!(
        counts,
        [3, 5, 7, 8],
        "the counts an irregular cell was authored on, the smoke template's own among them"
    );
    for name in ["mosaic-3-elbow", "mosaic-5-tee", "mosaic-7-elbow"] {
        assert!(
            irregular.iter().any(|(_, found)| found == name),
            "{name} left the library, or lost its irregular slot: {irregular:?}"
        );
    }
}

#[test]
fn the_ratio_row_lists_each_counts_own_shapes_in_order() {
    // The row, as a query: the shapes a count offers, in the documented order, one
    // entry per shape, and each entry with at least one layout behind it — a row that
    // offered a shape the strip cannot fill would be a button that switches to nowhere
    // (the strip is `with_slots_and_aspect`, the same shape's members). The row is what
    // a person picks first, so this is the band's own coverage rule beside S35's. The
    // order itself moved once: S36's was ascending, and S38 put the two most-used
    // shapes in front of it.
    let ratio_is = |one: f64, other: f64| (one - other).abs() <= ASPECT_TOLERANCE;
    for count in MIN_SLOTS..=MAX_PHOTOS {
        let row = templates::ratios_for(count);
        assert!(!row.is_empty(), "{count} cells offer no shape at all");
        // S38's documented order: the portrait page (3:4) then the square (1:1), and
        // the rest ascending, so the row is a fixed sequence with the shapes a count
        // lacks simply absent.
        let portrait = 3.0 / 4.0;
        let mut documented = vec![portrait, 1.0];
        documented.retain(|wanted| row.iter().any(|offered| ratio_is(*wanted, *offered)));
        let mut rest: Vec<f64> = row
            .iter()
            .copied()
            .filter(|offered| !documented.iter().any(|wanted| ratio_is(*wanted, *offered)))
            .collect();
        rest.sort_by(f64::total_cmp);
        documented.extend(rest);
        assert_eq!(
            row, documented,
            "{count} cells' row is not the documented order"
        );
        // One entry per shape: the library's members of that count, deduped.
        let mut expected: Vec<f64> = templates_under_test()
            .iter()
            .filter(|template| template.slots.len() == count && !template.one_photo)
            .map(|template| template.aspect)
            .collect();
        expected.sort_by(f64::total_cmp);
        expected.dedup();
        let mut sorted_row = row.clone();
        sorted_row.sort_by(f64::total_cmp);
        assert_eq!(sorted_row.len(), expected.len(), "{count} cells: {row:?}");
        for (offered, wanted) in sorted_row.iter().zip(&expected) {
            assert!(ratio_is(*offered, *wanted), "{count} cells: {row:?}");
        }
        // Every shape the row offers has a layout behind it, and every one of those
        // is that shape — which is what makes a row click safe.
        for ratio in &row {
            let members = templates::with_slots_and_aspect(count, *ratio);
            assert!(
                !members.is_empty(),
                "{count} cells: the row offers {ratio} with no layout"
            );
            for template in &members {
                assert!(
                    ratio_is(template.aspect, *ratio),
                    "{count} cells: {} is in {ratio}'s group and is not that shape",
                    template.name
                );
            }
        }
        // The count's own members are all reachable from the row: nothing the strip
        // could show is filtered out by the row's query.
        let reachable: Vec<String> = row
            .iter()
            .flat_map(|ratio| templates::with_slots_and_aspect(count, *ratio))
            .map(|template| template.name)
            .collect();
        for template in templates_under_test()
            .iter()
            .filter(|template| template.slots.len() == count && !template.one_photo)
        {
            assert!(
                reachable.contains(&template.name),
                "{} is no shape the row offers",
                template.name
            );
        }
        // S35's floor, seen from the row: every count offers at least four shapes —
        // and since S39 every count 1 … 9 offers all six, count 1 included.
        if count >= 2 {
            assert!(row.len() >= 4, "{count} cells offer only {}", row.len());
        }
        assert_eq!(
            row.len(),
            6,
            "{count} cells offer {} shapes where the library has six since S39",
            row.len()
        );
    }
}

#[test]
fn every_sheet_of_a_count_holds_the_arrangements_in_one_order() {
    // S39's ruling — every ratio's members in one order, the irregular cells last —
    // asserted over the library: every count offers all six sheets; each of a count's
    // six cells lists its arrangements in the same order (the order the library lists
    // them, which is the human's list first, the members the list does not name after
    // it, and the irregular cells last); one arrangement's members — one geometry,
    // authored at several sheets — follow the row's own sheet order; and the
    // irregular members come after the rectangular ones in their count.
    let templates = templates_under_test();
    assert_eq!(
        templates::ratios_for(2).len(),
        6,
        "the library ships six sheets"
    );
    let signature = |template: &Template| -> Vec<Vec<(f64, f64)>> {
        template
            .slots
            .iter()
            .map(|slot| {
                slot.outline
                    .points
                    .iter()
                    .map(|point| (point.x, point.y))
                    .collect()
            })
            .collect()
    };
    for count in MIN_SLOTS..=MAX_PHOTOS {
        let row = templates::ratios_for(count);
        assert_eq!(row.len(), 6, "{count} cells offer {} sheets", row.len());
        let in_count: Vec<&Template> = templates
            .iter()
            .filter(|template| template.slots.len() == count)
            .collect();
        // The arrangements, in the order the library lists them.
        let mut arrangements: Vec<Vec<Vec<(f64, f64)>>> = Vec::new();
        for template in &in_count {
            if !arrangements.contains(&signature(template)) {
                arrangements.push(signature(template));
            }
        }
        // Every cell lists them in that order...
        for ratio in &row {
            let cell: Vec<usize> = in_count
                .iter()
                .filter(|template| (template.aspect - ratio).abs() <= ASPECT_TOLERANCE)
                .map(|template| {
                    arrangements
                        .iter()
                        .position(|arrangement| arrangement == &signature(template))
                        .expect("the arrangement was collected")
                })
                .collect();
            assert!(!cell.is_empty(), "{count} cells on {ratio} hold nothing");
            assert!(
                cell.windows(2).all(|pair| pair[0] < pair[1]),
                "{count} cells on {ratio} are out of the canonical order: {cell:?}"
            );
        }
        // ... one arrangement's own sheets follow the row's order...
        for arrangement in &arrangements {
            let sheets: Vec<usize> = in_count
                .iter()
                .filter(|template| &signature(template) == arrangement)
                .map(|template| {
                    row.iter()
                        .position(|ratio| (template.aspect - ratio).abs() <= ASPECT_TOLERANCE)
                        .expect("its sheet is one of the row's")
                })
                .collect();
            assert!(
                sheets.windows(2).all(|pair| pair[0] < pair[1]),
                "{count}: one arrangement's sheets are out of order: {sheets:?}"
            );
        }
        // ... and the irregular members are last in their count.
        let flags: Vec<bool> = in_count
            .iter()
            .map(|template| {
                template
                    .slots
                    .iter()
                    .any(|slot| slot.outline.points.len() > 4)
            })
            .collect();
        assert!(
            flags.windows(2).all(|pair| !(pair[0] && !pair[1])),
            "{count}: an irregular member is not last: {:?}",
            in_count
                .iter()
                .map(|template| template.name.as_str())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn the_default_growth_chain_keeps_the_portrait_shape() {
    // S35's own criterion, moved to the shape the window now opens on: `AddCell`
    // hands every growth to `selection::layout_for` with the document's own aspect
    // and family — the same call the window's count control and the CLI's
    // `edit --add-cell` make. The document starts on the 3:4 sheet since the walk of
    // 2026-10-17 (`pixlay`'s `DEFAULT_TEMPLATE`, which also names the same template),
    // so the promise is that the portrait page survives growing from one photo to
    // nine; before S35 count 2 was the only count with no 4:3 member and the first
    // added photo moved the album page, which is the defect that step closed.
    //
    // The walk is literally what the command does — at each count the next template
    // is `layout_for`'s answer for the aspect the document then carries — and the
    // chain it produces is written down, because "the sheet keeps its shape" is a
    // product promise and a recipe edit that quietly moved the chain has to show up
    // here rather than in someone's document.
    let mut current = templates::get("grid-1-1x1-3x4").expect("the opening template is registered");
    let mut chain = vec![current.name.clone()];
    for count in 2..=MAX_PHOTOS {
        let next = layout_for(count, current.aspect, current.family())
            .unwrap_or_else(|| panic!("no layout for {count} photos"));
        assert_eq!(
            next.slots.len(),
            count,
            "{}: the count the chain asked for",
            next.name
        );
        assert!(
            (next.aspect - current.aspect).abs() <= ASPECT_TOLERANCE,
            "the sheet's shape moved at {count} photos: {} → {} ({} → {})",
            current.name,
            next.name,
            current.aspect,
            next.aspect
        );
        chain.push(next.name.clone());
        current = next;
    }
    assert_eq!(
        chain,
        [
            "grid-1-1x1-3x4",
            "strip-2-1x2-3x4",
            "strip-3-1x3-3x4",
            "strip-4-1x4-3x4",
            "mosaic-5-l3r2-3x4",
            "mosaic-6-t2b4-3x4",
            "mosaic-7-t4b3-3x4",
            "mosaic-8-t3m2b3-3x4",
            "mosaic-9-l5r4-3x4",
        ],
        "the portrait page's growth chain moved; it is what `+` walks from the document \
         the editor opens on"
    );
}

#[test]
fn the_matrix_is_grouped_by_aspect_ratio() {
    // `docs/CONTRACT.md` §3: the library is grouped by aspect ratio, because
    // a canvas and a template only fit each other when their ratios agree. A
    // portrait and a landscape variant of the same layout must be separate
    // entries, not one entry that gets stretched.
    // The collage library's aspects: `of_aspect` answers about layouts a photo count
    // can use, so an ID photo sheet (a copy count, S40) is not in the group.
    let templates: Vec<Template> = templates_under_test()
        .into_iter()
        .filter(|template| !template.one_photo)
        .collect();
    let aspects: Vec<f64> = templates.iter().map(|template| template.aspect).collect();
    assert!(
        aspects.iter().any(|&aspect| aspect < 1.0),
        "no portrait template: every layout would be landscape-only"
    );
    assert!(
        aspects.iter().any(|&aspect| aspect > 1.0),
        "no landscape template"
    );
    assert!(aspects.contains(&1.0), "no square template");

    // The query S7's picker ran: a canvas ratio returns exactly the templates
    // authored for it, and every one of them is a fit (`CollageDoc::validate`
    // makes a mismatch a hard error, so an approximate match would be a bug).
    for &aspect in &aspects {
        let matching = templates::of_aspect(aspect);
        assert_eq!(
            matching.len(),
            aspects.iter().filter(|&&other| other == aspect).count(),
            "of_aspect({aspect}) returned {} templates",
            matching.len()
        );
        for template in &matching {
            assert_eq!(template.aspect, aspect, "{}: wrong group", template.name);
            let doc = templates::document(template);
            doc.validate()
                .unwrap_or_else(|error| panic!("{}: {error}", template.name));
        }
    }
    // A ratio nothing was authored for is an empty answer, not an error.
    assert!(templates::of_aspect(1.7).is_empty());
}

#[test]
fn template_names_are_unique_and_registered() {
    let names = templates::names();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate names in {names:?}");

    for name in names {
        let template =
            templates::get(name).unwrap_or_else(|| panic!("{name} is listed but not registered"));
        assert_eq!(template.name, name);
        // The naming scheme (`templates/mod.rs`) starts with the layout family.
        assert!(
            ["strip-", "grid-", "mosaic-", "id-"]
                .iter()
                .any(|family| name.starts_with(family)),
            "{name} does not follow the family naming scheme"
        );
        // The count is the first segment after the family that is a number: every
        // collage family is `<family>-<slots>-<variant>-<sheet tag>`, and an ID photo
        // sheet's name carries its size before the count (`id-1cun-small-16-2x3`, S40).
        let count: usize = name
            .split('-')
            .skip(1)
            .find_map(|part| part.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("{name} carries no slot count"));
        assert_eq!(
            count,
            template.slots.len(),
            "{name}: the name says {count} slots"
        );
    }
    // Unknown names are None, not a panic or a default.
    assert!(templates::get("nope").is_none());
}

/// The 4R page the ID photo sheets are authored for (S40): 4 x 6 inch, exactly
/// 101.6 x 152.4 mm, whose aspect is 2:3 — the `2x3` sheet tag.
const ID_SHEET_MM: (f64, f64) = (101.6, 152.4);

/// The white between two copies, in millimetres: the recipe's own constant
/// (`generator::ID_GAP_MM`), which this test reads back out of the geometry.
const ID_GAP_MM: f64 = 2.0;

/// Each ID photo sheet: its name, its nominal cell in millimetres, and the copies it
/// carries (columns, rows). The five sizes are the human's ruling of 2026-10-17; the
/// sixth (大二寸) is the other standard two-inch size the same ruling allowed.
/// One ID photo sheet's spec: its name, its nominal cell in millimetres and its
/// copies as `(columns, rows)`.
type IdSheet = (&'static str, (f64, f64), (usize, usize));

const ID_SHEETS: &[IdSheet] = &[
    ("id-1cun-12-2x3", (25.0, 35.0), (3, 4)),
    ("id-1cun-small-16-2x3", (22.0, 32.0), (4, 4)),
    ("id-1cun-large-6-2x3", (33.0, 48.0), (2, 3)),
    ("id-2cun-6-2x3", (35.0, 49.0), (2, 3)),
    ("id-2cun-large-4-2x3", (35.0, 53.0), (2, 2)),
    ("id-2cun-small-6-2x3", (35.0, 45.0), (2, 3)),
];

/// The four edges of a slot's own extent, `(x0, y0, x1, y1)`.
fn extent(slot: &Slot) -> (f64, f64, f64, f64) {
    let mut box_ = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for point in &slot.outline.points {
        box_.0 = box_.0.min(point.x);
        box_.1 = box_.1.min(point.y);
        box_.2 = box_.2.max(point.x);
        box_.3 = box_.3.max(point.y);
    }
    box_
}

#[test]
fn an_id_sheet_is_a_grid_of_congruent_cells() {
    // S40: every slot of an ID photo sheet shows the same photo, so every cell is
    // **the same rectangle on a regular grid** — which is also what lets one bitmap
    // serve all the copies (a bitmap is sized for the slot it was built for, so a
    // non-congruent sheet would resample). The sizes are the recipe's provenance,
    // measured back through the page: on 101.6 x 152.4 mm a normalized extent is a
    // millimetre one, and the bound is the lattice's own resolution (`ID_GRID`) rather
    // than a chosen tolerance — half a step across, half a step down.
    let step_x = ID_SHEET_MM.0 / LATTICE;
    let step_y = ID_SHEET_MM.1 / LATTICE;
    let sheets = templates::one_photo();
    assert_eq!(
        sheets.len(),
        ID_SHEETS.len(),
        "the ID photo sheets this build ships"
    );
    for template in &sheets {
        let (_, cell_mm, (columns, rows)) = *ID_SHEETS
            .iter()
            .find(|(name, _, _)| *name == template.name)
            .unwrap_or_else(|| panic!("{}: not in this test's table", template.name));
        assert!(
            template.one_photo,
            "{}: a sheet holds one photo",
            template.name
        );
        assert_eq!(
            template.aspect,
            2.0 / 3.0,
            "{}: the 4R page is 2:3",
            template.name
        );
        assert_eq!(
            template.slots.len(),
            columns * rows,
            "{}: the copies of one photo",
            template.name
        );

        let (x0, y0, x1, y1) = extent(&template.slots[0]);
        let (width, height) = (x1 - x0, y1 - y0);
        let gap_x = if columns > 1 {
            extent(&template.slots[1]).0 - x1
        } else {
            0.0
        };
        let gap_y = if rows > 1 {
            extent(&template.slots[columns]).1 - y1
        } else {
            0.0
        };
        // The nominal size, in millimetres, as the page reads it.
        assert!(
            (width * ID_SHEET_MM.0 - cell_mm.0).abs() <= step_x,
            "{}: the cell is {:.3} mm across, not {}",
            template.name,
            width * ID_SHEET_MM.0,
            cell_mm.0
        );
        assert!(
            (height * ID_SHEET_MM.1 - cell_mm.1).abs() <= step_y,
            "{}: the cell is {:.3} mm down, not {}",
            template.name,
            height * ID_SHEET_MM.1,
            cell_mm.1
        );
        assert!(
            (gap_x * ID_SHEET_MM.0 - ID_GAP_MM).abs() <= step_x,
            "{}: the gap across is {:.3} mm, not {ID_GAP_MM}",
            template.name,
            gap_x * ID_SHEET_MM.0
        );
        assert!(
            (gap_y * ID_SHEET_MM.1 - ID_GAP_MM).abs() <= step_y,
            "{}: the gap down is {:.3} mm, not {ID_GAP_MM}",
            template.name,
            gap_y * ID_SHEET_MM.1
        );

        // Every copy is that same rectangle, and the grid is regular: row by row,
        // each row left to right — the order the generator writes and the order the
        // strip and `probe` read.
        for row in 0..rows {
            for column in 0..columns {
                let index = row * columns + column;
                let slot = &template.slots[index];
                let box_ = extent(slot);
                // Congruent to within the representation's own round-off: the
                // coordinates are nearest doubles to `k/2560`, so a difference of two
                // of them moves in the last bit.
                assert!(
                    ((box_.2 - box_.0) - width).abs() <= LATTICE_EPSILON
                        && ((box_.3 - box_.1) - height).abs() <= LATTICE_EPSILON,
                    "{}: slot {index} is {:.6}x{:.6}, not the first cell's {width:.6}x{height:.6}",
                    template.name,
                    box_.2 - box_.0,
                    box_.3 - box_.1
                );
                assert_eq!(
                    slot.outline.points.len(),
                    4,
                    "{}: slot {index} is a rectangle",
                    template.name
                );
                let want_x = x0 + column as f64 * (width + gap_x);
                let want_y = y0 + row as f64 * (height + gap_y);
                assert!(
                    (box_.0 - want_x).abs() <= step_x && (box_.1 - want_y).abs() <= step_y,
                    "{}: slot {index} sits at ({:.4}, {:.4}), not ({want_x:.4}, {want_y:.4})",
                    template.name,
                    box_.0,
                    box_.1
                );
            }
        }

        // The counts are the largest that fit: one more copy in either direction
        // would leave the page. The sheet is printed borderless, so what bounds them
        // is the page rather than a margin (the human's ruling of 2026-10-17).
        let fits = |count: usize, cell: f64, gap: f64| {
            count as f64 * cell + (count - 1) as f64 * gap <= 1.0
        };
        assert!(
            fits(columns, width, gap_x) && !fits(columns + 1, width, gap_x),
            "{}: {columns} columns is not the most that fit",
            template.name
        );
        assert!(
            fits(rows, height, gap_y) && !fits(rows + 1, height, gap_y),
            "{}: {rows} rows is not the most that fit",
            template.name
        );
    }
}

#[test]
fn the_smoke_template_is_frozen_by_name_and_version() {
    // `AGENTS.md`'s per-round command names this template, and a document embeds
    // its geometry: renaming it or changing `TEMPLATE_VERSION` breaks every
    // project saved against the old one, so both are pinned here on purpose.
    // S2 rewrote how the geometry is *produced* (a generator, not literals) and
    // changed nothing about what it is.
    assert_eq!(SMOKE_TEMPLATE, "mosaic-8-s14");
    assert_eq!(TEMPLATE_VERSION, 1);

    let template = templates::get(SMOKE_TEMPLATE).expect("registered");
    assert_eq!(template.version, TEMPLATE_VERSION);
    assert_eq!(template.name, SMOKE_TEMPLATE);
    assert_eq!(template.slots.len(), 8, "the name says 8");
    assert_eq!(template.aspect, 4.0 / 3.0);
    assert!(is_cut(&template), "the smoke template tiles its canvas");
    // Slot order is part of the layout: a document's cells are addressed by
    // index, so reordering the slots would move every photo.
    let areas: Vec<f64> = template.slots.iter().map(|slot| slot.area).collect();
    assert_eq!(
        areas,
        vec![
            9.0 / 64.0,
            6.0 / 64.0,
            9.0 / 64.0,
            3.0 / 64.0,
            6.0 / 64.0,
            6.0 / 64.0,
            19.0 / 64.0,
            6.0 / 64.0
        ],
        "the smoke template's area sequence changed, which moves photos"
    );
    // One concave slot: the name says "s14", and the concavity is what makes the
    // clip path, the probe's deepest-point search and S2's area sums non-trivial.
    let irregular = template
        .slots
        .iter()
        .filter(|slot| slot.outline.points.len() > 4)
        .count();
    assert_eq!(irregular, 1, "exactly one slot is irregular");
}

/// The templates that had shipped before S10, each with [`fingerprint`] of its
/// geometry. S10 could add layouts but not move one, and these are how that lasts.
///
/// `strip-10-10x1` was the twelfth row until S12c removed that recipe and moved
/// `DOC_VERSION` to 2: the name is not in the library any more, so a fingerprint
/// for it would be a row no build could check, and the project-compatibility
/// break it stands for is the version bump rather than a moved geometry.
#[rustfmt::skip]
const SHIPPED_BEFORE_S10: [(&str, u64); 11] = [
    ("strip-2-1x2", 8542848068752417087),
    ("strip-2-2x1", 12797188679629256609),
    ("strip-3-3x1", 1976477295676291470),
    ("grid-4-2x2", 13096350302310297877),
    ("grid-4-2x2g", 17389504763641419960),
    ("strip-4-4x1", 5175934212029319384),
    ("mosaic-5-hero", 16738324239986222256),
    ("grid-6-3x2", 12565181570727305131),
    ("mosaic-7-t4b3", 18222112623581619225),
    ("mosaic-8-s14", 1804114584250607058),
    ("grid-9-3x3", 14137598548244331905),
];

/// FNV-1a 64 over the source the generator emits for one template, from its table
/// row on (the file's prose header is not geometry and is not hashed).
///
/// Written out rather than taken from `std` because `DefaultHasher`'s algorithm
/// is not a stable interface, and a fingerprint that changed with the toolchain
/// would fail for the wrong reason (`AGENTS.md`: `cargo test` must not depend on a
/// toolchain version). The emitted floats use `{:?}`, which is the shortest
/// representation that reads back exactly — the same property `frozen.rs` itself
/// relies on.
fn fingerprint(template: &Template) -> u64 {
    let source = generator::emit_source(std::slice::from_ref(template));
    let start = source
        .find("    Frozen {")
        .expect("the emitter writes one table row per template");
    // Up to the table's own end: the emitter's prose and the trailer it writes after
    // the table (S40's `ONE_PHOTO` list) are not part of a template's geometry, and
    // hashing them would fail this test on a comment. The slice keeps the row, the
    // closing `];` and the newline after it, which is what the fingerprints recorded
    // here have always covered.
    let end = source[start..]
        .find("\n\n///")
        .map_or(source.len(), |at| start + at + 1);
    let body = &source[start..end];
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in body.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
fn templates_that_shipped_before_s10_keep_their_geometry() {
    // `AGENTS.md`: a template that ships keeps its name, its version and its
    // geometry forever, because a document embeds a copy of the geometry and a
    // saved project's layout *is* that copy. Regenerating `frozen.rs` cannot check
    // this: editing an old recipe and regenerating moves both sides together. So
    // the geometry of the templates that had already shipped when the gallery was
    // filled in is written down here, and a mismatch is a project-compatibility
    // break rather than a diff to accept.
    let templates = templates_under_test();
    for (name, expected) in SHIPPED_BEFORE_S10 {
        let template = templates
            .iter()
            .find(|template| template.name == name)
            .unwrap_or_else(|| panic!("{name} left the library"));
        let actual = fingerprint(template);
        assert_eq!(
            actual,
            expected,
            "{name}: the geometry of a template that shipped before S10 moved, which changes the \
             layout of every project built on it. Its geometry is now:\n{}",
            generator::emit_source(std::slice::from_ref(template))
        );
    }
}

#[test]
fn the_frozen_data_is_exactly_what_the_generator_produces() {
    // The committed bin, not `build.rs` (S2 review): the frozen geometry is an
    // interface, so regenerating it must be a reviewed commit. This runs that
    // bin into a scratch path and compares the bytes with the committed file, so
    // an edit to a recipe that was not regenerated fails here.
    let scratch = scratch_path("pixlay-gen-templates.rs");
    let _ = std::fs::remove_file(&scratch);
    let output = Command::new(env!("CARGO_BIN_EXE_pixlay-gen-templates"))
        .arg(&scratch)
        .output()
        .expect("run pixlay-gen-templates");
    assert!(
        output.status.success(),
        "the generator failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let committed = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/templates/frozen.rs"),
    )
    .expect("read src/templates/frozen.rs");
    let regenerated = std::fs::read_to_string(&scratch).expect("read the regenerated file");
    let _ = std::fs::remove_file(&scratch);

    assert_eq!(
        regenerated.len(),
        committed.len(),
        "src/templates/frozen.rs is stale by {} bytes: run `cargo run -p pixlay-core \
         --bin pixlay-gen-templates` and review the diff",
        regenerated.len() as i64 - committed.len() as i64
    );
    assert!(
        regenerated == committed,
        "src/templates/frozen.rs differs from the regenerated geometry"
    );

    // And the data a build serves is that frozen data, not a second computation:
    // the recipes are pure, so generating twice must agree anyway, but this is
    // the claim that matters — what ships is what was frozen.
    let generated = generator::generate();
    let served = templates::all();
    assert_eq!(served.len(), generated.len());
    for (frozen, fresh) in served.iter().zip(&generated) {
        assert_eq!(frozen, fresh, "{}: served data differs", frozen.name);
    }
}

/// A scratch path outside the repository. `AGENTS.md` puts artifacts on disk
/// (`/var/tmp` or `$XDG_CACHE_HOME`), never on tmpfs; this one is 8 KB, but the
/// rule has no size threshold and the same helper shape is what the CLI tests use.
fn scratch_path(name: &str) -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-core-tests/{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir.join(name)
}
