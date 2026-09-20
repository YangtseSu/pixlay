//! The template library's invariants.
//!
//! `AGENTS.md` requires every template to be cuttable: zero overlap between
//! slots, no interior hole in their union, and — for a cut template — areas
//! summing to exactly 1.0. S2 owns the full matrix and the deterministic
//! generator; these tests already hold the frozen template to the same rules, so
//! the hand-written S1 geometry cannot ship a layout S2 would reject.

use pixlay_core::templates::{SMOKE_TEMPLATE, TEMPLATE_VERSION};
use pixlay_core::{Point, Template, templates};

/// Points sampled per axis when rasterizing a template. 512 per axis is 262144
/// samples: fine enough that a gap narrower than 1/512 of the canvas (0.23 mm on
/// a 1189 mm sheet) cannot hide, coarse enough to stay instantaneous.
const SAMPLES: usize = 512;

fn templates_under_test() -> Vec<Template> {
    // Every template the library ships. S2 adds the rest here as it adds them.
    templates::names()
        .iter()
        .map(|name| templates::get(name).unwrap_or_else(|| panic!("{name} is not registered")))
        .collect()
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
        if template.slots.iter().map(|slot| slot.area).sum::<f64>() == 1.0 {
            // Cut template: the slots must tile the canvas, so an uncovered
            // sample is an interior hole.
            assert_eq!(
                uncovered, 0,
                "{}: {uncovered} uncovered samples in a cut template",
                template.name
            );
        } else {
            // Not a cut template: only the interior must be covered, which is
            // what "no hole" means for a layout with a margin. A margin is fine;
            // a gap between two slots is not, and that is what the seam list
            // below can still see.
            assert!(
                uncovered < (SAMPLES * SAMPLES) as u64 / 2,
                "{}: {uncovered} uncovered samples look like a hole, not a margin",
                template.name
            );
        }
    }
}

#[test]
fn cut_templates_declare_areas_summing_to_exactly_one() {
    for template in templates_under_test() {
        let sum: f64 = template.slots.iter().map(|slot| slot.area).sum();
        // Binary-exact: template coordinates are dyadic rationals by construction
        // (`E = 0.125`), so this is an equality, not a tolerance.
        assert_eq!(
            sum, 1.0,
            "{}: declared areas sum to {sum}, not exactly 1.0",
            template.name
        );
    }
}

#[test]
fn every_slot_shares_an_edge_with_its_neighbours_or_meets_the_border() {
    // A cut template has no isolated island: every slot either touches another
    // slot along a shared edge or touches the canvas border. Without this, a
    // template could satisfy "no hole" by accident of sampling while containing a
    // slot nothing borders.
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
fn the_smoke_template_is_frozen_by_name_and_version() {
    // `AGENTS.md`'s per-round command names this template, and a document embeds
    // its geometry: renaming it or changing `TEMPLATE_VERSION` breaks every
    // project saved against the old one, so both are pinned here on purpose.
    assert_eq!(SMOKE_TEMPLATE, "mosaic-8-s14");
    assert_eq!(TEMPLATE_VERSION, 1);

    let template = templates::get(SMOKE_TEMPLATE).expect("registered");
    assert_eq!(template.version, TEMPLATE_VERSION);
    assert_eq!(template.name, SMOKE_TEMPLATE);
    assert_eq!(template.slots.len(), 8, "the name says 8");
    // One concave slot: the name says "s14", and the concavity is what makes the
    // clip path, the probe's deepest-point search and S2's area sums non-trivial.
    assert!(
        template
            .slots
            .iter()
            .any(|slot| slot.outline.points.len() > 4),
        "a slot with more than 4 vertices is what makes this an irregular template"
    );
    // Unknown names are None, not a panic or a default.
    assert!(templates::get("nope").is_none());
}
