use super::{SkillEntry, SkillMeta, Surface, SyncStatus};
use std::path::{Path, PathBuf};

/// Scan ~/.pi/agent/extensions for globally auto-discovered Pi extensions.
///
/// Pi supports single-file, directory, and package-style extensions:
///   ~/.pi/agent/extensions/*.ts
///   ~/.pi/agent/extensions/*/index.ts
///   ~/.pi/agent/extensions/*/package.json with pi.extensions entries
pub fn scan_extensions() -> Vec<SkillEntry> {
    let base = dirs::home_dir()
        .unwrap_or_default()
        .join(".pi")
        .join("agent")
        .join("extensions");

    let mut entries = Vec::new();

    if !base.exists() {
        return entries;
    }

    if let Ok(reader) = std::fs::read_dir(&base) {
        for entry in reader.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("ts") {
                entries.push(extension_entry(file_stem_name(&path), path));
            } else if path.is_dir() {
                let index = path.join("index.ts");
                if index.exists() {
                    entries.push(extension_entry(file_name(&path), index));
                    continue;
                }

                if let Some(extension_path) = package_extension_path(&path) {
                    entries.push(extension_entry(file_name(&path), extension_path));
                }
            }
        }
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}

fn package_extension_path(dir: &Path) -> Option<PathBuf> {
    let package_json = dir.join("package.json");
    let content = std::fs::read_to_string(package_json).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let extensions = parsed
        .get("pi")
        .and_then(|pi| pi.get("extensions"))
        .and_then(|extensions| extensions.as_array())?;

    extensions
        .iter()
        .filter_map(|entry| entry.as_str())
        .map(|entry| dir.join(entry.trim_start_matches("./")))
        .find(|path| path.is_file())
}

fn extension_entry(name: String, path: PathBuf) -> SkillEntry {
    SkillEntry {
        name,
        surface: Surface::PiExtension,
        path,
        content: None,
        sync_status: SyncStatus::Unknown,
        meta: SkillMeta::default(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn file_stem_name(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}
