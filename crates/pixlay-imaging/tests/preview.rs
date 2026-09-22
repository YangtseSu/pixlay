//! S12's preview criteria, without a window.
//!
//! The window's decoding thread and the CLI's `gesture` probe run the same
//! [`Preview`], so what a gesture step costs is decided here rather than by
//! counting frames: a build that the two caches can answer decodes nothing at all,
//! and the pixels it hands over are the ones a cold build produces — byte for byte,
//! not within a threshold. That last equality is the whole reason the coarse grid
//! is allowed to exist: the release frame is a real resting-grid render, not an
//! upscaled gesture frame.
//!
//! Every case uses real files and the real decoder: a cache measured on a
//! synthetic sampler would be measuring the test.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pixlay_core::{CanvasSpec, Cell, CollageDoc, CropTransform, PixelSize, templates};
use pixlay_imaging::{Built, Depth, GESTURE_GRID_SCALE, Preview, Source, gesture_grid};

/// Two slots with a gutter between them: enough for "one cell changed, the other
/// was carried over" to mean something.
const TEMPLATE: &str = "strip-2-2x1g";

/// The resting grid's long edge in this test, in pixels.
const LONG_EDGE: i32 = 800;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../pixlay-cli/tests/fixtures/photos")
        .join(name)
}

/// Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!(
        "pixlay-preview-tests/{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the test directory");
    dir
}

/// One cell's source per photo, in cell order — what `Project::sources` returns
/// for a document whose cells are all occupied.
fn sources(paths: &[PathBuf]) -> Vec<Option<PathBuf>> {
    paths.iter().cloned().map(Some).collect()
}

/// A document at the template's aspect with `sources` in cell order.
fn document(sources: &[Option<PathBuf>]) -> CollageDoc {
    let template = templates::get(TEMPLATE).expect("the template is in the library");
    assert_eq!(sources.len(), template.slots.len());
    let mut doc = CollageDoc::new(
        CanvasSpec::with_ratio(template.aspect, CanvasSpec::A4_LANDSCAPE.width_mm),
        template,
    );
    for (cell, source) in doc.cells.iter_mut().zip(sources) {
        *cell = Cell {
            source: source.clone(),
            crop: CropTransform::IDENTITY,
            grade: Default::default(),
        };
    }
    doc
}

fn aspect() -> f64 {
    templates::get(TEMPLATE).expect("in the library").aspect
}

/// The resting grid of this test's template.
fn resting() -> PixelSize {
    PixelSize {
        width: LONG_EDGE,
        height: (f64::from(LONG_EDGE) / aspect()).round().max(1.0) as i32,
    }
}

/// A slot's pixels, for comparing two builds of it.
fn pixels(built: &Built, slot: usize) -> Vec<u8> {
    built
        .bitmaps
        .iter()
        .find(|bitmap| bitmap.slot == slot)
        .unwrap_or_else(|| panic!("slot {slot} produced no bitmap"))
        .pixels
        .clone()
}

/// The size of the buffer a source decodes into: `width * height * 4` bytes at 8
/// bits per sample, twice that at 16.
fn source_bytes(source: &Source) -> usize {
    let per_sample = match source.depth() {
        Depth::Eight => 1,
        Depth::Sixteen => 2,
    };
    source.width() as usize * source.height() as usize * 4 * per_sample
}

/// Sets a file's modification time to a fixed instant, so the test does not depend
/// on how fast the filesystem's clock ticks.
fn stamp(path: &Path, seconds: u64) {
    let file = std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open the file to stamp it");
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .expect("set the modification time");
}

#[test]
fn a_second_build_of_the_same_cells_decodes_nothing() {
    let sources = sources(&[fixture("landscape.jpg"), fixture("square.png")]);
    let doc = document(&sources);
    let mut preview = Preview::new();

    // Cold: every occupied cell is decoded once.
    let first = preview.build(&doc, &sources, resting());
    assert_eq!(first.decodes, 2, "the first build must decode both photos");
    assert!(first.failed.is_empty(), "{:?}", first.failed);

    // Warm, one cell framed differently: the other cell is carried over, and
    // neither file is decoded again. This is S12's central claim — the decode left
    // the gesture path — and it is a count, not a duration.
    let mut framed = doc.clone();
    framed.cells[0].crop = CropTransform {
        zoom: 1.4,
        rotation_deg: 12.0,
        ..CropTransform::IDENTITY
    };
    let second = preview.build(&framed, &sources, resting());
    assert_eq!(second.decodes, 0, "a warm step must not touch the disk");
    assert_ne!(
        pixels(&second, 0),
        pixels(&first, 0),
        "the framed cell was rebuilt, so its pixels must follow the new crop"
    );
    assert_eq!(
        pixels(&second, 1),
        pixels(&first, 1),
        "an untouched cell must be carried over unchanged"
    );
    assert_eq!(preview.decodes(), 2, "the cache decoded once per file");

    // And the contrast, in the same test: the same build with a cache that has
    // seen nothing decodes every cell, which is what pre-S12 code did per step.
    let third = Preview::new().build(&framed, &sources, resting());
    assert_eq!(third.decodes, 2, "a cold cache pays for every cell");
    assert_eq!(
        pixels(&third, 0),
        pixels(&second, 0),
        "a cached build and a fresh one are the same pixels"
    );
}

