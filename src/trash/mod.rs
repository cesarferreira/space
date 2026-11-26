//! The current user's trash: where it lives, how big it is, and how to empty it.
//!
//! - macOS: `~/.Trash`
//! - Linux and other freedesktop systems: `$XDG_DATA_HOME/Trash`
//!   (default `~/.local/share/Trash`), with `files/` and `info/` subdirectories.
//!
//! Per-volume trashes (`/Volumes/X/.Trashes`, `/mnt/x/.Trash-$UID`) are not handled.

use crate::{model::Estimate, output::terminal::format_bytes, scan::directory_size};
use anyhow::{Result, bail};
use std::{
    fs, io,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// A flat directory of trashed items (macOS `~/.Trash`).
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
            let result = match fs::symlink_metadata(&path) {
                Ok(m) if m.is_dir() => fs::remove_dir_all(&path),
                Ok(_) => fs::remove_file(&path),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => outcome.removed += 1,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => outcome.failures.push((path, e)),
            }
        }
        Ok(outcome)
    }
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

/// Show what is in the trash, ask for confirmation, and empty it.
///
/// Without `assume_yes`, the user must answer `y` or `yes`. When `interactive` is false
/// (stdin is not a terminal) and `assume_yes` is not set, nothing is deleted.
pub fn confirm_and_empty(
    trash: &Trash,
    assume_yes: bool,
    interactive: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<()> {
    let items = trash.item_count().map_err(|e| access_error(trash, e))?;
    if items == 0
        && trash
            .entries()
            .map_err(|e| access_error(trash, e))?
            .is_empty()
    {
        writeln!(out, "Trash is already empty ({}).", trash.path.display())?;
        return Ok(());
    }
    let size = trash.size();
    let plural = if items == 1 { "item" } else { "items" };
    let bytes = format_bytes(size.bytes);
    let bytes = if size.incomplete {
        format!("at least {bytes}")
    } else {
        bytes
    };
    writeln!(
        out,
        "Trash: {items} {plural}, {bytes} ({})",
        trash.path.display()
    )?;

    if !assume_yes {
        if !interactive {
            bail!(
                "refusing to empty the trash without confirmation; pass --yes to skip the prompt"
            );
        }
        write!(out, "Permanently delete everything in the trash? [y/N] ")?;
        out.flush()?;
        let mut answer = String::new();
        input.read_line(&mut answer)?;
        if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
            writeln!(out, "Aborted. Nothing was deleted.")?;
            return Ok(());
        }
    }

    let outcome = trash.empty().map_err(|e| access_error(trash, e))?;
    if outcome.failures.is_empty() {
        writeln!(out, "Emptied the trash. Freed {bytes}.")?;
        return Ok(());
    }
    for (path, e) in &outcome.failures {
        writeln!(out, "  could not delete {}: {e}", path.display())?;
    }
    bail!(
        "emptied the trash partially: {} failed to delete",
        outcome.failures.len()
    )
}
