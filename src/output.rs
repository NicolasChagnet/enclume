use snafu::prelude::*;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::path::SitePath;

/// Extensions which get a content hash in their built name
const FINGERPRINTED_EXTENSIONS: [&str; 2] = ["css", "js"];

/// Rules which place source files inside the output tree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputLayout {
    clean_urls: bool,
    fingerprint_assets: bool,
}

impl Default for OutputLayout {
    fn default() -> Self {
        Self {
            clean_urls: true,
            fingerprint_assets: false,
        }
    }
}

impl OutputLayout {
    pub fn new(clean_urls: bool, fingerprint_assets: bool) -> Self {
        Self {
            clean_urls,
            fingerprint_assets,
        }
    }

    /// Whether a copied file carries its content hash in its name
    pub fn fingerprints(&self, file: &SitePath) -> bool {
        self.fingerprint_assets
            && file
                .extension()
                .is_some_and(|extension| FINGERPRINTED_EXTENSIONS.contains(&extension))
    }

    /// The destination of a file the build rendered into HTML
    ///
    /// `index` and `404` keep their name: the dev server and static hosts look
    /// them up under that name.
    pub fn rendered_page(&self, file: &SitePath) -> Result<SitePath, snafu::Whatever> {
        match file.file_stem() {
            "index" | "404" => Ok(file.clone().with_extension("html")),
            _ if self.clean_urls => file.clone().without_extension().join("index.html"),
            _ => Ok(file.clone().with_extension("html")),
        }
    }

    /// The destination of a file the build copied verbatim
    pub fn copied_file(&self, file: &SitePath, digest: Option<&str>) -> SitePath {
        match digest {
            Some(digest) => file.clone().with_hashed_name(digest),
            None => file.clone(),
        }
    }

    /// The URL a rendered page is served at
    ///
    /// An `index` page is served at its directory without a trailing slash, so
    /// `blog/index.md` becomes `/blog` and the root `index.md` becomes `/`. Every
    /// other page uses its built name: `blog/post.md` → `/blog/post` with clean
    /// URLs, `/about.html` without them, and `/404.html` either way.
    pub fn page_url(&self, file: &SitePath) -> Result<String, snafu::Whatever> {
        let page = self.rendered_page(file)?;
        if page.file_stem() != "index" {
            return Ok(format!("/{page}"));
        }
        match page.as_path().parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                Ok(format!("/{}", parent.to_string_lossy()))
            }
            _ => Ok("/".to_string()),
        }
    }
}

/// The built name of every content file, keyed by its source path
///
/// The build fills this before rendering. Templates read it through the `asset()`
/// function and the build reads it to copy files, so a link and the file on disk
/// always agree.
#[derive(Debug, Clone, Default)]
pub struct Assets {
    files: HashMap<SitePath, SitePath>,
}

impl Assets {
    /// Record where `source` lands
    pub fn insert(&mut self, source: SitePath, destination: SitePath) {
        self.files.insert(source, destination);
    }

    /// The built name of `source`
    ///
    /// A file which the build never recorded keeps its own name.
    pub fn name(&self, source: &SitePath) -> SitePath {
        self.files
            .get(source)
            .cloned()
            .unwrap_or_else(|| source.clone())
    }

    /// Whether the build knows `source`
    pub fn contains(&self, source: &SitePath) -> bool {
        self.files.contains_key(source)
    }

    /// The built location of `source` as a root-relative URL
    pub fn url(&self, source: &SitePath) -> String {
        format!("/{}", self.name(source))
    }
}

/// The tree a build writes into
#[derive(Debug)]
pub struct Output<'a> {
    root: &'a Path,
    layout: &'a OutputLayout,
    assets: &'a Assets,
}

impl<'a> Output<'a> {
    pub fn new(root: &'a Path, layout: &'a OutputLayout, assets: &'a Assets) -> Self {
        Self {
            root,
            layout,
            assets,
        }
    }

    /// The destination of a file the build copies verbatim
    pub fn asset(&self, file: &SitePath) -> PathBuf {
        self.assets.name(file).resolve(self.root)
    }

    /// The destination of a page the build rendered
    pub fn page(&self, file: &SitePath) -> Result<PathBuf, snafu::Whatever> {
        Ok(self.layout.rendered_page(file)?.resolve(self.root))
    }

    /// Copy `source` to `destination`, creating the parent directories
    pub fn copy(&self, source: &Path, destination: &Path) -> Result<(), snafu::Whatever> {
        create_parents(destination)?;
        std::fs::copy(source, destination).whatever_context(format!(
            "Couldn't copy {} to {}",
            source.display(),
            destination.display()
        ))?;
        Ok(())
    }

    /// Write `bytes` at `destination`, creating the parent directories
    pub fn write(&self, destination: &Path, bytes: &[u8]) -> Result<(), snafu::Whatever> {
        create_parents(destination)?;
        std::fs::write(destination, bytes)
            .whatever_context(format!("Couldn't write {}", destination.display()))?;
        Ok(())
    }
}

