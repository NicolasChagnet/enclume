use std::{path::Path, sync::Arc};

use snafu::prelude::*;

use crate::{
    glob::glob_relative,
    output::Assets,
    pages::Pages,
    parsers::{ParsedData, RawHtml},
    path::SitePath,
};

pub trait Templater {
    /// Render `data` for the page served at `url`
    fn render(&self, data: ParsedData, url: &str) -> Result<RawHtml, snafu::Whatever>;
}

#[derive(Debug, Clone)]
pub struct TeraTemplater {
    engine: tera::Tera,
}

impl TeraTemplater {
    /// `assets` backs the [`asset()`] function which templates link to files with
    ///
    /// `pages` backs [`pages()`], which lists the other pages of the build.
    ///
    /// [`asset()`]: https://keats.github.io/tera/docs/#asset
    pub fn new(assets: Arc<Assets>, pages: Arc<Pages>) -> Self {
        let mut engine = tera::Tera::default();
        engine.register_function("asset", asset_function(assets));
        engine.register_function("pages", pages_function(pages));
        Self { engine }
    }

    /// Register every template in `template_dir`, keyed by its path relative to it
    pub fn load_templates(&mut self, template_dir: &Path) -> Result<(), snafu::Whatever> {
        let templates = glob_relative(template_dir, "**/*.html")?;
        let files = templates
            .into_iter()
            .map(|file| {
                let name = file.as_str().to_string();
                (file.resolve(template_dir), Some(name))
            })
            .collect::<Vec<_>>();
        self.engine
            .add_template_files(files)
            .whatever_context("Could not register the templates")?;
        Ok(())
    }
}

/// Build the `asset()` template function, which resolves a source path to its built URL
///
/// `{{ asset(path="assets/style.css") }}` renders as `/assets/style.<digest>.css` when
/// the build fingerprints the file. A path which is not a content file fails the build,
/// so a typo stops it instead of producing a dead link.
fn asset_function(
    assets: Arc<Assets>,
) -> impl Fn(tera::Kwargs, &tera::State) -> tera::TeraResult<String> + Send + Sync + 'static {
    move |kwargs, _| {
        let path = kwargs.must_get::<&str>("path")?;
        let source = SitePath::try_from(std::path::PathBuf::from(path))
            .map_err(|error| tera::Error::message(format!("asset(): {error}")))?;
        if !assets.contains(&source) {
            return Err(tera::Error::message(format!(
                "asset(): no content file at `{path}`"
            )));
        }
        Ok(assets.url(&source))
    }
}

/// Build the `pages()` template function, which lists the other pages of the build
///
/// `{% for post in pages(pattern="blog/**/*.md") %}{{ post.url }}{% endfor %}` yields
/// one `{path, url, vars}` entry per matching page, in source-path order. Templates
/// can then build an index, a feed or a sitemap without a second source of truth.
/// An invalid pattern fails the build instead of rendering an empty list.
fn pages_function(
    pages: Arc<Pages>,
) -> impl Fn(tera::Kwargs, &tera::State) -> tera::TeraResult<tera::Value> + Send + Sync + 'static {
    move |kwargs, _| {
        let pattern = kwargs.get::<&str>("pattern")?.unwrap_or("**/*");
        let value = pages
            .as_value(pattern)
            .map_err(|error| tera::Error::message(format!("pages(): {error}")))?;
        tera::Value::try_from_serializable(&value)
            .map_err(|error| tera::Error::message(format!("pages(): {error}")))
    }
}

impl Templater for TeraTemplater {
    fn render(&self, data: ParsedData, url: &str) -> Result<RawHtml, snafu::Whatever> {
        let template_name = data.template().as_str();
        let values = data.as_value()?;
        let mut context = tera::Context::from_serialize(&values)
            .whatever_context("Couldn't convert into template context")?;
        // The content holds an HTML fragment or structured data: never escape it
        if let Some(content) = values["__content__"].as_str() {
            context.insert_value("__content__", tera::Value::safe_string(content));
        }
        // The URL the page itself is served at, for canonical links and for a
        // listing which must skip its own entry
        context.insert_value("__url__", url.to_string().into());
        let rendered = self
            .engine
            .render(template_name, &context)
            .with_whatever_context(|_| format!("Could not render {template_name}"))?;
        Ok(RawHtml::new(rendered))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parsers::VarMap, path::SitePath};
    use std::path::PathBuf;

    fn site_path(path: &str) -> SitePath {
        SitePath::try_from(PathBuf::from(path)).unwrap()
    }

    /// Templater which knows the `post.html` template
    fn templater(template: &str, assets: Assets) -> TeraTemplater {
        let mut templater = TeraTemplater::new(Arc::new(assets), Arc::new(Pages::default()));
        templater
            .engine
            .add_raw_template("post.html", template)
            .unwrap();
        templater
    }

    /// Empty metadata which renders through the `post.html` template
    fn data() -> ParsedData {
        ParsedData::new(
            site_path("post.html"),
            VarMap::new(),
            serde_json::json!(null),
        )
    }

    #[test]
    fn keeps_json_value_types_in_the_context() {
        let mut vars = VarMap::new();
        vars.insert("views".to_string(), serde_json::json!(42));
        let data = ParsedData::new(site_path("post.html"), vars, serde_json::json!(null));

        let rendered = templater("{{vars.views + 1}}", Assets::default())
            .render(data, "/blog/post")
            .unwrap();

        assert_eq!(rendered.inner(), "43");
    }

    #[test]
    fn marks_parsed_content_as_safe_html() {
        let mut vars = VarMap::new();
        vars.insert("title".to_string(), serde_json::json!("Hello <b>world</b>"));
        let data = ParsedData::new(
            site_path("post.html"),
            vars,
            serde_json::json!("<p>body</p>"),
        );

        let rendered = templater("<h1>{{vars.title}}</h1>{{__content__}}", Assets::default())
            .render(data, "/blog/post")
            .unwrap();

        // Variables are escaped, the parsed content is inserted as raw HTML
        assert_eq!(
            rendered.inner(),
            "<h1>Hello &lt;b&gt;world&lt;/b&gt;</h1><p>body</p>"
        );
    }

    #[test]
    fn resolves_asset_paths_to_their_built_url() {
        let mut assets = Assets::default();
        assets.insert(
            site_path("assets/style.css"),
            site_path("assets/style.abc123.css"),
        );

        let rendered = templater(
            r#"<link href="{{ asset(path="assets/style.css") }}">"#,
            assets,
        )
        .render(data(), "/blog/post")
        .unwrap();

        assert_eq!(
            rendered.inner(),
            r#"<link href="/assets/style.abc123.css">"#
        );
    }

    #[test]
    fn fails_on_an_asset_the_build_does_not_know() {
        let error = templater(r#"{{ asset(path="assets/typo.css") }}"#, Assets::default())
            .render(data(), "/blog/post")
            .unwrap_err();
        let error = format!("{}", snafu::Report::from_error(error));

        assert!(
            error.contains("no content file at `assets/typo.css`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn fails_on_an_asset_path_which_escapes_the_root() {
        let error = templater(r#"{{ asset(path="../secret.css") }}"#, Assets::default())
            .render(data(), "/blog/post")
            .unwrap_err();
        let error = format!("{}", snafu::Report::from_error(error));

        assert!(
            error.contains("Not a site-relative path"),
            "unexpected error: {error}"
        );
    }
}
