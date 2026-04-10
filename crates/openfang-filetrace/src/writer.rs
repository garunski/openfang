//! Hourly rolling writer: active `.log` → `archive/{prefix}_{YYYYmmddHH}.log`.

use chrono::Utc;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug)]
pub struct RotatingWriter {
    active: PathBuf,
    archive_dir: PathBuf,
    archive_prefix: String,
    current_hour: Option<String>,
}

impl RotatingWriter {
    pub fn new(active: PathBuf, archive_dir: PathBuf, archive_prefix: String) -> Self {
        Self {
            active,
            archive_dir,
            archive_prefix,
            current_hour: None,
        }
    }

    /// Append one logical line (caller omits trailing `\n`; we add it).
    pub fn append_line(&mut self, line: &str) -> std::io::Result<()> {
        let hour = Utc::now().format("%Y%m%d%H").to_string();
        if let Some(ref old) = self.current_hour {
            if old != &hour {
                let prev = old.clone();
                self.rotate_closed_hour(&prev)?;
                self.current_hour = Some(hour.clone());
            }
        } else {
            self.current_hour = Some(hour.clone());
        }

        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.active)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()?;
        Ok(())
    }

    fn rotate_closed_hour(&mut self, old_hour: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.archive_dir)?;
        if self.active.exists() {
            let len = std::fs::metadata(&self.active)?.len();
            if len > 0 {
                let dest = self
                    .archive_dir
                    .join(format!("{}_{}.log", self.archive_prefix, old_hour));
                std::fs::rename(&self.active, &dest)?;
            } else {
                let _ = std::fs::remove_file(&self.active);
            }
        }
        Ok(())
    }
}
