//! The files of the project structure `enclume init` writes.
//!
//! The files live beside this crate, under `init/`, and are read into the
//! binary while it is built, so that a binary installed from a package writes
//! the same project as one built from a checkout.

use std::path::Path;

use include_dir::{Dir, include_dir};
use snafu::prelude::*;

/// The files of a project, under the base directory of a site
static PROJECT: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/init");

/// Initializes a project structure under `base`
///
/// A base directory which is already there is left alone, so that a project is
/// never written over.
pub fn project(base: &Path) -> Result<(), snafu::Whatever> {
    if base.exists() {
        log::warn!(
            "Base directory {} already exists!",
            base.to_str().unwrap_or_default()
        );
        return Ok(());
    }

    PROJECT
        .extract(base)
        .whatever_context(format!("Couldn't write a project under {}", base.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_files_of_a_project() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("src");

        project(&base).unwrap();

        let layout = std::fs::read_to_string(base.join("templates/layout.html")).unwrap();
        assert!(
            layout.contains(r#"{{ asset(path="lumis.css") }}"#),
            "the layout links the stylesheet of the code blocks: {layout}"
        );
        assert!(
            layout.contains(r#"{{ asset(path="lumis.js") }}"#),
            "the layout links the script of the code blocks: {layout}"
        );

        let stylesheet = std::fs::read_to_string(base.join("root/lumis.css")).unwrap();
        assert!(
            stylesheet.contains("pre.lumis > code"),
            "the stylesheet lays a block out: {stylesheet}"
        );
        assert!(
            stylesheet.contains("figcaption.lumis-title") && stylesheet.contains(".l-highlighted"),
            "the stylesheet draws the title bar of a block and its marked lines: {stylesheet}"
        );

        let script = std::fs::read_to_string(base.join("root/lumis.js")).unwrap();
        assert!(
            script.contains("data-copy-snippet"),
            "the script copies a block: {script}"
        );

        assert!(base.join("root/index.md").is_file());
        assert!(base.join("root/404.md").is_file());
    }

    #[test]
    fn leaves_a_project_which_is_already_there_alone() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("src");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("index.md"), "mine").unwrap();

        project(&base).unwrap();

        assert_eq!(
            std::fs::read_to_string(base.join("index.md")).unwrap(),
            "mine"
        );
        assert!(!base.join("templates").exists());
    }
}
