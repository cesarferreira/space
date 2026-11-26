# space

**Disk usage explained in human terms.**

<p>
  <a href="https://github.com/cesarferreira/space/actions/workflows/rust-tests.yml">
  <img alt="Tests" src="https://img.shields.io/github/actions/workflow/status/cesarferreira/space/rust-tests.yml?branch=main&logo=github&color=007ACC&label=tests"></a>
  <a href="https://crates.io/crates/space-cli"><img alt="Crates.io" src="https://img.shields.io/crates/v/space-cli?color=blue"></a>
  <a href="https://crates.io/crates/space-cli"><img src="https://img.shields.io/crates/d/space-cli.svg?color=blue" alt="Downloads"></a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT-green">
</p>

`df` shows filesystems. `space` shows the storage that matters, what is using it,
and which mounts are merely operating-system machinery.

It collapses duplicate filesystem views, hides virtual mounts, recognizes common
developer data, and stays honest when running inside a container.

## Example

```text
Macintosh HD  645.5 / 994.7 GB  64.9%  ████████░░░░  349.2 GB free

  Developer     5.8 GB
    Gradle      4.0 GB
    npm         1.1 GB
  Personal      14.4 GB
    Documents   10.6 GB
    Downloads   3.8 GB
  Trash         2.1 GB
  Other         625.3 GB

Vorssaint     42.9 / 51.7 MB  83.1%  ██████████░░  8.8 MB free

13 system mounts hidden · --all details · --why explain
```

Usage bars are green below 70%, yellow from 70–85%, and red at 85% or
above. `NO_COLOR` disables color; `TERM=dumb` switches to ASCII.

## Why space?

A modern machine may expose dozens of mounts: `tmpfs`, `proc`, Docker
overlays, APFS system volumes, bind mounts, container namespaces, firmware
partitions, and developer runtimes. Reporting every one as a disk creates a
technically detailed but useless answer.

`space` instead:

- identifies independently backed storage locations;
- groups duplicate views of the same storage;
- hides virtual, temporary, and system mounts by default;
- assigns known paths to semantic categories;
- uses filesystem statistics for capacity instead of recursively summing `/`;
- explains hidden mounts when you ask.

## Install

```bash
cargo install space-cli
```

That installs the `space` binary. Rust 1.88 or newer is required.

### From source

```bash
git clone git@github.com:cesarferreira/space.git
cd space
cargo install --path .
```

## Usage

```bash
space
```

Show every hidden or derived mount:

```bash
space --all
```

Explain a mount, a filesystem type, or a folder category. For a category,
`space` shows where it is, what it holds, whether it is safe to delete, its
largest entries, and how to clean it:

```bash
space --why tmpfs
space --why /dev
space --why gradle
space --why node_modules --top 10
```

Skip semantic directory scanning for an instant capacity overview, or give the
scan more time on a large home folder (the default is 2 seconds):

```bash
space --no-scan
space --scan-timeout 10
```

Sizes that the scan could not finish in time, or could not fully read, are
shown as lower bounds, such as `≥ 3.4 GB`.

Emit machine-readable output:

```bash
space --json
space --json --no-scan
```

### Find space to free

List caches, build output, and other reclaimable data, largest first:

```bash
space --reclaimable
space --reclaimable --json
```

```text
Reclaimable space

  Gradle            4.0 GB  cache         space --clean gradle
  Trash             2.1 GB  review first  space --empty-trash
  node_modules      1.8 GB  generated     space --clean node_modules
  Android           1.5 GB  review first  space --why android

  Total             9.4 GB  7.9 GB can be cleaned by space
```

"Review first" means the folder may hold something you want; `space --why`
explains what to check.

### Clean up

Delete caches and build output by category name, or a whole group such as
`developer` or `projects`. `space` shows what it will delete and its size, then
asks before deleting anything:

```bash
space --clean gradle
space --clean npm,pip,node_modules
space --clean gradle --yes   # skip the prompt, e.g. in scripts
```

Empty the trash, including the trashes on other mounted volumes:

```bash
space --empty-trash
space --empty-trash --yes
```

