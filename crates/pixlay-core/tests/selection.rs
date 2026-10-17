// SPDX-FileCopyrightText: 2026 Yangtse Su <yangtsesu@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! The selection policy: order, the 1–9 clamp, the count filter and the count rule.
//!
//! These are the rules the window, the layout stage (S14) and the CLI's
//! `init --photo` share, so they are asserted where they live — in `pixlay-core`,
//! with no window and no decoder. The CLI side of the same policy is asserted
//! from the outside in `pixlay-cli/tests/cli.rs`: "the selection's order is what
//! `init --photo` produces" is a statement about the window and the CLI, and each
//! one is checked against this one implementation.
//!
//! S14's LIFO rule left this module in S14b (the 2026-09-23 ruling made `+` a
//! layout switch): what remains of it is [`remove_last`], which clears a cell and
//! says which, and the neighbour rule the keyboard's swap needs.

use std::path::PathBuf;

use pixlay_core::{
    ASPECT_TOLERANCE, Cell, CollageDoc, CropTransform, Family, MAX_PHOTOS, MIN_PHOTOS, Selection,
    SelectionError, last_photo, layout_for, layout_with_aspect, remove_last, templates,
};

fn photo(name: &str) -> PathBuf {
    PathBuf::from(format!("/photos/{name}.jpg"))
}

/// A selection of `count` photos, named `p0`, `p1`, … so cell order is readable.
fn selection(count: usize) -> Selection {
    Selection::new(
        (0..count)
            .map(|index| photo(&format!("p{index}")))
            .collect(),
    )
    .expect("a selection inside the clamp")
}

/// The library's **collage** layouts: `templates::all` without the ID photo sheets
/// (S40).
///
/// A sheet's slot count is the copies it prints, not a photo count, so the candidate
/// queries — `Selection::layouts`, `layout_for`, `layout_with_aspect` — answer about
/// these and not about a sheet; the sweeps below compare them against the library, and
/// `templates::all` is what the library is.
fn collage_templates() -> Vec<pixlay_core::Template> {
    templates::all()
        .into_iter()
        .filter(|template| !template.one_photo)
        .collect()
}

/// A document on `template` with a source in every cell.
fn occupied(template: &str) -> CollageDoc {
    let template = templates::get(template).unwrap_or_else(|| panic!("template {template}"));
    let mut doc = CollageDoc::new(template);
    for (index, cell) in doc.cells.iter_mut().enumerate() {
        cell.source = Some(photo(&format!("cell{index}")));
    }
    doc
}

#[test]
fn the_selection_becomes_cells_in_the_order_it_was_given() {
    let selection = selection(3);
    assert_eq!(selection.len(), 3);
    assert_eq!(selection.photos()[0], photo("p0"));
    assert!(!selection.is_empty());

    let doc = selection
        .document(&templates::get("strip-3-3x1").expect("registered"))
        .expect("three photos, three slots");
    let sources: Vec<_> = doc
        .cells
        .iter()
        .map(|cell| cell.source.clone().expect("occupied"))
        .collect();
    assert_eq!(sources, vec![photo("p0"), photo("p1"), photo("p2")]);
    // The document is a valid one: the canvas and the template agree on their
    // aspect, and there is one cell per slot.
    doc.validate().expect("a valid document");
}

#[test]
fn push_refuses_the_tenth_photo_and_names_the_bound() {
    let mut selection = selection(MAX_PHOTOS);
    assert!(!selection.accepts_more());
    let refused = selection.push(photo("extra")).expect_err("refused");
    assert_eq!(
        refused,
        SelectionError::PhotoCount {
            found: MAX_PHOTOS + 1,
            min: MIN_PHOTOS,
            max: MAX_PHOTOS,
        }
    );
    // The message names both bounds, because a user who hits one cannot tell from
    // "too many" whether the limit is 3 or 30.
    let message = refused.to_string();
    assert!(message.contains("1..=9"), "{message}");
    // The refusal changed nothing.
    assert_eq!(selection.len(), MAX_PHOTOS);

    // Since S12c the library stops at nine too, so the ceiling is one number in
    // two places: a ten-photo selection is refused, and no ten-slot layout exists
    // to be offered or to load.
    assert!(templates::get("strip-10-10x1").is_none());
    assert_eq!(
        Selection::new((0..10).map(|_| photo("x")).collect()).expect_err("refused"),
        SelectionError::PhotoCount {
            found: 10,
            min: MIN_PHOTOS,
            max: MAX_PHOTOS,
        }
    );
}

