//! S6.5: writing a `.pixlay` — the atomic save, and what a copy does to the
//! paths inside it.
//!
//! The exit criteria the tests here carry: save → load → save is byte-identical,
//! a missing file and a refused version report clearly (the CLI turns both into a
//! non-zero exit code, `pixlay-cli/tests/cli.rs`), and nothing is written that this
//! build could not read back. The atomicity claim is measured where it can be seen
//! from outside: no temporary file survives a save, a failed save writes nothing at
//! all, and an error names the file the caller asked for rather than the temporary
//! one.

use std::path::{Path, PathBuf};

use pixlay_core::{
    Anchor, CanvasSpec, Cell, CollageDoc, CropTransform, DOC_VERSION, FilterPreset, Grade, Point,
    Project, Rgba8, TextLayer, TextMode, templates,
};

fn temp_dir(name: &str) -> PathBuf {
    // Artifacts go to disk, never to tmpfs (`AGENTS.md`, measurement rules).
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/tmp"));
    let dir = base.join(format!("pixlay-core-tests/{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test directory");
    dir
}

/// A document with everything the format carries, so a field that fails to
/// round-trip has somewhere to show up.
fn document() -> CollageDoc {
    let template = templates::get("strip-3-3x1").expect("registered");
    let mut doc = CollageDoc::new(CanvasSpec::with_ratio(template.aspect, 297.0), template);
    doc.cells[0] = Cell {
        source: Some(PathBuf::from("photos/a.jpg")),
        crop: CropTransform {
            zoom: 1.6,
            offset: (0.25, -0.4),
            rotation_deg: -12.5,
        },
        grade: Grade {
            factor: 1.2,
            saturation: 0.85,
            delta: -0.1,
        },
    };
    doc.cells[1].source = Some(PathBuf::from("/absolute/b.png"));
    doc.filter = FilterPreset::Warm;
    doc.text.push(TextLayer {
        content: "{date} #{index}".to_string(),
        mode: TextMode::Free {
            position: Point::new(0.5, 0.9),
            anchor: Anchor::BottomCenter,
        },
        size_rel: 0.03,
        rotation_deg: 6.0,
        color: Rgba8::BLACK,
        source_slot: Some(0),
    });
    doc.text_fallback.date = "2026-09-21".to_string();
    doc
}

/// The file a path names, with `..` resolved: a stored path is allowed to be
/// spelled `proj/../top.png`, and the point of the assertion is which file it
/// resolves to.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every entry of a directory that is not the project itself: a save that left
/// litter behind shows up here.
fn stray_files(dir: &Path, project: &str) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != project)
        .collect()
}

