use super::{Discovery, FilesystemStats, RawMount};
use anyhow::{Context, Result};
use std::{
    ffi::CString,
    fs,
    path::{Path, PathBuf},
};
fn decode(s: &str) -> String {
    s.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}
pub fn parse_mountinfo(input: &str) -> Result<Vec<RawMount>> {
    input
        .lines()
        .enumerate()
        .map(|(i, line)| {
            let (l, r) = line
                .split_once(" - ")
                .with_context(|| format!("mountinfo line {} has no separator", i + 1))?;
            let a: Vec<_> = l.split_whitespace().collect();
            let b: Vec<_> = r.split_whitespace().collect();
            if a.len() < 6 || b.len() < 3 {
                anyhow::bail!("mountinfo line {} is incomplete", i + 1)
            }
            Ok(RawMount {
                mount_id: a[0].parse()?,
                parent_id: a[1].parse()?,
                device_id: a[2].into(),
                root: decode(a[3]),
                target: PathBuf::from(decode(a[4])),
                mount_options: a[5].split(',').map(Into::into).collect(),
                filesystem: b[0].into(),
                source: decode(b[1]),
                super_options: b[2].split(',').map(Into::into).collect(),
                stats: None,
                stats_error: None,
                backing_id: None,
                volume_name: None,
                removable: false,
            })
        })
        .collect()
}
fn stats(path: &Path) -> Result<FilesystemStats> {
    let c = CString::new(path.as_os_str().as_encoded_bytes())?;
    let mut s = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(c.as_ptr(), s.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let s = unsafe { s.assume_init() };
    let bs = if s.f_frsize == 0 {
        s.f_bsize
    } else {
        s.f_frsize
    };
    let total = s.f_blocks.saturating_mul(bs);
    let free = s.f_bfree.saturating_mul(bs);
    Ok(FilesystemStats {
        total_bytes: total,
        used_bytes: total.saturating_sub(free),
        available_bytes: s.f_bavail.saturating_mul(bs),
    })
}
pub fn discover() -> Result<Discovery> {
    let text = fs::read_to_string("/proc/self/mountinfo")?;
    let mut mounts = parse_mountinfo(&text)?;
    for m in &mut mounts {
        match stats(&m.target) {
            Ok(s) => m.stats = Some(s),
            Err(e) => m.stats_error = Some(e.to_string()),
        }
        m.backing_id = Some(m.device_id.clone());
        m.removable = m.target.starts_with("/media") || m.target.starts_with("/run/media");
    }
    Ok(Discovery {
        mounts,
        warnings: vec![],
    })
}
