//! Known folders: what they are, how safe they are to delete, and how to clean them.

use crate::{
    model::{Confidence, Reclaimability},
    trash::Trash,
};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

/// How `space --clean` frees a rule's space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clean {
    /// Not offered: the data is yours, required, or too costly to get back.
    Never,
    /// Delete everything inside the folder, except folders that belong to other rules.
    Contents,
    /// Delete the folder itself (per-project build output).
    Directory,
    /// Run the owning tool's own cleanup command.
    Command(&'static [&'static str]),
    /// Empty this trash.
    Trash(Trash),
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub path: PathBuf,
    pub category: &'static str,
    pub name: &'static str,
    pub reclaimability: Reclaimability,
    pub confidence: Confidence,
    /// What the folder holds.
    pub what: &'static str,
    /// Whether deleting it is safe, and what it costs.
    pub safety: &'static str,
    pub clean: Clean,
}

impl Rule {
    pub fn slug(&self) -> String {
        slug(self.name)
    }

    pub fn cleanable(&self) -> bool {
        self.clean != Clean::Never
    }

    /// The command a user runs to clean this rule.
    pub fn clean_hint(&self) -> Option<String> {
        match self.clean {
            Clean::Never => None,
            Clean::Trash(_) => Some("space --empty-trash".into()),
            _ => Some(format!("space --clean {}", self.slug())),
        }
    }
}

