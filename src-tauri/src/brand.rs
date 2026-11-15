//! Every brand-specific value comes from `brand/brand.json`, built into the program. Nothing
//! else in the code names the product, the suite, the catalog address or the catalog key.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub struct Brand {
    pub product_name: String,
    pub identifier: String,
    pub suite_name: String,
    pub accent: String,
    pub accent_dark: String,
    pub expected_signer: String,
    pub repository: String,
    pub catalog_url: String,
    pub catalog_file: String,
    pub catalog_public_key: String,
    pub policy_name: String,
    pub sibling_label: String,
    pub sibling_url: String,
}

pub static BRAND: LazyLock<Brand> = LazyLock::new(|| serde_json::from_str(include_str!("../../brand/brand.json")).expect("brand/brand.json is not valid"));

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Brand {
    pub fn repository_url(&self) -> String {
        format!("https://github.com/{}", self.repository)
    }
    pub fn signature_url(&self) -> String {
        format!("{}.sig", self.catalog_url)
    }
    /// `%LOCALAPPDATA%\<identifier>`: settings, the cached catalog and the download folder.
    pub fn data_dir(&self) -> PathBuf {
        #[cfg(debug_assertions)]
        if let Ok(d) = std::env::var("PRISM_HUB_DATA_DIR") {
            return PathBuf::from(d);
        }
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        base.join(&self.identifier)
    }
}