#[test]
fn save_load_save_is_byte_identical() {
    let dir = temp_dir("save-round-trip");
    let path = dir.join("project.pixlay");
    let doc = document();
    doc.save(&path).expect("saves");
    let first = std::fs::read(&path).expect("read");

    // Saving a document that was loaded produces the same bytes: the format has
    // one writer, and a load keeps every field it read.
    let project = Project::load(&path).expect("loads");
    project.save().expect("saves back");
    assert_eq!(std::fs::read(&path).expect("read"), first);
    assert_eq!(project.path(), path.as_path());

    // And again, through a second generation: nothing accumulates.
    let project = Project::load(&path).expect("loads");
    project.save().expect("saves back");
    assert_eq!(std::fs::read(&path).expect("read"), first);

    // The document that comes back is the document that went in, field for field.
    assert_eq!(project.doc(), &doc);
    assert_eq!(
        stray_files(&dir, "project.pixlay"),
        Vec::<String>::new(),
        "a save left files behind"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saving_over_a_project_replaces_it_and_leaves_no_temporary_file() {
    let dir = temp_dir("save-atomic");
    let path = dir.join("project.pixlay");
    document().save(&path).expect("saves");
    let first = std::fs::read_to_string(&path).expect("read");

    // A second, different document over the same path: the file is replaced (this
    // is what saving is, and what `init` deliberately refuses to do).
    let mut second = document();
    second.canvas = CanvasSpec::with_ratio(16.0 / 9.0, 420.0);
    second.save(&path).expect("saves");
    let replaced = std::fs::read_to_string(&path).expect("read");
    assert_ne!(replaced, first);
    assert_eq!(
        CollageDoc::from_json(&replaced).expect("loads"),
        second,
        "the file is the new document"
    );
    assert_eq!(
        stray_files(&dir, "project.pixlay"),
        Vec::<String>::new(),
        "the temporary file survived the rename"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_save_that_cannot_be_written_writes_nothing_and_names_the_target() {
    let dir = temp_dir("save-failure");

    // A directory that does not exist: the temporary file cannot be created
    // either, so nothing is written anywhere.
    let missing = dir.join("nowhere").join("project.pixlay");
    let error = document()
        .save(&missing)
        .expect_err("a missing directory must fail");
    let message = error.to_string();
    assert!(
        message.contains("nowhere/project.pixlay"),
        "the error must name the file that was asked for, got: {message}"
    );
    assert!(
        !message.contains(".tmp"),
        "the error names the temporary file, which nobody asked for: {message}"
    );
    assert!(!missing.exists());

    // An invalid document is refused before anything is written: the file is the
    // user's work, and a build that writes one it cannot read back has made a
    // document nobody can open.
    let mut broken = document();
    broken.cells[1].crop.zoom = 0.0;
    let path = dir.join("broken.pixlay");
    let error = broken
        .save(&path)
        .expect_err("an invalid document must be refused");
    assert!(error.to_string().contains("crop zoom"), "{error}");
    assert!(!path.exists(), "an invalid document was written anyway");
    assert_eq!(
        stray_files(&dir, "broken.pixlay"),
        Vec::<String>::new(),
        "a refused save left litter"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_project_and_a_newer_version_report_clearly() {
    let dir = temp_dir("load-errors");

    let missing = dir.join("absent.pixlay");
    let error = Project::load(&missing).expect_err("a missing file must fail");
    assert!(
        error.to_string().contains("absent.pixlay"),
        "the error must name the missing file: {error}"
    );

    // The version policy: a newer document is refused, an older one is refused
    // with somewhere to go, and neither is guessed at (docs/CONTRACT.md §1).
    let path = dir.join("newer.pixlay");
    let json = document().to_json().expect("serializes").replace(
        &format!("\"docVersion\": {DOC_VERSION}"),
        &format!("\"docVersion\": {}", DOC_VERSION + 1),
    );
    std::fs::write(&path, &json).expect("write");
    let error = Project::load(&path).expect_err("a newer version must be refused");
    assert!(
        error
            .to_string()
            .contains("newer than the supported version"),
        "{error}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_rebases_relative_sources_and_leaves_absolute_ones_alone() {
    let dir = temp_dir("save-as");
    // The project lives one directory down, with a photo beside it, another one
    // above it, and a third one named absolutely.
    let project_dir = dir.join("proj");
    std::fs::create_dir_all(project_dir.join("photos")).expect("create photos");
    std::fs::create_dir_all(dir.join("photos")).expect("create the absolute photo's directory");
    for file in [
        project_dir.join("photos/p.png"),
        dir.join("top.png"),
        dir.join("photos/p.png"),
    ] {
        std::fs::write(&file, b"not really a photo").expect("write");
    }

    let absolute = dir.join("photos").join("p.png");
    let mut doc = document();
    doc.cells[0].source = Some(PathBuf::from("photos/p.png"));
    doc.cells[1].source = Some(absolute.clone());
    doc.cells[2].source = Some(PathBuf::from("../top.png"));

    let original = project_dir.join("a.pixlay");
    doc.save(&original).expect("saves");
    let project = Project::load(&original).expect("loads");
    let resolved = project.sources().expect("resolves");
    assert_eq!(
        canonical(resolved[0].as_ref().expect("a source")),
        canonical(&project_dir.join("photos/p.png"))
    );
    assert_eq!(resolved[1], Some(absolute.clone()));
    assert_eq!(
        canonical(resolved[2].as_ref().expect("a source")),
        canonical(&dir.join("top.png"))
    );

    // A copy two directories down: the relative paths have to keep pointing at the
    // same files, which means one `..` per directory crossed.
    let deep = project_dir.join("copies").join("here");
    std::fs::create_dir_all(&deep).expect("create copies");
    let copy = deep.join("b.pixlay");
    project.save_as(&copy).expect("saves the copy");
    let copied = Project::load(&copy).expect("loads the copy");
    assert_eq!(
        copied.doc().cells[0].source,
        Some(PathBuf::from("../../photos/p.png"))
    );
    assert_eq!(
        copied.doc().cells[2].source,
        Some(PathBuf::from("../../../top.png"))
    );
    assert_eq!(
        copied.doc().cells[1].source,
        Some(absolute.clone()),
        "an absolute source is left as it stands"
    );
    let copied_sources = copied.sources().expect("resolves");
    for (index, (copied, original)) in copied_sources.iter().zip(&resolved).enumerate() {
        assert_eq!(
            copied.clone().map(|path| canonical(&path)),
            original.clone().map(|path| canonical(&path)),
            "cell {index}: the copy points at a different file than the original"
        );
    }

    // And the original is untouched by writing the copy.
    assert_eq!(
        Project::load(&original).expect("loads").doc(),
        &doc,
        "writing a copy changed the project it was copied from"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn save_as_beside_the_original_is_a_plain_copy() {
    // The same directory means every relative path already means the right file,
    // so nothing is rewritten and the copy is byte-identical — which is also what
    // makes "save, load, save" stable.
    let dir = temp_dir("save-as-beside");
    let doc = document();
    let first = dir.join("first.pixlay");
    doc.save(&first).expect("saves");
    let project = Project::load(&first).expect("loads");
    let second = dir.join("second.pixlay");
    project.save_as(&second).expect("saves");
    assert_eq!(
        std::fs::read(&second).expect("read"),
        std::fs::read(&first).expect("read")
    );
    assert_eq!(
        Project::load(&second).expect("loads").doc(),
        &doc,
        "the copy is the same document"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
