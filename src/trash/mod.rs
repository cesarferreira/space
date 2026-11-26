//! The user's trashes: where they live, how big they are, and how to empty them.
//!
//! - macOS: `~/.Trash`, plus `/Volumes/<name>/.Trashes/<uid>` on other volumes.
//! - Linux and other freedesktop systems: `$XDG_DATA_HOME/Trash`
//!   (default `~/.local/share/Trash`), plus `<mount>/.Trash/<uid>` and
//!   `<mount>/.Trash-<uid>` on other mounts. Each has `files/` and `info/` folders.

use crate::{
    clean::{confirm, remove_path},
    model::Estimate,
    output::terminal::format_bytes,
    scan::{directory_size, merge},
};
use anyhow::{Result, bail};
use std::{
    fs, io,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// A flat directory of trashed items (macOS).
    Flat,
    /// The freedesktop.org layout: `files/`, `info/`, `expunged/`, `directorysizes`.
    FreeDesktop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trash {
    pub path: PathBuf,
    layout: Layout,
}

#[derive(Debug, Default)]
pub struct EmptyOutcome {
    pub removed: usize,
    pub failures: Vec<(PathBuf, io::Error)>,
}

impl Trash {
    /// The macOS home trash under `home`.
    pub fn macos(home: &Path) -> Self {
        Self {
            path: home.join(".Trash"),
            layout: Layout::Flat,
        }
    }

    /// The freedesktop.org home trash under `data_home` (usually `~/.local/share`).
    pub fn freedesktop(data_home: &Path) -> Self {
        Self {
            path: data_home.join("Trash"),
            layout: Layout::FreeDesktop,
        }
    }

    /// The current user's home trash, or `None` on unsupported platforms or without `$HOME`.
    pub fn detect() -> Option<Self> {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)?;
        if cfg!(target_os = "macos") {
            Some(Self::macos(&home))
        } else if cfg!(unix) {
            let data_home = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(".local/share"));
            Some(Self::freedesktop(&data_home))
        } else {
            None
        }
    }

    /// The home trash plus every existing per-volume trash on `mounts`.
    pub fn detect_all(mounts: &[PathBuf]) -> Vec<Self> {
        let mut trashes: Vec<Self> = Self::detect().into_iter().collect();
        if let Some(uid) = current_uid() {
            for mount in mounts {
                for trash in volume_trashes(mount, uid, cfg!(target_os = "macos")) {
                    if !trashes.iter().any(|t| t.path == trash.path) {
                        trashes.push(trash);
                    }
                }
            }
        }
        trashes
    }

    pub fn size(&self) -> Estimate {
        directory_size(&self.path)
    }

    /// The paths that emptying the trash removes. Each one is a trashed item or its metadata.
    fn entries(&self) -> io::Result<Vec<PathBuf>> {
        let dirs = match self.layout {
            Layout::Flat => vec![self.path.clone()],
            Layout::FreeDesktop => ["files", "info", "expunged"]
                .iter()
                .map(|d| self.path.join(d))
                .collect(),
        };
        let mut entries = vec![];
        for dir in dirs {
            match fs::read_dir(&dir) {
                Ok(rd) => {
                    for entry in rd {
                        entries.push(entry?.path());
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        if self.layout == Layout::FreeDesktop {
            let cache = self.path.join("directorysizes");
            if cache.exists() {
                entries.push(cache);
            }
        }
        Ok(entries)
    }

    /// The number of trashed items, not counting metadata or Finder's `.DS_Store`.
    pub fn item_count(&self) -> io::Result<usize> {
        let dir = match self.layout {
            Layout::Flat => self.path.clone(),
            Layout::FreeDesktop => self.path.join("files"),
        };
        match fs::read_dir(&dir) {
            Ok(rd) => Ok(rd
                .filter_map(Result::ok)
                .filter(|e| e.file_name() != ".DS_Store")
                .count()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// Permanently delete everything in the trash. The trash directory itself is kept.
    pub fn empty(&self) -> io::Result<EmptyOutcome> {
        let mut outcome = EmptyOutcome::default();
        for path in self.entries()? {
            match remove_path(&path) {
                Ok(()) => outcome.removed += 1,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => outcome.failures.push((path, e)),
            }
        }
        Ok(outcome)
    }
}

#[cfg(unix)]
fn current_uid() -> Option<u32> {
    Some(unsafe { libc::getuid() })
}

#[cfg(not(unix))]
fn current_uid() -> Option<u32> {
    None
}

/// The existing trashes that user `uid` has on the volume mounted at `mount`.
pub fn volume_trashes(mount: &Path, uid: u32, macos: bool) -> Vec<Trash> {
    if macos {
        if !mount.starts_with("/Volumes") {
            return vec![];
        }
        let path = mount.join(".Trashes").join(uid.to_string());
        return if path.is_dir() {
            vec![Trash {
                path,
                layout: Layout::Flat,
            }]
        } else {
            vec![]
        };
    }
    let mut found = vec![];
    // `$topdir/.Trash` must be a real directory with the sticky bit (freedesktop spec).
    let shared = mount.join(".Trash");
    if is_sticky_dir(&shared) {
        let path = shared.join(uid.to_string());
        if path.is_dir() {
            found.push(Trash {
                path,
                layout: Layout::FreeDesktop,
            });
        }
    }
    let personal = mount.join(format!(".Trash-{uid}"));
    if personal.is_dir() {
        found.push(Trash {
            path: personal,
            layout: Layout::FreeDesktop,
        });
    }
    found
}

#[cfg(unix)]
fn is_sticky_dir(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && m.permissions().mode() & 0o1000 != 0)
}

#[cfg(not(unix))]
fn is_sticky_dir(_: &Path) -> bool {
    false
}

fn access_error(trash: &Trash, e: io::Error) -> anyhow::Error {
    let hint = if cfg!(target_os = "macos") && e.kind() == io::ErrorKind::PermissionDenied {
        "\nmacOS protects the Trash. Grant your terminal Full Disk Access in \
         System Settings → Privacy & Security, then try again."
    } else {
        ""
    };
    anyhow::anyhow!("cannot read {}: {e}{hint}", trash.path.display())
}

fn size_text(size: &Estimate) -> String {
    let bytes = format_bytes(size.bytes);
    if size.incomplete {
        format!("at least {bytes}")
    } else {
        bytes
    }
}

/// Show what is in the trashes, ask for confirmation, and empty them.
///
/// Without `assume_yes`, the user must answer `y` or `yes`. When `interactive` is false
/// (stdin is not a terminal) and `assume_yes` is not set, nothing is deleted.
pub fn confirm_and_empty(
    trashes: &[Trash],
    assume_yes: bool,
    interactive: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<()> {
    let mut full = vec![];
    for trash in trashes {
        let items = trash.item_count().map_err(|e| access_error(trash, e))?;
        let has_entries = !trash
            .entries()
            .map_err(|e| access_error(trash, e))?
            .is_empty();
        if items > 0 || has_entries {
            full.push((trash, items, trash.size()));
        }
    }
    if full.is_empty() {
        let paths: Vec<_> = trashes
            .iter()
            .map(|t| t.path.display().to_string())
            .collect();
        writeln!(out, "Trash is already empty ({}).", paths.join(", "))?;
        return Ok(());
    }
    let mut total = Estimate::exact(0);
    for (trash, items, size) in &full {
        let plural = if *items == 1 { "item" } else { "items" };
        writeln!(
            out,
            "Trash: {items} {plural}, {} ({})",
            size_text(size),
            trash.path.display()
        )?;
        total = merge(&total, size);
    }

    if !confirm(
        "Permanently delete everything in the trash?",
        assume_yes,
        interactive,
        input,
        out,
    )? {
        writeln!(out, "Aborted. Nothing was deleted.")?;
        return Ok(());
    }

    let mut failures = 0;
    let mut permission_denied = Vec::new();
    for (trash, _, _) in &full {
        let outcome = trash.empty().map_err(|e| access_error(trash, e))?;
        for (path, e) in &outcome.failures {
            writeln!(out, "  could not delete {}: {e}", path.display())?;
            if cfg!(target_os = "macos") && e.kind() == io::ErrorKind::PermissionDenied {
                permission_denied.push(path.clone());
            }
        }
        failures += outcome.failures.len();
    }
    if !permission_denied.is_empty() {
        if interactive {
            if confirm(
                "Retry the permission-denied items with sudo? Sudo may ask for your password.",
                false,
                interactive,
                input,
                out,
            )? {
                for path in permission_denied {
                    match Command::new("/usr/bin/sudo")
                        .args(["--", "/bin/rm", "-rf", "--"])
                        .arg(&path)
                        .status()
                    {
                        Ok(status) if status.success() => failures -= 1,
                        Ok(status) => {
                            writeln!(out, "  sudo retry failed for {}: {status}", path.display())?;
                            break;
                        }
                        Err(e) => {
                            writeln!(out, "  sudo retry failed for {}: {e}", path.display())?;
                            break;
                        }
                    }
                }
            }
        } else {
            writeln!(
                out,
                "  Run space --empty-trash in a terminal to retry with sudo."
            )?;
        }
    }
    if failures == 0 {
        writeln!(out, "Emptied the trash. Freed {}.", size_text(&total))?;
        return Ok(());
    }
    bail!("emptied the trash partially: {failures} failed to delete")
}
