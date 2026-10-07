use std::path::Path;

use anyhow::Context;

use crate::{
    parsers::{RawHtml, VAR_CONTENT, VarMap},
    path::SitePath,
};

pub trait Templater {
    fn render(&self, template: &SitePath, values: VarMap) -> anyhow::Result<RawHtml>;
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
    pub fn load_templates(&mut self, template_dir: &Path) -> anyhow::Result<()> {
        let templates = crate::glob::glob_files(&template_dir.join("**/*.html"))?;
        self.engine.add_template_files(
            templates
                .into_iter()
                .map(|file| {
                    let name = file
                        .strip_prefix(template_dir)
                        .with_context(|| {
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
                .collect::<anyhow::Result<Vec<_>>>()?,
        )?;
        Ok(())
    }
}

impl Templater for TeraTemplater {
    fn render(&self, template: &SitePath, values: VarMap) -> anyhow::Result<RawHtml> {
        let context = convert_values_into_context(values)?;
        let rendered = self.engine.render(&template.to_string(), &context)?;
        Ok(RawHtml::new(rendered))
    }
}

/// Convert the variable map into a Tera context
///
/// Values keep their JSON type so templates can read structured content. The
/// content of a Markdown file is an HTML fragment, so it bypasses the escaping
/// applied to every other string.
fn convert_values_into_context(values: VarMap) -> anyhow::Result<tera::Context> {
    let mut context = tera::Context::new();
    for (key, value) in values {
        let value = match (key.as_str(), value) {
            (VAR_CONTENT, serde_json::Value::String(html)) => tera::Value::safe_string(&html),
            (_, value) => tera::Value::try_from_serializable(&value)
                .with_context(|| format!("Could not convert variable {key:?} for the template"))?,
        };
        context.insert_value(key, value);
    }
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_json_value_types_in_the_context() {
        let mut values = VarMap::new();
        values.insert("views".to_string(), serde_json::json!(42));
        values.insert("tags".to_string(), serde_json::json!(["a", "b"]));
        values.insert(VAR_CONTENT.to_string(), serde_json::json!({"items": ["a"]}));

        let context = convert_values_into_context(values).unwrap();

        assert!(context.get("views").unwrap().is_number());
        assert!(context.get("tags").unwrap().is_array());
        assert!(context.get(VAR_CONTENT).unwrap().is_map());
    }

    #[test]
    fn marks_parsed_markdown_as_safe_html() {
        let mut values = VarMap::new();
        values.insert(VAR_CONTENT.to_string(), serde_json::json!("<p>body</p>"));

        let context = convert_values_into_context(values).unwrap();

        let content = context.get(VAR_CONTENT).unwrap();
        assert!(content.is_safe());
        assert_eq!(content.as_str(), Some("<p>body</p>"));
    }
}
