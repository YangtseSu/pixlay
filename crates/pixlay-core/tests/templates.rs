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
//! * **The lattice.** Every coordinate is an integer multiple of 1/32, so every
//!   vertex lies on a line of the 32x32 grid and an axis-parallel slot is a union
//!   of whole cells of it. Areas are then sums of exactly representable terms, so
//!   "sums to exactly 1.0" is an equality, not a tolerance.
//! * **The sampling grid.** `SAMPLES = 512` is a multiple of 32, and the sample
//!   points sit at cell *centers* (`(i + 0.5) / 512`, i.e. odd/1024) while every
//!   edge lies on an even/1024 line, so no sample ever lands on a boundary and no
//!   feature is thinner than one step. A covered or uncovered sample is therefore
//!   the whole truth, not a guess at it.

use std::process::Command;

use pixlay_core::templates::{SMOKE_TEMPLATE, TEMPLATE_VERSION, generator};
use pixlay_core::{
    CoreError, MAX_PHOTOS, MAX_SLOTS, MIN_SLOTS, Point, Polygon, Slot, Template, templates,
};

/// Points sampled per axis when rasterizing a template. 512 per axis is 262144
/// samples: a multiple of the 32-cell lattice, so the coverage verdict is exact,
/// and small enough to stay instantaneous.
const SAMPLES: usize = 512;

/// The lattice the geometry is authored on: every coordinate is a multiple of
/// this. See the module comment for why the tests depend on it.
const LATTICE: f64 = 32.0;

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
                    assert_eq!(
                        (value * LATTICE).fract(),
                        0.0,
                        "{} slot {index}: {axis} = {value} is off the 1/32 lattice",
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
    for count in MIN_SLOTS..=MAX_SLOTS {
        let found: Vec<&str> = templates
            .iter()
            .filter(|template| template.slots.len() == count)
            .map(|template| template.name.as_str())
            .collect();
        assert!(
            !found.is_empty(),
            "no template with {count} slots; the library has: {found:?}"
        );
    }
    for template in &templates {
        assert!(
            (MIN_SLOTS..=MAX_SLOTS).contains(&template.slots.len()),
            "{}: {} slots is outside the product range",
            template.name,
            template.slots.len()
        );
    }
}

#[test]
fn every_count_from_two_up_carries_three_layouts_in_two_aspect_families() {
    // S10's exit criterion (ruling 10): the gallery of S14 has to give a person a
    // choice, so every photo count from two carries at least three layouts, spread
    // over at least two aspect families. **Count 1 is the exception and the reason
    // the range starts at two**: it has exactly one layout — the whole sheet, added
    // by S19 — because three one-photo layouts would be three names for one
    // geometry (ruling 34, a single photo is a legal collage). Nine is the ceiling,
    // since S12c removed the ten-slot recipe.
    let templates = templates_under_test();
    let mut histogram = Vec::new();
    for count in 2..=MAX_PHOTOS {
        let layouts: Vec<&Template> = templates
            .iter()
            .filter(|template| template.slots.len() == count)
            .collect();
        let mut aspects: Vec<f64> = layouts.iter().map(|template| template.aspect).collect();
        aspects.sort_by(f64::total_cmp);
        aspects.dedup();
        histogram.push((count, layouts.len(), aspects.len()));
    }
    // The histogram as (photos, layouts, aspect families). One assertion over it,
    // so a failure reports the shape of the whole library rather than one row, and
    // a reader can check the same numbers against `pixlay-render templates`.
    assert!(
        histogram
            .iter()
            .all(|&(_, layouts, families)| layouts >= 3 && families >= 2),
        "every count from 2 to {MAX_PHOTOS} needs at least three layouts in at least two \
         aspect families; the histogram is {histogram:?}"
    );

    let one: Vec<&str> = templates
        .iter()
        .filter(|template| template.slots.len() == 1)
        .map(|template| template.name.as_str())
        .collect();
    assert_eq!(
        one,
        ["grid-1-1x1"],
        "one photo is the sheet and the sheet alone (S19, ruling 34)"
    );
}

#[test]
fn the_matrix_is_grouped_by_aspect_ratio() {
    // `docs/CONTRACT.md` §3: the library is grouped by aspect ratio, because
    // a canvas and a template only fit each other when their ratios agree. A
    // portrait and a landscape variant of the same layout must be separate
    // entries, not one entry that gets stretched.
    let templates = templates_under_test();
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
            name.starts_with("strip-") || name.starts_with("grid-") || name.starts_with("mosaic-"),
            "{name} does not follow the family naming scheme"
        );
        let count: usize = name
            .split('-')
            .nth(1)
            .expect("the name carries a slot count")
            .parse()
            .expect("the slot count is a number");
        assert_eq!(
            count,
            template.slots.len(),
            "{name}: the name says {count} slots"
        );
    }
    // Unknown names are None, not a panic or a default.
    assert!(templates::get("nope").is_none());
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
    let body = &source[source
        .find("    Frozen {")
        .expect("the emitter writes one table row per template")..];
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
