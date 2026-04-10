//! Delete `archive/*_*_{YYYYmmddHH}.log` segments older than retention.

use chrono::Utc;
use std::path::Path;
use std::time::SystemTime;

use crate::hour_bucket_start_utc;

pub fn prune_archive_dir(dir: &Path, retention: std::time::Duration) -> std::io::Result<u64> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let cutoff = Utc::now() - chrono::Duration::from_std(retention).unwrap_or_else(|_| {
        chrono::Duration::try_seconds(48 * 3600).expect("48h")
    });
    let mut removed_bytes: u64 = 0;
    for ent in std::fs::read_dir(dir)? {
        let ent = ent?;
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.ends_with(".log") {
            continue;
        }
        let Some(ts) = extract_hour_suffix(name) else {
            continue;
        };
        let Some(bucket_start) = hour_bucket_start_utc(ts) else {
            continue;
        };
        if bucket_start >= cutoff {
            continue;
        }
        if let Ok(meta) = path.metadata() {
            removed_bytes = removed_bytes.saturating_add(meta.len());
        }
        let _ = std::fs::remove_file(&path);
    }
    Ok(removed_bytes)
}

/// Expect `..._<YYYYmmddHH>.log` (10-digit hour bucket).
fn extract_hour_suffix(filename: &str) -> Option<&str> {
    let name = filename.strip_suffix(".log")?;
    let idx = name.rfind('_')?;
    let ts = &name[idx + 1..];
    if ts.len() == 10 && ts.chars().all(|c| c.is_ascii_digit()) {
        Some(ts)
    } else {
        None
    }
}

/// Also prune by filesystem mtime when hour parse fails (defensive).
#[allow(dead_code)]
pub fn prune_archive_dir_by_mtime(
    dir: &Path,
    retention: std::time::Duration,
) -> std::io::Result<u64> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let now = SystemTime::now();
    let mut removed_bytes: u64 = 0;
    for ent in std::fs::read_dir(dir)? {
        let ent = ent?;
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let Ok(meta) = path.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age > retention {
            removed_bytes = removed_bytes.saturating_add(meta.len());
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(removed_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extract_hour() {
        assert_eq!(
            extract_hour_suffix("prj_abcd_2020010100.log"),
            Some("2020010100")
        );
        assert_eq!(extract_hour_suffix("sys_2099123123.log"), Some("2099123123"));
    }

    #[test]
    fn prune_old_file() {
        let dir = tempdir().unwrap();
        let arch = dir.path().join("archive");
        std::fs::create_dir_all(&arch).unwrap();
        let old = arch.join("sys_2020010100.log");
        std::fs::write(&old, b"bye").unwrap();
        let n = prune_archive_dir(&arch, std::time::Duration::from_secs(60)).unwrap();
        assert!(n >= 3);
        assert!(!old.exists());
    }
}