#[test]
fn a_changed_file_is_decoded_again() {
    let dir = scratch("changed-file");
    let photo = dir.join("photo.png");
    std::fs::copy(fixture("square.png"), &photo).expect("copy the photo");
    stamp(&photo, 1_000_000);
    // The same file in both cells: the source cache is what makes that one decode.
    let sources = sources(&[photo.clone(), photo.clone()]);
    let doc = document(&sources);
    let mut preview = Preview::new();

    let first = preview.build(&doc, &sources, resting());
    assert_eq!(
        first.decodes, 1,
        "two cells pointing at one file decode it once"
    );

    // The same path, a different file: the modification time is the evidence, and
    // a stale cache would be a wrong picture.
    std::fs::copy(fixture("landscape.jpg"), &photo).expect("replace the photo");
    stamp(&photo, 2_000_000);
    let second = preview.build(&doc, &sources, resting());
    assert_eq!(second.decodes, 1, "a changed file must be decoded again");
    assert_ne!(
        pixels(&second, 0),
        pixels(&first, 0),
        "the new file's pixels must be what the canvas shows"
    );

    // And once more unchanged: the cache holds the new file now, not the old one.
    let third = preview.build(&doc, &sources, resting());
    assert_eq!(third.decodes, 0);
    assert_eq!(pixels(&third, 0), pixels(&second, 0));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_cache_evicts_the_least_recently_used_source() {
    let paths = [fixture("landscape.jpg"), fixture("square.png")];
    let sources = sources(&paths);
    let doc = document(&sources);
    let sizes: Vec<usize> = paths
        .iter()
        .map(|path| source_bytes(&Source::decode(path).expect("decode")))
        .collect();
    // One byte short of holding both: which one survives is decided by use, and
    // the second lookup is the one that decides it.
    let mut preview = Preview::with_budget(sizes[0] + sizes[1] - 1);

    let first = preview.build(&doc, &sources, resting());
    assert_eq!(first.decodes, 2);
    // Framing the *first* cell again is a cache miss, because the second cell's
    // photo is the one the ring kept: that is the eviction, seen as its cost.
    let mut framed = doc.clone();
    framed.cells[0].crop = CropTransform {
        zoom: 1.2,
        ..CropTransform::IDENTITY
    };
    let second = preview.build(&framed, &sources, resting());
    assert_eq!(
        second.decodes, 1,
        "the least recently used photo was evicted"
    );
    assert_eq!(preview.decodes(), 3);
    assert_eq!(
        pixels(&second, 1),
        pixels(&first, 1),
        "the surviving cell is still carried over"
    );

    // A budget that cannot hold any source caches nothing, and stays correct: the
    // photo is decoded, used and dropped. Both cells are framed so that the
    // *bitmap* cache cannot answer them — otherwise it would, and the source cache
    // would never be asked.
    let mut tiny = Preview::with_budget(1);
    let reframed = {
        let mut doc = doc.clone();
        for cell in doc.cells.iter_mut() {
            cell.crop = CropTransform {
                zoom: 1.2,
                ..CropTransform::IDENTITY
            };
        }
        doc
    };
    assert_eq!(tiny.build(&doc, &sources, resting()).decodes, 2);
    assert_eq!(tiny.build(&reframed, &sources, resting()).decodes, 2);
}

#[test]
fn a_gesture_build_refines_into_the_bytes_a_cold_build_produces() {
    let sources = sources(&[fixture("landscape.jpg"), fixture("portrait.jpg")]);
    let doc = document(&sources);
    let coarse = gesture_grid(resting());
    let mut preview = Preview::new();

    // The sequence a live gesture makes: a coarse frame first, then the resting
    // grid again once the gesture ends.
    let frame = preview.build(&doc, &sources, coarse);
    assert_eq!(frame.decodes, 2);
    let refined = preview.build(&doc, &sources, resting());
    assert_eq!(refined.decodes, 0, "the refinement re-decodes nothing");
    assert!(refined.failed.is_empty(), "{:?}", refined.failed);

    // "Equal to a direct full-quality draw" is a byte-for-byte claim: the same
    // function at the same grid, from a cache that never saw the coarse grid.
    let direct = Preview::new().build(&doc, &sources, resting());
    for slot in 0..sources.len() {
        assert_eq!(
            pixels(&refined, slot),
            pixels(&direct, slot),
            "slot {slot}: the refined frame is not the resting grid's own pixels"
        );
    }
    // And the coarse frame is the coarse grid's own pixels, too: no bitmap crosses
    // between grids.
    let coarse_direct = Preview::new().build(&doc, &sources, coarse);
    for slot in 0..sources.len() {
        assert_eq!(pixels(&frame, slot), pixels(&coarse_direct, slot));
    }
    assert_ne!(
        pixels(&frame, 0).len(),
        pixels(&refined, 0).len(),
        "the coarse grid is a smaller grid, so its bitmaps are smaller"
    );
}

#[test]
fn the_gesture_grid_is_half_the_resting_one() {
    let resting = PixelSize {
        width: 1600,
        height: 1067,
    };
    assert_eq!(
        gesture_grid(resting),
        PixelSize {
            width: 800,
            height: 534
        }
    );
    assert_eq!(GESTURE_GRID_SCALE, 0.5);
    // A one-pixel grid cannot be halved into nothing.
    assert_eq!(
        gesture_grid(PixelSize {
            width: 1,
            height: 1
        }),
        PixelSize {
            width: 1,
            height: 1
        }
    );
}
