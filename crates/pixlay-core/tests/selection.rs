//! The selection policy: order, the 2–9 clamp, the count filter and the LIFO rule.
//!
//! These are the rules the picker (S13), the layout stage (S14) and the CLI's
//! `init --photo` share, so they are asserted where they live — in `pixlay-core`,
//! with no window and no decoder. The CLI side of the same policy is asserted
//! from the outside in `pixlay-cli/tests/cli.rs`: "the picked list's order is what
//! `init --photo` produces" is a statement about both callers, and each one is
//! checked against this one implementation.

use std::path::PathBuf;

use pixlay_core::{
    ASPECT_TOLERANCE, Cell, CollageDoc, CropTransform, Family, MAX_PHOTOS, MIN_PHOTOS, Removed,
    Selection, SelectionError, last_photo, layout_for, remove_last, templates,
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
    assert!(message.contains("2..=9"), "{message}");
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
    let template = templates::get("strip-2-2x1").expect("registered");
    for count in [0, 1] {
        let selection = selection(count);
        let refused = selection
            .document(&template)
            .expect_err("no collage of fewer than two photos");
        assert_eq!(
            refused,
            SelectionError::PhotoCount {
                found: count,
                min: MIN_PHOTOS,
                max: MAX_PHOTOS,
            }
        );
        assert!(refused.to_string().contains("2..=9"), "{refused}");
    }
    // The floor is only a floor for a *document*: the pick may hold one photo
    // while the user is still picking, which is why `push` allows it.
    assert!(selection(0).accepts_more());
    assert!(selection(1).accepts_more());
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
        // names must be exactly the ones `templates::all` holds for that count,
        // in library order.
        let expected: Vec<String> = templates::all()
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
fn pop_and_remove_drop_photos_in_order_or_by_index() {
    let mut selection = selection(4);
    assert_eq!(selection.pop(), Some(photo("p3")));
    assert_eq!(selection.remove(0), Some(photo("p0")));
    assert_eq!(
        selection.photos(),
        &[photo("p1"), photo("p2")],
        "the rest keeps its order"
    );
    // A stale index is a no-op, not a panic.
    assert_eq!(selection.remove(9), None);
    assert_eq!(selection.pop(), Some(photo("p2")));
    assert_eq!(selection.pop(), Some(photo("p1")));
    assert_eq!(selection.pop(), None);
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
    let removed = remove_last(&mut doc).expect("there is a photo to drop");
    assert_eq!(removed.slot, 3);
    assert_eq!(removed.cell.source, Some(photo("cell3")));
    assert!(doc.cells[3].source.is_none(), "the last cell is cleared");
    // Everything else is untouched — including the cell that was already empty.
    assert_eq!(doc.cells[0].crop, before.cells[0].crop);
    assert_eq!(doc.cells[2].source, Some(photo("cell2")));
    assert!(doc.cells[1].source.is_none());

    // The second removal takes cell 2, so the batch control is LIFO across holes.
    assert_eq!(remove_last(&mut doc).expect("another photo").slot, 2);
    doc.validate().expect("still a valid document");
}

#[test]
fn restore_puts_the_cell_back_where_it_was() {
    let mut doc = occupied("grid-4-2x2");
    // A framing on the cell that is about to leave, so "restored in place" means
    // the *contents* came back, not just a source path.
    doc.cells[1].crop = CropTransform {
        zoom: 3.0,
        offset: (-0.4, 0.6),
        rotation_deg: -22.5,
    };
    let before = doc.clone();
    let removed = remove_last(&mut doc).expect("there is a photo to drop");
    // The last photo of this template is cell 3; removing cell 1 on its own is the
    // per-cell clear, which is a different command.
    assert_eq!(removed.slot, 3);

    removed.restore(&mut doc).expect("the cell is free again");
    assert_eq!(doc, before, "restore is the exact inverse of remove_last");

    // And a hole in the middle comes back into the hole, not into the first empty
    // cell: the batch control must not reorder a user's photos.
    let mut doc = occupied("strip-4-4x1");
    doc.cells[1] = Cell::default();
    let removed = remove_last(&mut doc).expect("a photo");
    removed.restore(&mut doc).expect("restores");
    assert_eq!(doc.cells[3].source, Some(photo("cell3")));
    assert!(doc.cells[1].source.is_none());
}

#[test]
fn restore_refuses_a_cell_that_is_no_longer_free() {
    let mut doc = occupied("strip-3-3x1");
    let removed = remove_last(&mut doc).expect("a photo");
    // Someone put a photo back in the meantime: restoring would drop it, so the
    // caller has to decide instead.
    doc.cells[2].source = Some(photo("other"));
    let refused = removed
        .clone()
        .restore(&mut doc)
        .expect_err("the cell is taken");
    assert!(refused.to_string().contains("slot 2"), "{refused}");
    assert_eq!(doc.cells[2].source, Some(photo("other")));

    // A document that shrank under the removed cell grows back to the layout the
    // cell was taken from — the token carries it — and one that cannot is refused
    // rather than written past the end. (The one that *can* is
    // `restore_returns_the_layout_it_was_taken_from` below.)
    let mut small = occupied("strip-2-2x1");
    let removed = Removed {
        slot: 5,
        cell: Cell::default(),
        template: templates::get("strip-2-2x1").expect("registered"),
    };
    assert!(removed.restore(&mut small).is_err());
    assert_eq!(small.cells.len(), 2, "a refused restore changes nothing");
}

#[test]
fn restore_returns_the_layout_it_was_taken_from() {
    // Ruling 7's control "drops the last photo and brings it back", and the count
    // moves the layout with it. For four photos of `grid-4-2x2` (1:1) there is no
    // three-slot *grid* and no three-slot 1:1 layout at all, so the count rule's
    // own answer on the way down (a 2:3 strip, the nearest aspect) cannot lead back
    // to a grid on the way up: the token has to carry the layout.
    let before = occupied("grid-4-2x2");
    let mut doc = before.clone();
    doc.cells[3].crop = CropTransform {
        zoom: 2.1,
        offset: (-0.2, 0.1),
        rotation_deg: 31.0,
    };
    let before = doc.clone();
    let removed = remove_last(&mut doc).expect("a photo");
    assert_eq!(removed.template.name, "grid-4-2x2");
    // The document shrinks to what the survivors need — that is the count rule, and
    // it is a *different* layout.
    let shrunk =
        layout_for(3, doc.template.aspect, doc.template.family()).expect("a three-slot layout");
    doc.template = shrunk.clone();
    doc.cells.truncate(3);
    assert_ne!(shrunk.name, "grid-4-2x2");

    removed
        .clone()
        .restore(&mut doc)
        .expect("the cell comes back");
    assert_eq!(
        doc, before,
        "remove then restore is the exact inverse, layout included"
    );

    // A layout the user picked while the cell was out is kept when it can host the
    // cell: that pick is newer than the token, and the cell only needs its slot.
    let mut doc = occupied("grid-4-2x2");
    let removed = remove_last(&mut doc).expect("a photo");
    doc.template = templates::get("mosaic-4-hero").expect("registered");
    doc.cells.resize(4, Cell::default());
    removed.restore(&mut doc).expect("the slot is free");
    assert_eq!(
        doc.template.name, "mosaic-4-hero",
        "the user's own layout survives the add-back"
    );
    assert_eq!(doc.cells[3].source, Some(photo("cell3")));
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

    // 1. The same aspect wins, whatever the family asks for: `mosaic-5-hero` is
    //    the library's only 4:3 five-slot layout.
    assert_eq!(
        named(5, thirds, Some(Family::Strip)),
        Some("mosaic-5-hero".to_string())
    );
    assert_eq!(
        named(5, 16.0 / 9.0, Some(Family::Mosaic)),
        Some("strip-5-5x1".to_string())
    );
    // The aspect comparison is the picker's own tolerance, so a ratio that only
    // prints approximately still matches.
    assert_eq!(
        named(5, 1.3333333333333333, Some(Family::Mosaic)),
        Some("mosaic-5-hero".to_string())
    );

    // 2. With no aspect match, the family decides — *before* the distance does:
    //    1.5 is 0.833 away from `strip-3-1x3` (2:3) and only 0.167 away from
    //    `mosaic-3-hero` (4:3), and the strip is still chosen for a strip.
    assert_eq!(
        named(3, 1.5, Some(Family::Strip)),
        Some("strip-3-3x1".to_string())
    );
    // Within one family the nearer aspect wins: 1:1 is 0.33 away from
    // `grid-6-2x3` (2:3) and 0.5 away from `grid-6-3x2` (3:2).
    assert_eq!(
        named(6, 1.0, Some(Family::Grid)),
        Some("grid-6-2x3".to_string())
    );
    // And a family the count has no member of falls through to the aspect: no
    // five-slot grid exists, so the 4:3 mosaic is chosen for a 4:3 count.
    assert_eq!(
        named(5, thirds, Some(Family::Grid)),
        Some("mosaic-5-hero".to_string())
    );

    // 3. The nearest aspect, when nothing above it decided: 1.1 is closest to the
    //    two square four-slot grids, and the earlier recipe is the answer.
    assert_eq!(named(4, 1.1, None), Some("grid-4-2x2".to_string()));

    // 4. Library order breaks a tie the three rungs above leave standing: 1.25 is
    //    exactly as far from 3:2 as from 1:1, so the earlier recipe is chosen.
    assert_eq!(
        named(2, 1.25, None),
        Some("strip-2-2x1".to_string()),
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
    // Outside the range there is nothing to offer — one photo has no layout, and
    // ten left the library with S12c.
    for count in [0, 1, 10, 11] {
        assert_eq!(named(count, 1.0, None), None, "{count} photos");
    }

    // The rule is a preference *among the count's own layouts*: every answer is a
    // template the gallery would list for that count.
    for template in templates::all() {
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
