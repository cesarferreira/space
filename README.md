# space

**Disk usage explained in human terms.**

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

The repository is currently private and the crate is not published. Install
from a clone:

```bash
git clone git@github.com:cesarferreira/space.git
cd space
cargo install --path .
```

Rust 1.85 or newer is required.

## Usage

```bash
space
```

Show every hidden or derived mount:

```bash
space --all
```

Explain a mount or filesystem type:

```bash
space --why tmpfs
space --why /dev
```

Skip semantic directory scanning for an instant capacity overview:

```bash
space --no-scan
```

Emit machine-readable output:

```bash
space --json
space --json --no-scan
```

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

`space` is read-only. It does not delete data, repair filesystems, edit
partitions, or require root for normal operation.

Inaccessible paths produce lower-bound estimates rather than aborting the
entire scan.

## Platform status

- Linux: native mount discovery through `/proc/self/mountinfo`, sysfs identity,
  and `statvfs` capacity data.
- macOS: native mount discovery through `getmntinfo`, with structured APFS
  container enrichment from `diskutil -plist` when available.

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
