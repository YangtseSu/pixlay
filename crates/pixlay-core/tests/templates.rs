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
use pixlay_core::{MAX_PHOTOS, MAX_SLOTS, MIN_PHOTOS, MIN_SLOTS, Point, Template, templates};

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

#[test]
fn a_non_cut_template_is_a_gutter_not_an_interior_hole() {
    // For a non-cut layout, "no interior hole" needs its own check: an uncovered
    // sample must be able to reach the canvas border without crossing a slot. A
    // flood fill from the border over the uncovered samples proves it — a gap
    // sealed between slots cannot be reached.
    for template in templates_under_test() {
        if is_cut(&template) {
            continue;
        }
        let step = 1.0 / SAMPLES as f64;
        let covered = |ix: usize, iy: usize| {
            let point = (
                (ix as f64 + 0.5) / SAMPLES as f64,
                (iy as f64 + 0.5) / SAMPLES as f64,
            );
            !inside(&template, point.0, point.1).is_empty()
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
        let sealed = (0..SAMPLES * SAMPLES)
            .filter(|&index| !covered(index % SAMPLES, index / SAMPLES) && !reached[index])
            .count();
        assert_eq!(
            sealed,
            0,
            "{}: {sealed} uncovered samples (about {:.4} of the canvas) are sealed off \
             from the border: that is an interior hole, not a gutter",
            template.name,
            sealed as f64 * step * step
        );
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
fn every_slot_count_from_two_to_ten_is_covered() {
    // S2's exit criterion: "covers 2-10 slots, at least one template each". The
    // bound comes from the constants, not from a literal, so widening the product
    // range fails here rather than silently shipping a gap.
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
fn every_count_the_picker_offers_carries_three_layouts_in_two_aspect_families() {
    // S10's exit criterion (ruling 10): the gallery of S14 has to give a person a
    // choice, so every photo count the picker can produce carries at least three
    // layouts, spread over at least two aspect families. The range is the
    // *selection's* own pair of constants — a picker cannot ask for a count it
    // refuses to select — and its ceiling is 9 rather than the library's
    // `MAX_SLOTS` of 10 because ten photos is a count the product does not offer
    // (ruling 3); the coverage test above is the one that still holds 10 to a
    // layout.
    let templates = templates_under_test();
    let mut histogram = Vec::new();
    for count in MIN_PHOTOS..=MAX_PHOTOS {
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
        "every count from {MIN_PHOTOS} to {MAX_PHOTOS} needs at least three layouts in at least two \
         aspect families; the histogram is {histogram:?}"
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

    // The query S7's picker runs: a canvas ratio returns exactly the templates
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
            pixlay_core::CanvasSpec::with_ratio(aspect, 1000.0);
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
#[rustfmt::skip]
const SHIPPED_BEFORE_S10: [(&str, u64); 12] = [
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
    ("strip-10-10x1", 11821882746657861092),
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