#[test]
fn a_selection_below_the_floor_cannot_become_a_document() {
    // Since S19 the floor is one photo (ruling 34), so the empty selection is the
    // only one below it — and a single photo is not a special case anywhere: it
    // becomes the one-slot sheet like any other count.
    let sheet = templates::get("grid-1-1x1").expect("registered");
    let refused = selection(0)
        .document(&sheet)
        .expect_err("no collage of zero photos");
    assert_eq!(
        refused,
        SelectionError::PhotoCount {
            found: 0,
            min: MIN_PHOTOS,
            max: MAX_PHOTOS,
        }
    );
    assert!(refused.to_string().contains("1..=9"), "{refused}");
    let one = selection(1)
        .document(&sheet)
        .expect("one photo on the sheet");
    assert_eq!(one.cells.len(), 1);
    assert!(one.cells[0].source.is_some());
    one.validate().expect("a valid document");

    // The floor is only a floor for a *document*: the pick may hold no photo while
    // the user is still picking, which is why `push` allows it.
    assert!(selection(0).accepts_more());
}

#[test]
fn a_template_whose_slot_count_differs_is_refused_naming_both_numbers() {
    let selection = selection(3);
    let refused = selection
        .document(&templates::get("strip-2-2x1").expect("registered"))
        .expect_err("three photos cannot fill two slots");
    assert_eq!(
        refused,
        SelectionError::SlotCount {
            template: "strip-2-2x1".to_string(),
            slots: 2,
            photos: 3,
        }
    );
    let message = refused.to_string();
    assert!(message.contains("strip-2-2x1"), "{message}");
    assert!(message.contains('2') && message.contains('3'), "{message}");
}

#[test]
fn layouts_are_exactly_the_templates_with_that_many_slots() {
    for count in MIN_PHOTOS..=MAX_PHOTOS {
        let layouts = selection(count).layouts();
        assert!(!layouts.is_empty(), "{count} photos have no layout");
        for template in &layouts {
            assert_eq!(template.slots.len(), count, "{}", template.name);
        }
        // The filter is the library's own, not a second list that can drift: the
        // names must be exactly the ones the library holds for that count, in library
        // order — the collage library, because a sheet's slots are copies (S40).
        let expected: Vec<String> = collage_templates()
            .into_iter()
            .filter(|template| template.slots.len() == count)
            .map(|template| template.name)
            .collect();
        let listed: Vec<String> = layouts.into_iter().map(|template| template.name).collect();
        assert_eq!(listed, expected, "{count} photos");
    }
    // An empty selection offers nothing: no layout has zero slots.
    assert!(Selection::default().layouts().is_empty());
}

#[test]
fn remove_drops_a_photo_by_index() {
    let mut selection = selection(4);
    assert_eq!(selection.remove(0), Some(photo("p0")));
    assert_eq!(
        selection.photos(),
        &[photo("p1"), photo("p2"), photo("p3")],
        "the rest keeps its order"
    );
    // A stale index is a no-op, not a panic.
    assert_eq!(selection.remove(9), None);
    assert_eq!(selection.remove(2), Some(photo("p3")));
    assert_eq!(selection.photos(), &[photo("p1"), photo("p2")]);
}

#[test]
fn remove_last_clears_only_the_last_occupied_cell() {
    let mut doc = occupied("strip-4-4x1");
    // Framing that a removal must not disturb: cell 0 keeps it, and
    // cell 1 is emptied on its own so the batch control has a *hole* to deal with.
    doc.cells[0].crop = CropTransform {
        zoom: 2.0,
        offset: (0.3, -0.2),
        rotation_deg: 12.0,
    };
    let before = doc.clone();
    doc.cells[1] = Cell::default();

    // The last photo is cell 3, not "the fourth cell": the hole changes nothing,
    // and a batch control that dropped the tail cell instead would drop nothing.
    assert_eq!(last_photo(&doc), Some(3));
    assert_eq!(
        remove_last(&mut doc),
        Some(3),
        "the slot it cleared, named so a caller does not have to guess"
    );
    assert_eq!(
        doc.cells[3],
        Cell::default(),
        "the cleared cell is a whole default cell: no photo and no framing"
    );
    // Everything else is untouched — including the cell that was already empty.
    assert_eq!(doc.cells[0].crop, before.cells[0].crop);
    assert_eq!(doc.cells[2].source, Some(photo("cell2")));
    assert!(doc.cells[1].source.is_none());

    // The second removal takes cell 2, so the batch control steps backwards across
    // holes.
    assert_eq!(remove_last(&mut doc), Some(2));
    doc.validate().expect("still a valid document");
}

