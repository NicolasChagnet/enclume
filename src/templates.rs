use std::path::Path;

use snafu::prelude::*;

use crate::parsers::{ParsedData, RawHtml};

pub trait Templater {
    fn render(&self, data: ParsedData) -> Result<RawHtml, snafu::Whatever>;
}

#[derive(Debug, Clone)]
pub struct TeraTemplater {
    engine: tera::Tera,
}

impl TeraTemplater {
    pub fn new() -> Self {
        Self {
            engine: tera::Tera::default(),
        }
    }

    /// Register every template in `template_dir`, keyed by its path relative to it
    pub fn load_templates(&mut self, template_dir: &Path) -> Result<(), snafu::Whatever> {
        let templates = crate::glob::glob_files(&template_dir.join("**/*.html"))?;
        let files = templates
            .into_iter()
            .map(|file| {
                let name = file
                    .strip_prefix(template_dir)
                    .with_whatever_context(|_| {
                        format!(
                            "Template {} is not inside {}",
                            file.display(),
                            template_dir.display()
                        )
                    })?
                    .to_string_lossy()
                    .into_owned();
                Ok((file, Some(name)))
            })
            .collect::<Result<Vec<_>, snafu::Whatever>>()?;
        self.engine
            .add_template_files(files)
            .whatever_context("Could not register the templates")?;
        Ok(())
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
    fn templater(template: &str) -> TeraTemplater {
        let mut templater = TeraTemplater::new();
        templater
            .engine
            .add_raw_template("post.html", template)
            .unwrap();
        templater
    }

    #[test]
    fn keeps_json_value_types_in_the_context() {
        let mut vars = VarMap::new();
        vars.insert("views".to_string(), serde_json::json!(42));
        let data = ParsedData::new(site_path("post.html"), vars, serde_json::json!(null));

        let rendered = templater("{{vars.views + 1}}").render(data).unwrap();

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

        let rendered = templater("<h1>{{vars.title}}</h1>{{__content__}}")
            .render(data)
            .unwrap();

        // Variables are escaped, the parsed content is inserted as raw HTML
        assert_eq!(
            rendered.inner(),
            "<h1>Hello &lt;b&gt;world&lt;/b&gt;</h1><p>body</p>"
        );
    }
}