/// A lowercase, dash-separated name used on the command line: "Go modules" → "go-modules".
pub fn slug(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Os {
    Any,
    Linux,
    Mac,
}

impl Os {
    fn current(self) -> bool {
        match self {
            Os::Any => true,
            Os::Linux => cfg!(target_os = "linux"),
            Os::Mac => cfg!(target_os = "macos"),
        }
    }
}

struct Spec {
    name: &'static str,
    category: &'static str,
    os: Os,
    /// Relative to the home folder, or absolute when it starts with `/`.
    path: &'static str,
    reclaimability: Reclaimability,
    what: &'static str,
    safety: &'static str,
    clean: Clean,
}

#[allow(clippy::too_many_arguments)]
const fn spec(
    name: &'static str,
    category: &'static str,
    os: Os,
    path: &'static str,
    reclaimability: Reclaimability,
    what: &'static str,
    safety: &'static str,
    clean: Clean,
) -> Spec {
    Spec {
        name,
        category,
        os,
        path,
        reclaimability,
        what,
        safety,
        clean,
    }
}

use Reclaimability::{Cache, Generated, PossiblyRemovable, Required, UserOwned};

const PERSONAL_SAFETY: &str = "No. These are your own files. Review them yourself.";
const CACHE_SAFETY: &str = "Yes. The tool downloads or rebuilds what it needs again, so the next build or install is slower.";

#[rustfmt::skip]
const SPECS: &[Spec] = &[
    spec("Downloads", "Personal", Os::Any, "Downloads", UserOwned, "Files you downloaded.", PERSONAL_SAFETY, Clean::Never),
    spec("Documents", "Personal", Os::Any, "Documents", UserOwned, "Your documents.", PERSONAL_SAFETY, Clean::Never),
    spec("Photos", "Personal", Os::Any, "Pictures", UserOwned, "Your pictures and photo libraries.", PERSONAL_SAFETY, Clean::Never),

    spec("Cargo", "Developer", Os::Any, ".cargo/registry", Cache, "Crates downloaded by Cargo.", CACHE_SAFETY, Clean::Contents),
    spec("Rustup", "Developer", Os::Any, ".rustup", PossiblyRemovable, "Rust toolchains installed by rustup.", "Partly. Remove toolchains you no longer use with `rustup toolchain uninstall`.", Clean::Never),
    spec("Gradle", "Developer", Os::Any, ".gradle/caches", Cache, "Dependencies and build outputs shared by Gradle builds.", CACHE_SAFETY, Clean::Contents),
    spec("Maven", "Developer", Os::Any, ".m2/repository", Cache, "Dependencies downloaded by Maven.", CACHE_SAFETY, Clean::Contents),
    spec("npm", "Developer", Os::Any, ".npm/_cacache", Cache, "Packages cached by npm.", CACHE_SAFETY, Clean::Contents),
    spec("pnpm", "Developer", Os::Linux, ".local/share/pnpm/store", Cache, "The pnpm content-addressable package store.", "Partly. `pnpm store prune` removes packages that no project uses.", Clean::Command(&["pnpm", "store", "prune"])),
    spec("pnpm", "Developer", Os::Mac, "Library/pnpm/store", Cache, "The pnpm content-addressable package store.", "Partly. `pnpm store prune` removes packages that no project uses.", Clean::Command(&["pnpm", "store", "prune"])),
    spec("Yarn", "Developer", Os::Linux, ".cache/yarn", Cache, "Packages cached by Yarn.", CACHE_SAFETY, Clean::Contents),
    spec("Yarn", "Developer", Os::Mac, "Library/Caches/Yarn", Cache, "Packages cached by Yarn.", CACHE_SAFETY, Clean::Contents),
    spec("pip", "Developer", Os::Linux, ".cache/pip", Cache, "Python packages cached by pip.", CACHE_SAFETY, Clean::Contents),
    spec("pip", "Developer", Os::Mac, "Library/Caches/pip", Cache, "Python packages cached by pip.", CACHE_SAFETY, Clean::Contents),
    spec("uv", "Developer", Os::Any, ".cache/uv", Cache, "Python packages and interpreters cached by uv.", CACHE_SAFETY, Clean::Command(&["uv", "cache", "clean"])),
    spec("Go modules", "Developer", Os::Any, "go/pkg/mod", Cache, "Go module downloads.", CACHE_SAFETY, Clean::Command(&["go", "clean", "-modcache"])),
    spec("Go build", "Developer", Os::Linux, ".cache/go-build", Cache, "The Go build cache.", CACHE_SAFETY, Clean::Contents),
    spec("Go build", "Developer", Os::Mac, "Library/Caches/go-build", Cache, "The Go build cache.", CACHE_SAFETY, Clean::Contents),
    spec("Android", "Developer", Os::Linux, "Android/Sdk", PossiblyRemovable, "The Android SDK: platforms, build tools, and emulator images.", "Partly. Remove unused platforms and system images with Android Studio's SDK Manager.", Clean::Never),
    spec("Android", "Developer", Os::Mac, "Library/Android/sdk", PossiblyRemovable, "The Android SDK: platforms, build tools, and emulator images.", "Partly. Remove unused platforms and system images with Android Studio's SDK Manager.", Clean::Never),
    spec("DerivedData", "Developer", Os::Mac, "Library/Developer/Xcode/DerivedData", Generated, "Xcode build products and indexes.", "Yes. Xcode rebuilds them, so the next build is slower.", Clean::Contents),
    spec("iOS support", "Developer", Os::Mac, "Library/Developer/Xcode/iOS DeviceSupport", Cache, "Debug symbols Xcode copies from each connected device and iOS version.", "Yes. Xcode copies them again when you connect a device.", Clean::Contents),
    spec("Simulators", "Developer", Os::Mac, "Library/Developer/CoreSimulator", PossiblyRemovable, "iOS, watchOS, and tvOS simulator devices and runtimes.", "Partly. `xcrun simctl delete unavailable` removes devices for runtimes that are no longer installed.", Clean::Command(&["xcrun", "simctl", "delete", "unavailable"])),
    spec("Docker", "Developer", Os::Mac, "Library/Containers/com.docker.docker/Data", PossiblyRemovable, "The Docker Desktop virtual disk: images, containers, volumes, and build cache.", "Partly. `docker system prune --force` removes stopped containers, unused networks, dangling images, and build cache. Docker Desktop must be running.", Clean::Command(&["docker", "system", "prune", "--force"])),
    spec("Homebrew", "Developer", Os::Linux, ".linuxbrew", Required, "Software installed with Homebrew.", "No. Uninstall formulae with `brew uninstall` instead.", Clean::Never),
    spec("Homebrew", "Developer", Os::Mac, "/opt/homebrew", Required, "Software installed with Homebrew.", "No. Uninstall formulae with `brew uninstall` instead.", Clean::Never),
    spec("Brew cache", "Developer", Os::Linux, ".cache/Homebrew", Cache, "Downloads cached by Homebrew.", CACHE_SAFETY, Clean::Contents),
    spec("Brew cache", "Developer", Os::Mac, "Library/Caches/Homebrew", Cache, "Downloads cached by Homebrew.", CACHE_SAFETY, Clean::Contents),
    spec("Nix", "Developer", Os::Linux, ".nix-profile", Required, "Your Nix profile.", "No. Use `nix-collect-garbage` to remove unused store paths.", Clean::Never),

    spec("Ollama", "AI", Os::Any, ".ollama/models", PossiblyRemovable, "Models downloaded by Ollama.", "Partly. Remove models you no longer use with `ollama rm`. Models are large to download again.", Clean::Never),
    spec("Hugging Face", "AI", Os::Any, ".cache/huggingface", PossiblyRemovable, "Models and datasets downloaded from the Hugging Face Hub.", "Partly. Remove what you no longer use with `hf cache delete`. Models are large to download again.", Clean::Never),

    spec("Caches", "Caches", Os::Linux, ".cache", Cache, "Application caches (XDG cache folder), excluding the caches listed separately.", "Mostly. Applications rebuild their caches. Close running applications first.", Clean::Contents),
    spec("Caches", "Caches", Os::Mac, "Library/Caches", Cache, "Application caches, excluding the caches listed separately.", "Mostly. Applications rebuild their caches. Close running applications first.", Clean::Contents),
];

pub const NODE_MODULES: &str = "node_modules";
pub const RUST_TARGET: &str = "Rust target";

/// The static rules for `home`, for the current operating system.
pub fn static_rules(home: &Path) -> Vec<Rule> {
    SPECS
        .iter()
        .filter(|s| s.os.current())
        .map(|s| Rule {
            path: if s.path.starts_with('/') {
                PathBuf::from(s.path)
            } else {
                home.join(s.path)
            },
            category: s.category,
            name: s.name,
            reclaimability: s.reclaimability,
            confidence: Confidence::High,
            what: s.what,
            safety: s.safety,
            clean: s.clean.clone(),
        })
        .collect()
}

pub fn trash_rule(trash: Trash) -> Rule {
    Rule {
        path: trash.path.clone(),
        category: "Trash",
        name: "Trash",
        reclaimability: PossiblyRemovable,
        confidence: Confidence::High,
        what: "Files you moved to the trash.",
        safety: "Yes, if you no longer need them. Emptying the trash deletes them permanently.",
        clean: Clean::Trash(trash),
    }
}

pub fn project_rule(path: PathBuf, kind: crate::scan::ProjectKind) -> Rule {
    let (name, what, safety) = match kind {
        crate::scan::ProjectKind::NodeModules => (
            NODE_MODULES,
            "Packages installed into JavaScript projects.",
            "Yes. Run your package manager's install command in the project to get them back.",
        ),
        crate::scan::ProjectKind::RustTarget => (
            RUST_TARGET,
            "Build output of Rust projects (Cargo `target` folders).",
            "Yes. Cargo rebuilds it, so the next build is slower.",
        ),
    };
    Rule {
        path,
        category: "Projects",
        name,
        reclaimability: Generated,
        confidence: Confidence::High,
        what,
        safety,
        clean: Clean::Directory,
    }
}

/// Every rule for this machine: static rules, all trashes on `mounts`, and project folders.
///
/// The project search stops at `deadline`; the second value is true when it stopped early.
pub fn known_rules(mounts: &[PathBuf], deadline: Option<Instant>) -> (Vec<Rule>, bool) {
    let Some(home) = home_dir() else {
        return (vec![], false);
    };
    let mut rules = static_rules(&home);
    rules.extend(Trash::detect_all(mounts).into_iter().map(trash_rule));
    let mut roots = vec![home.clone()];
    if let Ok(cwd) = std::env::current_dir()
        && cwd.parent().is_some()
        && !cwd.starts_with(&home)
    {
        roots.push(cwd);
    }
    let skip: Vec<PathBuf> = rules
        .iter()
        .map(|r| r.path.clone())
        .chain(cfg!(target_os = "macos").then(|| home.join("Library")))
        .collect();
    let (projects, timed_out) = crate::scan::find_projects(&roots, &skip, deadline);
    rules.extend(
        projects
            .into_iter()
            .map(|(path, kind)| project_rule(path, kind)),
    );
    (rules, timed_out)
}
