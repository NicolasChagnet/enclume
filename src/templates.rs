use anyhow::Context;

use crate::{
    parsers::{RawHtml, VarMap},
    path::AbsPath,
};

pub const VAR_SLOT: &str = "slot";

pub trait Templater {
    fn render(&self, template_name: &str, values: VarMap) -> anyhow::Result<RawHtml>;
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

    /// Register templates with the engine, keyed by their file name
    pub fn load_templates<'a>(
        &mut self,
        templates: impl IntoIterator<Item = &'a Template>,
    ) -> anyhow::Result<()> {
        self.engine.add_template_files(
            templates
                .into_iter()
                .map(|t| (t.file.inner(), Some(t.name.as_str()))),
        )?;
        Ok(())
    }
}

impl Templater for TeraTemplater {
    fn render(&self, template_name: &str, values: VarMap) -> anyhow::Result<RawHtml> {
        let context = convert_values_into_context(values)?;
        let rendered = self.engine.render(template_name, &context)?;
        Ok(RawHtml::new(rendered))
    }
}

/// Convert the variable map into a Tera context
///
/// Values keep their JSON type so templates can read structured content. The
/// slot holds an HTML fragment parsed from the content file, so it bypasses
/// the escaping applied to every other variable.
fn convert_values_into_context(values: VarMap) -> anyhow::Result<tera::Context> {
    let mut context = tera::Context::new();
    for (key, value) in values {
        if key == VAR_SLOT {
            let html = value
                .as_str()
                .with_context(|| format!("{VAR_SLOT} must be a string"))?;
            context.insert_value(key, tera::Value::safe_string(html));
        } else {
            let value = tera::Value::try_from_serializable(&value)
                .with_context(|| format!("Could not convert variable {key:?} for the template"))?;
            context.insert_value(key, value);
        }
    }
    Ok(context)
}

#[derive(Debug, Clone)]
pub struct Template {
    name: String,
    file: AbsPath,
}

impl Template {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn file(&self) -> &AbsPath {
        &self.file
    }

    pub fn load(file: AbsPath) -> anyhow::Result<Self> {
        if !file.inner().is_file() {
            anyhow::bail!("Template file {file} does not exist or is not a file!")
        }
        // Register under the file name so tera autoescapes `*.html` templates
        let name = file
            .inner()
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Could not extract a template name from {file}"))?
            .to_string();
        Ok(Self { name, file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_json_value_types_in_the_context() {
        let mut values = VarMap::new();
        values.insert("views".to_string(), serde_json::json!(42));
        values.insert("tags".to_string(), serde_json::json!(["a", "b"]));
        values.insert(VAR_SLOT.to_string(), serde_json::json!("<p>body</p>"));

        let context = convert_values_into_context(values).unwrap();

        assert!(context.get("views").unwrap().is_number());
        assert!(context.get("tags").unwrap().is_array());
        let slot = context.get(VAR_SLOT).unwrap();
        assert!(slot.is_safe());
        assert_eq!(slot.as_str(), Some("<p>body</p>"));
    }

    #[test]
    fn rejects_a_non_string_slot() {
        let mut values = VarMap::new();
        values.insert(VAR_SLOT.to_string(), serde_json::json!(42));

        assert!(convert_values_into_context(values).is_err());
    }
}
