use std::path::Path;

use crate::{
    glob::{glob_files, match_files},
    path::{Roots, SitePath},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionFile {
    /// Content file patterns, relative to `<base>/root/`
    pub pattern: String,
    /// Template filename, relative to `<base>/templates/`
    pub template: String,
    /// Variables the collection expects in the frontmatter of each file
    pub variables: Vec<String>,
}

impl CollectionFile {
    pub fn parse_file(file: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read(file)?;
        let parsed_content: Self = toml::from_slice(&content)?;
        Ok(parsed_content)
    }
}

#[derive(Debug, Clone)]
pub struct Collection {
    name: String,
    files: Vec<SitePath>,
    template: String,
}

impl Collection {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn files(&self) -> &[SitePath] {
        &self.files
    }

    pub fn template(&self) -> &str {
        &self.template
    }

    /// Load a collection and select the content files matching its pattern
    pub fn try_load_file(
        collection_path: &Path,
        content_files: &[SitePath],
    ) -> anyhow::Result<Self> {
        let collection_raw = CollectionFile::parse_file(collection_path).map_err(|e| {
            e.context(format!(
                "Could not load collection {}",
                collection_path.display()
            ))
        })?;

        let files = match_files(&collection_raw.pattern, content_files)?;

        let template = collection_raw.template;

        let name = collection_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not extract filename for {}",
                    collection_path.display()
                )
            })?;

        Ok(Self {
            name,
            files,
            template,
        })
    }

    /// Load every collection declared in `<base>/collections/`
    pub fn load_all(roots: &Roots, content_files: &[SitePath]) -> anyhow::Result<Vec<Self>> {
        let collection_pattern = roots.collections_dir().join("*.toml");
        glob_files(&collection_pattern)?
            .iter()
            .map(|file| Collection::try_load_file(file, content_files))
            .collect()
    }
}

impl std::fmt::Display for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