/// Create the parent directories of a path which is about to be written
fn create_parents(path: &Path) -> Result<(), snafu::Whatever> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).whatever_context(format!(
            "Couldn't create parent directory {}",
            parent.display()
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site_path(path: &str) -> SitePath {
        SitePath::try_from(PathBuf::from(path)).unwrap()
    }

    #[test]
    fn renders_pages_into_directories_with_clean_urls() {
        let layout = OutputLayout::default();

        assert_eq!(
            layout
                .rendered_page(&site_path("about.md"))
                .unwrap()
                .as_str(),
            "about/index.html"
        );
        assert_eq!(
            layout
                .rendered_page(&site_path("blog/post.md"))
                .unwrap()
                .as_str(),
            "blog/post/index.html"
        );
    }

    #[test]
    fn maps_pages_to_the_url_they_are_served_at() {
        let clean = OutputLayout::default();
        assert_eq!(clean.page_url(&site_path("index.md")).unwrap(), "/");
        assert_eq!(
            clean.page_url(&site_path("blog/index.md")).unwrap(),
            "/blog"
        );
        assert_eq!(clean.page_url(&site_path("about.md")).unwrap(), "/about");
        assert_eq!(
            clean.page_url(&site_path("blog/post.md")).unwrap(),
            "/blog/post"
        );
        // `404` keeps its name under both layouts
        assert_eq!(clean.page_url(&site_path("404.md")).unwrap(), "/404.html");

        let flat = OutputLayout::new(false, false);
        assert_eq!(
            flat.page_url(&site_path("about.md")).unwrap(),
            "/about.html"
        );
        assert_eq!(flat.page_url(&site_path("blog/index.md")).unwrap(), "/blog");
        assert_eq!(flat.page_url(&site_path("404.md")).unwrap(), "/404.html");
    }

    #[test]
    fn keeps_the_name_of_index_and_404_pages() {
        let layout = OutputLayout::default();

        assert_eq!(
            layout
                .rendered_page(&site_path("index.md"))
                .unwrap()
                .as_str(),
            "index.html"
        );
        assert_eq!(
            layout
                .rendered_page(&site_path("blog/index.md"))
                .unwrap()
                .as_str(),
            "blog/index.html"
        );
        assert_eq!(
            layout.rendered_page(&site_path("404.md")).unwrap().as_str(),
            "404.html"
        );
    }

    #[test]
    fn renders_pages_next_to_their_source_without_clean_urls() {
        let layout = OutputLayout::new(false, false);

        assert_eq!(
            layout
                .rendered_page(&site_path("about.md"))
                .unwrap()
                .as_str(),
            "about.html"
        );
        assert_eq!(
            layout
                .rendered_page(&site_path("blog/post.md"))
                .unwrap()
                .as_str(),
            "blog/post.html"
        );
        assert_eq!(
            layout.rendered_page(&site_path("404.md")).unwrap().as_str(),
            "404.html"
        );
    }

    #[test]
    fn fingerprints_css_and_js_when_asked() {
        assert!(!OutputLayout::default().fingerprints(&site_path("assets/style.css")));

        let layout = OutputLayout::new(true, true);
        assert!(layout.fingerprints(&site_path("assets/style.css")));
        assert!(layout.fingerprints(&site_path("assets/app.js")));
        assert!(!layout.fingerprints(&site_path("assets/logo.png")));
    }

    #[test]
    fn copies_files_under_their_own_name_or_their_hash() {
        let layout = OutputLayout::default();

        assert_eq!(
            layout
                .copied_file(&site_path("img/logo.png"), None)
                .as_str(),
            "img/logo.png"
        );
        assert_eq!(
            layout
                .copied_file(&site_path("assets/style.css"), Some("abc123"))
                .as_str(),
            "assets/style.abc123.css"
        );
    }

    #[test]
    fn resolves_assets_to_their_built_name() {
        let mut assets = Assets::default();
        assets.insert(
            site_path("assets/style.css"),
            site_path("assets/style.abc123.css"),
        );
        assets.insert(site_path("img/logo.png"), site_path("img/logo.png"));

        assert!(assets.contains(&site_path("img/logo.png")));
        assert!(!assets.contains(&site_path("img/typo.png")));
        assert_eq!(
            assets.name(&site_path("assets/style.css")).as_str(),
            "assets/style.abc123.css"
        );
        assert_eq!(
            assets.name(&site_path("img/typo.png")).as_str(),
            "img/typo.png"
        );
        assert_eq!(
            assets.url(&site_path("assets/style.css")),
            "/assets/style.abc123.css"
        );
    }

    #[test]
    fn writes_files_below_the_output_root() {
        let dir = tempfile::tempdir().unwrap();
        let layout = OutputLayout::default();
        let assets = Assets::default();
        let output = Output::new(dir.path(), &layout, &assets);

        let destination = output.page(&site_path("blog/post.md")).unwrap();
        output.write(&destination, b"<p>body</p>").unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("blog/post/index.html")).unwrap(),
            "<p>body</p>"
        );
        assert_eq!(
            output.asset(&site_path("assets/style.css")),
            dir.path().join("assets/style.css")
        );
    }
}
