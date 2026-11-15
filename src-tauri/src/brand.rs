// SPDX-License-Identifier: Apache-2.0
//! The brand: every value that makes this program one hub rather than the other (OpenStage Hub or Prism Hub), read from brand/brand.json when the program is built.
//! Nothing brand-specific is written anywhere else in the Rust code.

use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brand {
    pub product_name: String,
    pub short_name: String,
    pub identifier: String,
    pub repository: String,
    pub catalog: CatalogPlace,
    /// The public half of the hub key (32 bytes of Ed25519, in base64). Empty until Make Hub Key.bat has been run.
    pub catalog_public_key: String,
    pub expected_signer: String,
    pub sibling: Sibling,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CatalogPlace {
    pub tag: String,
    pub file: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Sibling {
    pub label: String,
    pub url: String,
}

pub const BRAND_JSON: &str = include_str!("../../brand/brand.json");

pub fn brand() -> &'static Brand {
    static BRAND: OnceLock<Brand> = OnceLock::new();
    BRAND.get_or_init(|| serde_json::from_str(BRAND_JSON).expect("brand/brand.json is read when the program is built"))
}

impl Brand {
    /// Where the catalog is published: an asset on the fixed release named by catalog.tag in the hub's repository.
    pub fn catalog_url(&self) -> String {
        format!("https://github.com/{}/releases/download/{}/{}", self.repository, self.catalog.tag, self.catalog.file)
    }

    pub fn signature_url(&self) -> String {
        format!("https://github.com/{}/releases/download/{}/{}", self.repository, self.catalog.tag, self.catalog.signature)
    }

    pub fn repository_url(&self) -> String {
        format!("https://github.com/{}", self.repository)
    }
}

/// This hub's own version (from Cargo.toml, which tools/build-installer.mjs keeps equal to the release).
pub const HUB_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brand_file_reads_and_points_at_the_hub_repository() {
        let b = brand();
        assert!(!b.product_name.is_empty());
        assert!(b.catalog_url().starts_with("https://github.com/"));
        assert!(b.catalog_url().ends_with(&b.catalog.file));
        assert!(b.sibling.url.starts_with("https://"));
    }
}
