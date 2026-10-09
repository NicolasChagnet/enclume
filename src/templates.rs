use std::{path::Path, sync::Arc};

use snafu::prelude::*;

use crate::{
    glob::glob_relative,
    output::Assets,
    parsers::{ParsedData, RawHtml},
    path::SitePath,
};

pub trait Templater {
    fn render(&self, data: ParsedData) -> Result<RawHtml, snafu::Whatever>;
}

#[derive(Debug, Clone)]
pub struct TeraTemplater {
    engine: tera::Tera,
}

impl TeraTemplater {
    /// `assets` backs the [`asset()`] function which templates link to files with
    ///
    /// [`asset()`]: https://keats.github.io/tera/docs/#asset
    pub fn new(assets: Arc<Assets>) -> Self {
        let mut engine = tera::Tera::default();
        engine.register_function("asset", asset_function(assets));
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

impl Templater for TeraTemplater {
    fn render(&self, data: ParsedData) -> Result<RawHtml, snafu::Whatever> {
        let template_name = data.template().as_str();
        let values = data.as_value()?;
        let mut context = tera::Context::from_serialize(&values)
            .whatever_context("Couldn't convert into template context")?;
        // The content holds an HTML fragment or structured data: never escape it
        if let Some(content) = values["__content__"].as_str() {
            context.insert_value("__content__", tera::Value::safe_string(content));
        }
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
        let mut templater = TeraTemplater::new(Arc::new(assets));
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
            .render(data)
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
            .render(data)
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
        .render(data())
        .unwrap();

        assert_eq!(
            rendered.inner(),
            r#"<link href="/assets/style.abc123.css">"#
        );
    }

    #[test]
    fn fails_on_an_asset_the_build_does_not_know() {
        let error = templater(r#"{{ asset(path="assets/typo.css") }}"#, Assets::default())
            .render(data())
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
            .render(data())
            .unwrap_err();
        let error = format!("{}", snafu::Report::from_error(error));

        assert!(
            error.contains("Not a site-relative path"),
            "unexpected error: {error}"
        );
    }
}
