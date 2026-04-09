//! Persist which catalog models are hidden from dropdowns / pickers (`model_dropdown_disabled.json`).

use std::collections::HashSet;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize)]
struct File {
    #[serde(default)]
    disabled_ids: Vec<String>,
}

pub fn load(home: &Path) -> HashSet<String> {
    let path = home.join("model_dropdown_disabled.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return HashSet::new();
    };
    serde_json::from_slice::<File>(&bytes)
        .map(|f| f.disabled_ids.into_iter().collect())
        .unwrap_or_default()
}

pub fn save(home: &Path, ids: &HashSet<String>) -> std::io::Result<()> {
    let mut v: Vec<String> = ids.iter().cloned().collect();
    v.sort();
    let file = File { disabled_ids: v };
    let path = home.join("model_dropdown_disabled.json");
    let json = serde_json::to_vec_pretty(&file).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
    })?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}
