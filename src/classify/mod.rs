use crate::model::{Confidence, Reclaimability};
use std::path::PathBuf;
pub struct Rule {
    pub path: PathBuf,
    pub category: &'static str,
    pub name: &'static str,
    pub reclaimability: Reclaimability,
    pub confidence: Confidence,
}
pub fn known_rules() -> Vec<Rule> {
    let h = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    [
        ("Downloads", "Downloads"),
        ("Documents", "Documents"),
        ("Photos", "Pictures"),
        ("Rust", ".cargo"),
        ("Gradle", ".gradle"),
        ("npm", ".npm"),
        ("Android", "Android/Sdk"),
        ("Nix", ".nix-profile"),
        ("Homebrew", ".linuxbrew"),
    ]
    .into_iter()
    .map(|(n, p)| Rule {
        path: h.join(p),
        category: if matches!(n, "Downloads" | "Documents" | "Photos") {
            "Personal"
        } else {
            "Developer"
        },
        name: n,
        reclaimability: if matches!(n, "Downloads" | "Documents" | "Photos") {
            Reclaimability::UserOwned
        } else {
            Reclaimability::Cache
        },
        confidence: Confidence::High,
    })
    .collect()
}
