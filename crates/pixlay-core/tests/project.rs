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
    Cell, CollageDoc, CoreError, CropTransform, DOC_VERSION, Point, Polygon, Project, Slot,
    Template, templates,
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
    let mut doc = CollageDoc::new(template);
    doc.cells[0] = Cell {
        source: Some(PathBuf::from("photos/a.jpg")),
        crop: CropTransform {
            zoom: 1.6,
            offset: (0.25, -0.4),
            rotation_deg: -12.5,
        },
    };
    doc.cells[1].source = Some(PathBuf::from("/absolute/b.png"));
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
    second.frame.gap_rel = 0.05;
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

/// S15h (PIX-021): a file-backed failure says which file it was.
///
/// The message inside — serde's, or a validation refusal — does not name the
/// project; the loader is what knows the path, and this asserts that it passes it
/// on. An I/O failure is the other side of the same rule: it already names its
/// path, so it must not be wrapped a second time.
#[test]
fn a_file_backed_failure_names_the_project_it_read() {
    let dir = temp_dir("load-error-path");

    // A parse failure keeps serde's detail and gains the path.
    let broken = dir.join("broken.pixlay");
    std::fs::write(&broken, "{ not json").expect("write");
    let error = CollageDoc::load(&broken).expect_err("a parse failure must fail");
    assert!(
        matches!(error, CoreError::AtPath { .. }),
        "a parse failure has to carry the path: {error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("broken.pixlay"), "{message}");
    assert!(
        message.contains("project JSON"),
        "serde's detail is kept: {message}"
    );

    // A validation failure too: the file was read and parsed, and it is still the
    // file that has to be named.
    let mut invalid = document();
    invalid.cells[1].crop.zoom = 0.0;
    let path = dir.join("invalid.pixlay");
    std::fs::write(&path, invalid.to_json().expect("serializes")).expect("write");
    let error = CollageDoc::load(&path).expect_err("an invalid document must fail");
    assert!(
        matches!(error, CoreError::AtPath { .. }),
        "a validation failure has to carry the path: {error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("invalid.pixlay"), "{message}");
    assert!(
        message.contains("crop zoom"),
        "the reason is kept: {message}"
    );

    // A missing file already names its path: wrapping it would print the path
    // twice, which is what `AtPath` exists to avoid.
    let missing = dir.join("absent.pixlay");
    let error = Project::load(&missing).expect_err("a missing file must fail");
    assert!(
        matches!(error, CoreError::Io { .. }),
        "an I/O failure is not wrapped: {error:?}"
    );
    let message = error.to_string();
    assert_eq!(
        message.matches("absent.pixlay").count(),
        1,
        "the missing path is named once: {message}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A template from hand-written outlines: the geometry a `.pixlay` may carry and
/// this build's library never produces (S15g).
fn hand_made(name: &str, slots: &[&[(f64, f64)]]) -> Template {
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

/// The four corners of `(x0, y0, x1, y1)`.
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

#[test]
fn a_document_whose_own_geometry_breaks_the_rules_is_refused_at_load() {
    // S15g's exit criterion: the loader checks the geometry the *file* carries, not
    // only the geometry this build's library ships (PIX-007, ruled 2026-09-24). The
    // three cases are the ruling's own — overlapping slots, a region sealed off from
    // the border, and an outline that crosses itself — and each is written by
    // `to_json`, which only serializes, so the test can leave exactly the file a
    // hand edit leaves.
    let dir = temp_dir("topology");
    let cases: [(&str, Template, &str); 3] = [
        (
            "overlap",
            hand_made(
                "hand-overlap",
                &[&rect(0.0, 0.0, 0.6, 0.6), &rect(0.4, 0.2, 0.8, 0.8)],
            ),
            "overlap at",
        ),
        (
            "hole",
            hand_made(
                "hand-ring",
                &[
                    &rect(0.0, 0.0, 1.0, 0.4),
                    &rect(0.0, 0.6, 1.0, 1.0),
                    &rect(0.0, 0.4, 0.4, 0.6),
                    &rect(0.6, 0.4, 1.0, 0.6),
                ],
            ),
            "interior hole at",
        ),
        (
            "bowtie",
            hand_made(
                "hand-bowtie",
                &[
                    &[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.5)],
                    &rect(0.6, 0.1, 1.0, 0.3),
                ],
            ),
            "outline crosses itself",
        ),
    ];
    for (name, template, expected) in cases {
        let path = dir.join(format!("{name}.pixlay"));
        let json = CollageDoc::new(template).to_json().expect("serializes");
        std::fs::write(&path, json).expect("write");
        let error = Project::load(&path).expect_err("the loader must refuse it");
        assert!(
            error.to_string().contains(expected),
            "{name}: expected a message about {expected:?}, got {error}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_document_whose_geometry_is_fine_still_loads() {
    // The other half of the criterion, and the one a refusal can quietly break: a
    // gutter (uncovered, but reaching the border) and a slanted cut (a seam off the
    // library's lattice) are templates a person may write, so a project with either
    // loads.
    let dir = temp_dir("topology-ok");
    for (name, template) in [
        (
            "gutter",
            hand_made(
                "hand-gutter",
                &[&rect(0.0, 0.0, 1.0, 0.45), &rect(0.0, 0.55, 1.0, 1.0)],
            ),
        ),
        (
            "slant",
            hand_made(
                "hand-slant",
                &[
                    &[(0.0, 0.0), (0.3, 0.0), (0.7, 1.0), (0.0, 1.0)],
                    &[(0.3, 0.0), (1.0, 0.0), (1.0, 1.0), (0.7, 1.0)],
                ],
            ),
        ),
    ] {
        let path = dir.join(format!("{name}.pixlay"));
        let json = CollageDoc::new(template.clone())
            .to_json()
            .expect("serializes");
        std::fs::write(&path, json).expect("write");
        let project = Project::load(&path)
            .unwrap_or_else(|error| panic!("{name} is a document a person may write: {error}"));
        assert_eq!(project.doc().template.slots.len(), template.slots.len());
    }
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

#[test]
fn a_dot_dot_in_any_path_still_rebases_to_the_same_file() {
    // PIX-006 (S15d): the rebase compares `Path::components` literally, so a `..`
    // in the project path, the copy path or the source itself used to produce a
    // relative source that resolved somewhere else — `/a/b` to `/a/b/../copy.pixlay`
    // handed back one `..` too many. `normalize_lexical` is what both sides go
    // through now, and this is the round trip the finding's own example names.
    let dir = temp_dir("dot-dot");
    let project_dir = dir.join("a").join("b");
    std::fs::create_dir_all(project_dir.join("photos")).expect("create the photos");
    let photo = project_dir.join("photos/p.png");
    std::fs::write(&photo, b"not really a photo").expect("write");

    let mut doc = document();
    doc.cells[0].source = Some(PathBuf::from("photos/p.png"));
    // A source that walks into a subdirectory and back out: the same file, spelled
    // the long way. The subdirectory has to exist for the *filesystem* to resolve
    // it — the kernel walks every component, `..` included — which is exactly why
    // the rebase collapses the spelling rather than leaving it to the reader.
    std::fs::create_dir_all(project_dir.join("sub")).expect("create the subdirectory");
    doc.cells[1].source = Some(PathBuf::from("sub/../photos/p.png"));
    let original = project_dir.join("a.pixlay");
    doc.save(&original).expect("saves");
    let project = Project::load(&original).expect("loads");
    let resolved = project.sources().expect("resolves");
    assert_eq!(
        canonical(resolved[0].as_ref().expect("a source")),
        canonical(&photo)
    );

    // The copy is spelled with a `..` that lands in the directory above the
    // project: `/a/b/../copy.pixlay` is `/a/copy.pixlay`, so the photos one
    // directory down are `b/photos/p.png` from there.
    let copy = dir.join("a").join("b").join("..").join("copy.pixlay");
    let written = project.save_as(&copy).expect("saves the copy");
    assert_eq!(
        written.doc().cells[0].source,
        Some(PathBuf::from("b/photos/p.png")),
        "the copy's source does not resolve to the photo"
    );
    assert_eq!(
        written.doc().cells[1].source,
        Some(PathBuf::from("b/photos/p.png")),
        "a `..` inside the source was not collapsed"
    );
    let copied = Project::load(&copy).expect("loads the copy");
    let copied_sources = copied.sources().expect("resolves");
    for (index, (copied, original)) in copied_sources.iter().zip(&resolved).enumerate() {
        assert_eq!(
            copied.clone().map(|path| canonical(&path)),
            original.clone().map(|path| canonical(&path)),
            "cell {index}: the copy points at a different file than the original"
        );
    }

    // A project *path* with a `..` is the same document read from the directory it
    // means, and saving it beside itself rewrites nothing.
    let spelled = dir.join("a").join("b").join("..").join("copy.pixlay");
    let loaded = Project::load(&spelled).expect("loads a `..` spelling");
    let again = dir.join("a").join("second.pixlay");
    loaded.save_as(&again).expect("saves beside itself");
    assert_eq!(
        std::fs::read(&again).expect("read"),
        std::fs::read(&copy).expect("read"),
        "saving beside itself is a plain copy, `..` or not"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every temporary file this build's writer left behind, by the name it gives them.
fn temporary_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .expect("read directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

/// The mode of a file, as the permission bits alone.
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::metadata(path).expect("stat").permissions().mode() & 0o777
}

#[test]
fn saving_over_a_private_project_keeps_it_private() {
    use std::os::unix::fs::PermissionsExt as _;

    // A project the user made private — `umask 077`, a `chmod`, whatever the reason —
    // may not come back readable by everyone because it was written through a
    // temporary file (S15c, PIX-016): the mode belongs to the file, not to the way
    // this build replaces it.
    let dir = temp_dir("save-mode");
    let path = dir.join("private.pixlay");
    document().save(&path).expect("saves");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    document().save(&path).expect("saves again");
    assert_eq!(mode(&path), 0o600, "the save widened a private file's mode");

    // The other direction too: a mode the user chose is kept, not replaced by a
    // default of this build's.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    document().save(&path).expect("saves again");
    assert_eq!(
        mode(&path),
        0o644,
        "the save narrowed a readable file's mode"
    );

    // A file that is not there yet gets what any new file gets: the process's umask
    // default, measured against a file this test writes with `std::fs::write` rather
    // than written down, because the umask is the environment's.
    let reference = dir.join("reference");
    std::fs::write(&reference, b"x").expect("write");
    let fresh = dir.join("fresh.pixlay");
    document().save(&fresh).expect("saves");
    assert_eq!(
        mode(&fresh),
        mode(&reference),
        "a new project does not get the umask's default"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_write_that_fails_in_the_middle_leaves_the_previous_file_byte_identical() {
    use pixlay_core::atomic;
    use std::io::Write as _;

    // The failure an export hits when the disk fills up or an encoder breaks: the
    // body has already written bytes and then fails. What must not happen is the
    // previous file disappearing with them (S15c, PIX-011) — and the writer under
    // test is the one a project save and an image export both go through.
    let dir = temp_dir("atomic-body-failure");
    let path = dir.join("project.pixlay");
    document().save(&path).expect("saves");
    let before = std::fs::read(&path).expect("read");

    let failure = atomic::write_atomic(&path, |file| {
        file.write_all(b"{ half a document")
            .and_then(|()| Err(std::io::Error::other("the disk is full")))
    })
    .expect_err("the body failed");
    match failure {
        atomic::Failure::Body(source) => assert_eq!(source.to_string(), "the disk is full"),
        other => panic!("the body's own error must come back, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(&path).expect("read"),
        before,
        "the previous file changed under a failed write"
    );
    assert_eq!(
        temporary_files(&dir),
        Vec::<String>::new(),
        "the temporary file survived a failed write"
    );

    // And the same when the failure is the rename's rather than the body's: a
    // directory sits at the target's name, so the content is written and cannot be
    // put where it was asked to go. The directory is untouched and the temporary
    // file is gone.
    let blocked = dir.join("blocked.pixlay");
    std::fs::create_dir(&blocked).expect("create directory");
    let failure = atomic::write_atomic(&blocked, |file| file.write_all(b"content"))
        .expect_err("a directory cannot be replaced");
    assert!(
        matches!(failure, atomic::Failure::Io(_)),
        "a rename that cannot happen is the writer's own failure"
    );
    assert!(blocked.is_dir(), "the directory was replaced by a file");
    assert_eq!(
        std::fs::read_dir(&blocked).expect("read").count(),
        0,
        "something was written into the directory"
    );
    assert_eq!(
        temporary_files(&dir),
        Vec::<String>::new(),
        "the temporary file survived a failed rename"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
