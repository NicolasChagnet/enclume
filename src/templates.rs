use crate::{
    parsers::{RawHtml, StringMap},
    path::AbsPath,
};

pub const VAR_SLOT: &str = "slot";

pub trait Templater {
    fn render(&self, template_name: &str, values: &StringMap) -> anyhow::Result<RawHtml>;
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
    fn render(&self, template_name: &str, values: &StringMap) -> anyhow::Result<RawHtml> {
        let context = convert_values_into_context(values);
        let rendered = self.engine.render(template_name, &context)?;
        Ok(RawHtml::new(rendered))
    }
}

/// Convert the HashMap of values into the Context wrapper
fn convert_values_into_context(values: &StringMap) -> tera::Context {
    let mut context = tera::Context::new();
    for (key, value) in values {
        if key == VAR_SLOT {
            // The slot holds an HTML fragment parsed from the content file, so
            // it must bypass the escaping applied to every other variable
            context.insert_value(key.clone(), tera::Value::safe_string(value));
        } else {
            context.insert(key.clone(), value);
        }
    }
    context
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