#[test]
fn remove_last_answers_none_on_an_empty_document() {
    // Every cell empty: there is nothing to clear, and the answer comes without
    // changing the document to find out.
    let mut doc = CollageDoc::new(templates::get("strip-3-3x1").expect("registered"));
    let before = doc.clone();
    assert_eq!(remove_last(&mut doc), None);
    assert_eq!(doc, before, "a refused removal changes nothing");
}

#[test]
fn a_neighbour_is_the_cell_next_to_this_one_on_the_sheet() {
    // S14b's swap needs a second cell, and the keyboard can only name one by
    // direction. The rule is geometric, because the library is not one row: cell 1
    // of a 2x2 grid is next to cell 3 *above/below*, and index arithmetic would
    // name cell 0 or 2 instead.
    let square = templates::get("grid-4-2x2").expect("registered");
    // The library's slot order for this template is row-major, so 0 is top-left,
    // 1 top-right, 2 bottom-left, 3 bottom-right.
    assert_eq!(square.neighbour(0, (1, 0)), Some(1), "right of 0 is 1");
    assert_eq!(square.neighbour(1, (-1, 0)), Some(0), "left of 1 is 0");
    assert_eq!(square.neighbour(0, (0, 1)), Some(2), "below 0 is 2");
    assert_eq!(square.neighbour(3, (0, -1)), Some(1), "above 3 is 1");
    // The edges of the sheet answer nothing rather than clamping, which is what
    // makes an arrow key at the border a no-op.
    assert_eq!(square.neighbour(0, (-1, 0)), None);
    assert_eq!(square.neighbour(0, (0, -1)), None);
    assert_eq!(square.neighbour(3, (1, 0)), None);
    assert_eq!(square.neighbour(3, (0, 1)), None);
    // A direction of nothing is not a direction.
    assert_eq!(square.neighbour(0, (0, 0)), None);

    // A strip: the same row all the way along, so the left/right neighbours are the
    // index neighbours and up/down answers nothing at all.
    let strip = templates::get("strip-4-4x1").expect("registered");
    assert_eq!(strip.neighbour(0, (1, 0)), Some(1));
    assert_eq!(strip.neighbour(3, (-1, 0)), Some(2));
    assert_eq!(strip.neighbour(1, (0, 1)), None);
    assert_eq!(strip.neighbour(1, (0, -1)), None);

    // A column: the other way round, and the *vertical* neighbour is the index
    // neighbour — which is exactly the case index arithmetic would get right by
    // luck and a row-major assumption would get wrong everywhere else.
    let column = templates::get("strip-3-1x3").expect("registered");
    assert_eq!(column.neighbour(0, (0, 1)), Some(1), "below the top cell");
    assert_eq!(column.neighbour(2, (0, -1)), Some(1));
    assert_eq!(column.neighbour(0, (1, 0)), None);

    // **The stacking case, which is why the rule is geometric**: a mosaic where
    // cell 0 is a tall left column and cells 1 and 2 stack beside it. "Right of 0"
    // has two candidates at the same distance across, and the answer has to be
    // deterministic — the nearer one along the direction, and the lower index when
    // they tie.
    let mosaic = templates::get("mosaic-3-hero").expect("registered");
    let right = mosaic
        .neighbour(0, (1, 0))
        .expect("cell 0 has something right");
    assert!(right != 0, "a cell is not its own neighbour");
    assert!(
        mosaic.slots[right].outline.bbox().x0 >= mosaic.slots[0].outline.bbox().x0,
        "the neighbour is to the right"
    );

    // Every direction on every shipped layout answers with a cell that exists, and
    // never with the cell asked about: the caller indexes with the answer.
    for template in templates::all() {
        for slot in 0..template.slots.len() {
            for direction in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if let Some(other) = template.neighbour(slot, direction) {
                    assert!(other < template.slots.len(), "{}", template.name);
                    assert_ne!(other, slot, "{}: {slot}", template.name);
                }
            }
        }
    }
}