Without a terminal on stdin, `--clean` and `--empty-trash` refuse to delete
unless you pass `--yes`.

Some categories are never deleted by `space`, because they hold your files or
are costly to get back: Downloads, Documents, Photos, Rustup, Android, Homebrew,
Nix, Ollama, and Hugging Face. `space --why <category>` tells you how to clean
them yourself. Where a tool has its own cleanup command, `space` runs it instead
of deleting files: `pnpm store prune`, `uv cache clean`, `go clean -modcache`,
`xcrun simctl delete unavailable`, and `docker system prune --force`.

### Track changes

Show what grew or shrank since the previous `--diff` run. The first run saves a
snapshot:

```bash
space --diff
```

```text
Changes since 2 days ago

Home       +3.0 GB used  (70.8 GB → 73.8 GB)
  Gradle         +2.0 GB
  npm          -500.0 MB
```

### Scripts and monitoring

Exit with status 3 when any volume is at least 90% full:

```bash
space --no-scan --fail-above 90
```

## Categories

| Group     | Categories                                                                                                  |
| --------- | ----------------------------------------------------------------------------------------------------------- |
| Personal  | Downloads, Documents, Photos                                                                                |
| Developer | Cargo, Rustup, Gradle, Maven, npm, pnpm, Yarn, pip, uv, Go modules, Go build, Android, Homebrew, Brew cache, Nix |
| Developer (macOS) | DerivedData, iOS support, Simulators, Docker                                                        |
| AI        | Ollama, Hugging Face                                                                                        |
| Caches    | Everything else in `~/.cache` (Linux) or `~/Library/Caches` (macOS)                                         |
| Projects  | `node_modules` and Rust `target` folders found under your home folder and the current folder              |
| Trash     | `~/.Trash`, `~/.local/share/Trash`, and per-volume trashes                                                  |

Sizes count the disk blocks files use, like `du`, so sparse files such as
Docker's virtual disk are not overstated. A folder inside another category's
folder is counted only once. The project
search skips hidden folders and, on macOS, `~/Library`.

## JSON

The JSON report uses byte counts and a versioned root schema:

```json
{
  "schema_version": 1,
  "storage": [
    {
      "name": "Home",
      "total_bytes": 150300000000,
      "used_bytes": 70800000000,
      "available_bytes": 79500000000,
      "used_percent": 47.1,
      "filesystem": "nfs4",
      "physical": true,
      "categories": []
    }
  ],
  "hidden_mounts": [],
  "mount_count": 43,
  "independent_storage_count": 4
}
```

New fields may be added, but existing field names and meanings are intended to
remain stable.

## Container behavior

Containers generally cannot see the host's complete physical-disk topology.
When `space` detects a container, it reports independently backed storage
locations visible inside that environment—such as Home, Workspace, mounted
data, and the container root—and says so explicitly.

It does not pretend those locations are physical disks.

## Safety and permissions

`space` is read-only by default. Only `--clean` and `--empty-trash` delete
data, and both ask for confirmation first. `--diff` writes one snapshot file:
`~/.local/state/space/snapshot.json` on Linux,
`~/Library/Application Support/space/snapshot.json` on macOS, or
`%LOCALAPPDATA%\space\snapshot.json` on Windows. Set `SPACE_STATE_DIR` to
use another folder. `space` does not repair filesystems, edit partitions, or
require root for normal operation.

On macOS, the Trash is protected by the system: if your terminal does not have
Full Disk Access, the Trash shows as `unreadable` and cannot be emptied.

Inaccessible paths produce lower-bound estimates rather than aborting the
entire scan.

## Platform status

- Linux: native mount discovery through `/proc/self/mountinfo`, sysfs identity,
  and `statvfs` capacity data.
- macOS: native mount discovery through `getmntinfo`, with structured APFS
  container enrichment from `diskutil -plist` when available.
- Windows: drive discovery through the Win32 volume APIs, when built from
  source. Windows is not tested in CI, release binaries are not published for
  it, and there is no Recycle Bin support.

## Development

```bash
make build
make test
make lint
make build-release
```

Or run the Cargo commands directly:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release
```

## License

MIT
