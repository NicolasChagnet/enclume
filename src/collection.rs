use crate::{
    glob::{glob_files, match_files},
    path::{AbsPath, COLLECTIONS_DIR, CONTENT_DIR, TEMPLATES_DIR},
    templates::{self, Template},
};
use log::info;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionFile {
    /// Content file patterns, relative to `<base_dir>/root/`
    pub pattern: String,
    /// Template filename, relative to `<base_dir>/templates/`
    pub template: String,
    /// Variables the collection expects in the frontmatter of each file
    pub variables: Vec<String>,
}

impl CollectionFile {
    pub fn parse_file(file: &AbsPath) -> anyhow::Result<Self> {
        let content = std::fs::read(file.inner())?;
        let parsed_content: Self = toml::from_slice(&content)?;
        Ok(parsed_content)
    }
}

#[derive(Debug, Clone)]
pub struct Collection {
    name: String,
    files: Vec<AbsPath>,
    template: templates::Template,
}

impl Collection {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn files(&self) -> &[AbsPath] {
        &self.files
    }

    pub fn template(&self) -> &templates::Template {
        &self.template
    }

    /// Load a collection and select the content files matching its pattern
    pub fn try_load_file(
        base_dir: &AbsPath,
        collection_path: &AbsPath,
        content_files: &[AbsPath],
    ) -> anyhow::Result<Self> {
        if !collection_path.inner().is_file()
            || collection_path.inner().extension() != Some(OsStr::new("toml"))
        {
            anyhow::bail!("Invalid collection file: {collection_path}")
        }

        let collection_raw = CollectionFile::parse_file(collection_path)
            .map_err(|e| e.context(format!("Could not load collection {collection_path}")))?;

        let content_dir: AbsPath = base_dir.inner().join(CONTENT_DIR).try_into()?;
        let files = match_files(content_dir.inner(), &collection_raw.pattern, content_files)?;

        let template_path: AbsPath = base_dir
            .inner()
            .join(TEMPLATES_DIR)
            .join(&collection_raw.template)
            .with_extension("html")
            .try_into()?;
        let template = Template::load(template_path)?;

        let name = if let Some(name) = collection_path
            .inner()
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
        {
            name
        } else {
            anyhow::bail!("Could not extract filename for {collection_path}")
        };

        Ok(Self {
            name,
            files,
            template,
        })
    }

    /// Load every collection declared in `<base_dir>/collections/`
    pub fn load_all(base_dir: &AbsPath, content_files: &[AbsPath]) -> anyhow::Result<Vec<Self>> {
        let collection_pattern = base_dir.inner().join(COLLECTIONS_DIR).join("*.toml");
        info!("Loading all collections {}", collection_pattern.display());
        glob_files(&collection_pattern)?
            .iter()
            .map(|file| Collection::try_load_file(base_dir, file, content_files))
            .collect()
    }
}

impl std::fmt::Display for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