#[test]
fn the_count_rule_picks_the_layout_that_follows_the_document() {
    // S14's count control: when the photo count changes there is no single layout
    // to pick, so `layout_for` answers with a preference order — same aspect, then
    // same recipe family, then the nearest aspect, then library order. Each
    // assertion below is one rung of that order, with the rungs above it absent or
    // tied.
    let named = |count: usize, aspect: f64, family: Option<Family>| {
        layout_for(count, aspect, family).map(|template| template.name)
    };
    let thirds = 4.0 / 3.0;

    // 1. The same aspect outranks the family, and library order settles which of
    //    its members answers: count 5's 4:3 cell holds five members since S39 and
    //    none of them is a strip, so the aspect rung answers with the canonical
    //    order's first — `mosaic-5-l3r2-4x3`.
    assert_eq!(
        named(5, thirds, Some(Family::Strip)),
        Some("mosaic-5-l3r2-4x3".to_string())
    );
    // The same shape with a family that does have members of it: the mosaic family's
    // 16:9 members start at the canonical order's first arrangement too.
    assert_eq!(
        named(5, 16.0 / 9.0, Some(Family::Mosaic)),
        Some("mosaic-5-l3r2-16x9".to_string())
    );
    // The aspect comparison is the library's own tolerance, so a ratio that only
    // prints approximately still matches.
    assert_eq!(
        named(5, 1.3333333333333333, Some(Family::Mosaic)),
        Some("mosaic-5-l3r2-4x3".to_string())
    );

    // 2. With no aspect match, the family decides — *before* the distance does:
    //    count 6's only strip is 16:9 (0.578 away from 1.2) while the nearest shape
    //    overall is 1:1 (0.2 away), and the strip is still chosen for a strip.
    assert_eq!(
        named(6, 1.2, Some(Family::Strip)),
        Some("strip-6-6x1".to_string())
    );
    // The aspect rung outranks the family one: a 4:3 document keeps its shape even
    // when the strip family asks, which is what makes the album page survive a photo
    // being added. Count 2's 4:3 cell carries six members since S39 and the strip
    // family has one of them — `strip-2-1x2-4x3`, the family rung's own answer.
    assert_eq!(
        named(2, thirds, Some(Family::Strip)),
        Some("strip-2-1x2-4x3".to_string())
    );
    // Within one family the nearer aspect wins, and a candidate of another family is
    // not considered even when it is nearer: at 1.2 the nearest grid is the 4:3
    // member (0.133 away, `grid-6-2x3-4x3`) — the count's grids have every shape
    // since S39 — while the square member `grid-6-2x3-1x1` is 0.2 away and
    // `mosaic-6-hero`, of the other family, is 0.133 away too.
    assert_eq!(
        named(6, 1.2, Some(Family::Grid)),
        Some("grid-6-2x3-4x3".to_string())
    );
    // And a family the count has no member of falls through to the aspect: no
    // five-slot grid exists, so the 4:3 answer is the aspect rung's own.
    assert_eq!(
        named(5, thirds, Some(Family::Grid)),
        Some("mosaic-5-l3r2-4x3".to_string())
    );

    // 3. The nearest aspect, when nothing above it decided: 1.1 is closest to the
    //    two square four-slot grids, and the earlier recipe is the answer.
    assert_eq!(named(4, 1.1, None), Some("grid-4-2x2".to_string()));

    // 4. Library order breaks a tie the three rungs above leave standing: 0.875 is
    //    exactly as far from 1:1 as from 3:4 — both shapes carry the count's whole
    //    set since S39 — and the canonical order's first member of either is
    //    `mosaic-5-l3r2-3x4`.
    assert_eq!(
        named(5, 0.875, None),
        Some("mosaic-5-l3r2-3x4".to_string()),
        "a tie on the aspect keeps library order"
    );

    // 5. Every count the product offers has an answer, and it has that count: the
    //    rule is total over `MIN_PHOTOS..=MAX_PHOTOS`, which is what lets the
    //    count control walk the range one step at a time.
    for count in MIN_PHOTOS..=MAX_PHOTOS {
        for aspect in [1.0, thirds, 16.0 / 9.0, 2.0 / 3.0] {
            for family in [None, Some(Family::Strip), Some(Family::Grid)] {
                let template = layout_for(count, aspect, family)
                    .unwrap_or_else(|| panic!("{count} photos, aspect {aspect}, {family:?}"));
                assert_eq!(template.slots.len(), count, "{}", template.name);
            }
        }
    }
    // Outside the range there is nothing to offer — zero photos have no layout,
    // and ten left the library with S12c.
    for count in [0, 10, 11] {
        assert_eq!(named(count, 1.0, None), None, "{count} photos");
    }

    // The rule is a preference *among the count's own layouts*: every answer is a
    // template the gallery would list for that count. A sheet is not one of them (S40):
    // its count is the copies, and no collage has sixteen.
    for template in collage_templates() {
        let count = template.slots.len();
        let chosen = layout_for(count, template.aspect, template.family())
            .expect("its own count has a layout");
        assert!(
            (chosen.aspect - template.aspect).abs() <= ASPECT_TOLERANCE,
            "{}: {} was chosen over a template of the same aspect",
            chosen.name,
            template.name
        );
    }
}

#[test]
fn the_ratio_query_answers_the_shape_asked_for_or_nothing() {
    // S36's query, and the one thing that separates it from the count rule above:
    // it is **exact**. A caller naming a shape is asking about that shape — the
    // layout band's ratio row lists the shapes the count offers, and `edit --aspect`
    // refuses the ones it does not — so a ratio the count has no member of must
    // answer `None` rather than the nearest shape `layout_for` would have chosen.
    let named = |count: usize, aspect: f64, family: Option<Family>| {
        layout_with_aspect(count, aspect, family).map(|template| template.name)
    };
    let thirds = 4.0 / 3.0;

    // The query answers the canonical order's first member of the shape (S39): a
    // cell holds the list's arrangements first and the members the list does not
    // name after them, so the 4:3 cell of a count starts with the list's own.
    assert_eq!(
        named(5, thirds, None),
        Some("mosaic-5-l3r2-4x3".to_string())
    );
    assert_eq!(named(8, thirds, None), Some("grid-8-2x4-4x3".to_string()));
    assert_eq!(named(9, thirds, None), Some("grid-9-3x3-4x3".to_string()));

    // The refusal, which is the whole difference: a shape no template declares is
    // `None` — where the count rule answers the nearest shape instead. `layout_for`
    // is asserted beside it so the two answers are visibly different rules rather
    // than one rule twice. (Every count 1 … 9 carries all six sheets since S39, so
    // the contrast is a shape outside the library and not a count that lacks one.)
    let nine_sixteen = 9.0 / 16.0;
    assert_eq!(
        named(2, nine_sixteen, None),
        None,
        "no template declares 9:16"
    );
    assert_eq!(
        layout_for(2, nine_sixteen, None).map(|template| template.name),
        Some("strip-2-1x2".to_string()),
        "the count rule's nearest-shape answer (2:3), which this query must not give"
    );
    assert_eq!(
        named(5, 1.25, None),
        None,
        "no template declares 5:4 either"
    );
    // Outside the product's range there is nothing at all, as for the count rule.
    for count in [0, 10] {
        assert_eq!(named(count, thirds, None), None, "{count} cells");
    }

    // The match is the library's own tolerance, as `of_aspect`'s is: the ratio a
    // caller rounds still finds its group, and a ratio that is merely *near* one is
    // not that group — the boundary is `ASPECT_TOLERANCE`, not a distance.
    assert_eq!(
        named(8, 1.3333333333333333, None),
        Some("grid-8-2x4-4x3".to_string())
    );
    assert_eq!(named(8, thirds + 20.0 * ASPECT_TOLERANCE, None), None);
    assert_eq!(
        named(8, thirds + 0.5 * ASPECT_TOLERANCE, None),
        Some("grid-8-2x4-4x3".to_string())
    );

    // Where a count carries several layouts of one shape (count 4's square pair, and
    // since S37 five more: 2/4:3, 3/4:3, 5/1:1, 7/4:3 and 7/3:4), the
    // family is the tie-break `layout_for` uses, and library order is what is left
    // when the family has no member of that shape either. Count 4's square layouts
    // were two grids until S38 gave it three mosaics as well, so the family rung is
    // now visible at this count: each kind answers its own earliest recipe.
    assert_eq!(named(4, 1.0, None), Some("grid-4-2x2".to_string()));
    assert_eq!(
        named(4, 1.0, Some(Family::Grid)),
        Some("grid-4-2x2".to_string())
    );
    assert_eq!(
        named(4, 1.0, Some(Family::Mosaic)),
        Some("mosaic-4-t1b3-1x1".to_string()),
        "the earliest 1:1 mosaic, which S38 added"
    );

    // The family preference is honoured wherever the count *can* honour it — the
    // shipped library exercises it only where a shape has several members, so the
    // claim is asserted over the whole collage library: for every template, the query
    // with its own count, shape and family answers that template unless an earlier
    // library-order member has the same shape *and* family. The ID photo sheets are
    // outside it for `the_count_rule`'s reason (S40).
    for template in collage_templates() {
        let count = template.slots.len();
        let chosen = layout_with_aspect(count, template.aspect, template.family())
            .unwrap_or_else(|| panic!("{}: its own shape has no answer", template.name));
        assert!(
            (chosen.aspect - template.aspect).abs() <= ASPECT_TOLERANCE,
            "{}: {} is not the shape asked for",
            template.name,
            chosen.name
        );
        assert_eq!(chosen.slots.len(), count, "{}", chosen.name);
        let same = templates::with_slots_and_aspect(count, template.aspect)
            .into_iter()
            .find(|other| other.family() == template.family())
            .expect("the template itself is one of the count's members of that shape");
        assert_eq!(
            chosen.name, same.name,
            "{}: the family's own member of the shape was not preferred",
            template.name
        );
    }
}
