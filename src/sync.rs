use crate::scanner::{SkillEntry, Surface};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Get the digtwin backup base path.
fn digtwin_base() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join("repos").join("digtwin"))
}

/// Compute the backup destination for an entry, using the new structure:
///   ~/repos/digtwin/claude/skills/{name}/SKILL.md
///   ~/repos/digtwin/claude/hooks/settings-hooks.json
///   ~/repos/digtwin/agentskills/{name}/SKILL.md
///   ~/repos/digtwin/pi/{name}.ts
///   ~/repos/digtwin/pi/{name}/index.ts
pub fn backup_dest(entry: &SkillEntry) -> Option<PathBuf> {
    let base = digtwin_base()?;
    match entry.surface {
        Surface::ClaudeSkill => Some(
            base.join("claude")
                .join("skills")
                .join(&entry.name)
                .join("SKILL.md"),
        ),
        Surface::ClaudeHook => Some(
            base.join("claude")
                .join("hooks")
                .join("settings-hooks.json"),
        ),
        Surface::AgentSkill => Some(
            base.join("agentskills")
                .join(&entry.name)
                .join("SKILL.md"),
        ),
        Surface::PiExtension => {
            let extension_root = pi_extension_root(entry)?;
            if extension_root.is_dir() {
                Some(base.join("pi").join(&entry.name).join("index.ts"))
            } else {
                let filename = entry.path.file_name()?;
                Some(base.join("pi").join(filename))
            }
        }
    }
}

/// Backup a single entry to digtwin (copy, not symlink).
pub fn backup_entry(entry: &SkillEntry) -> Result<PathBuf, String> {
    let dest = backup_dest(entry).ok_or("Cannot compute backup path")?;

    // Create parent dirs
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dirs: {}", e))?;
    }

    // For hooks, content is already in-memory (just the JSON portion, not the script)
    if entry.surface == Surface::ClaudeHook {
        if let Some(ref content) = entry.content {
            // Strip the "--- Script content ---" suffix if present
            let json_part = content
                .split("\n\n--- Script content ---")
                .next()
                .unwrap_or(content);
            std::fs::write(&dest, json_part)
                .map_err(|e| format!("Failed to write hook backup: {}", e))?;
            return Ok(dest);
        }
    }

    // For skills: copy the entire directory (scripts, templates, agents, etc.),
    // then replace the source directory with a symlink to the digtwin copy.
    if matches!(entry.surface, Surface::ClaudeSkill | Surface::AgentSkill) {
        let source_dir = entry
            .path
            .parent()
            .ok_or("Cannot determine skill directory")?;
        let dest_dir = dest.parent().ok_or("Cannot determine backup directory")?;
        sync_dir_to_digtwin(source_dir, dest_dir)?;
        copy_hardcoded_skill_scripts(&entry.path, dest_dir)?;
        return Ok(dest);
    }

    // For Pi directory extensions: copy the whole extension directory so helper
    // modules, package manifests, and runtime assets stay together, then symlink.
    if entry.surface == Surface::PiExtension {
        let extension_root = pi_extension_root(entry).ok_or("Cannot determine extension root")?;
        if extension_root.is_dir() {
            let dest_dir = dest
                .parent()
                .ok_or("Cannot determine extension backup directory")?;
            sync_dir_to_digtwin(&extension_root, dest_dir)?;
        } else {
            sync_file_to_digtwin(&entry.path, &dest)?;
        }
        return Ok(dest);
    }

    // Fallback: plain copy
    std::fs::copy(&entry.path, &dest).map_err(|e| format!("Failed to copy: {}", e))?;

    Ok(dest)
}

/// Copy `source_dir` into `dest_dir`, then replace `source_dir` with a
/// symlink pointing to `dest_dir`. The digtwin copy becomes the source of
/// truth; the original location is a symlink back.
///
/// Idempotent: if `source_dir` is already a symlink to `dest_dir`, skip.
fn sync_dir_to_digtwin(source_dir: &Path, dest_dir: &Path) -> Result<(), String> {
    let already_linked = std::fs::symlink_metadata(source_dir)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
        && std::fs::read_link(source_dir)
            .map(|t| t == dest_dir)
            .unwrap_or(false);

    if already_linked {
        // Make sure digtwin copy exists; if it does, nothing to do.
        if dest_dir.exists() {
            return Ok(());
        }
        // Symlink dangles — fall through and rebuild from... nowhere. Bail.
        return Err(format!(
            "{} is a symlink to missing {}",
            source_dir.display(),
            dest_dir.display()
        ));
    }

    copy_dir_recursive(source_dir, dest_dir)?;

    // Replace source with symlink
    std::fs::remove_dir_all(source_dir)
        .map_err(|e| format!("Failed to remove source {}: {}", source_dir.display(), e))?;
    std::os::unix::fs::symlink(dest_dir, source_dir).map_err(|e| {
        format!(
            "Failed to symlink {} -> {}: {}",
            source_dir.display(),
            dest_dir.display(),
            e
        )
    })?;
    Ok(())
}

/// Copy a single file into digtwin, then replace the source with a symlink.
fn sync_file_to_digtwin(source: &Path, dest: &Path) -> Result<(), String> {
    let already_linked = std::fs::symlink_metadata(source)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
        && std::fs::read_link(source).map(|t| t == dest).unwrap_or(false);

    if already_linked && dest.exists() {
        return Ok(());
    }

    std::fs::copy(source, dest).map_err(|e| format!("Failed to copy: {}", e))?;
    std::fs::remove_file(source)
        .map_err(|e| format!("Failed to remove source {}: {}", source.display(), e))?;
    std::os::unix::fs::symlink(dest, source).map_err(|e| {
        format!(
            "Failed to symlink {} -> {}: {}",
            source.display(),
            dest.display(),
            e
        )
    })?;
    Ok(())
}

/// Backup all entries that match a given group label (e.g. "Personal").
/// Returns (success_count, errors).
pub fn backup_group(entries: &[SkillEntry], group_label: &str) -> (usize, Vec<String>) {
    let mut success = 0;
    let mut errors = Vec::new();

    for entry in entries {
        if entry.group_label() == group_label {
            match backup_entry(entry) {
                Ok(_) => success += 1,
                Err(e) => errors.push(format!("{}: {}", entry.name, e)),
            }
        }
    }

    (success, errors)
}

/// Backup ALL personal entries across all surfaces.
pub fn backup_all_personal(entries: &[SkillEntry]) -> (usize, Vec<String>) {
    backup_group(entries, "Personal")
}

/// Recursively copy a directory's contents to a destination.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create {}: {}", dst.display(), e))?;

    let entries =
        std::fs::read_dir(src).map_err(|e| format!("Failed to read {}: {}", src.display(), e))?;

    for entry in entries.flatten() {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy {}: {}", src_path.display(), e))?;
        }
    }
    Ok(())
}

/// Copy hard-coded device-local JS/TS script references mentioned in SKILL.md.
///
/// Contract: only absolute `/home/john/...` paths ending in `.js` or `.ts` are
/// treated as skill scripts. They are mirrored under `_external/home/john/...`
/// inside the backed-up skill directory.
fn copy_hardcoded_skill_scripts(skill_md: &Path, backup_skill_dir: &Path) -> Result<(), String> {
    let content = match std::fs::read_to_string(skill_md) {
        Ok(content) => content,
        Err(err) => {
            return Err(format!(
                "Failed to read {} for external script scan: {}",
                skill_md.display(),
                err
            ))
        }
    };

    for script_path in extract_hardcoded_script_paths(&content) {
        let relative = script_path
            .strip_prefix("/")
            .map_err(|_| format!("Expected absolute path, got {}", script_path.display()))?;
        let dest = backup_skill_dir.join("_external").join(relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
        std::fs::copy(&script_path, &dest).map_err(|e| {
            format!(
                "Failed to copy referenced script {}: {}",
                script_path.display(),
                e
            )
        })?;
    }

    Ok(())
}

fn extract_hardcoded_script_paths(content: &str) -> BTreeSet<PathBuf> {
    let mut paths = BTreeSet::new();
    let bytes = content.as_bytes();
    let mut start = 0usize;

    while let Some(offset) = content[start..].find("/home/john/") {
        let absolute_start = start + offset;
        let mut end = absolute_start;
        while end < bytes.len() && !is_skill_path_terminator(bytes[end] as char) {
            end += 1;
        }

        let candidate = &content[absolute_start..end];
        if looks_like_hardcoded_script_path(candidate) {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                paths.insert(path);
            }
        }

        start = end.saturating_add(1);
    }

    paths
}

fn looks_like_hardcoded_script_path(candidate: &str) -> bool {
    candidate.starts_with("/home/john/")
        && (candidate.ends_with(".js") || candidate.ends_with(".ts"))
}

fn is_skill_path_terminator(ch: char) -> bool {
    ch.is_whitespace()
        || matches!(
            ch,
            '`' | '"' | '\'' | ')' | '(' | ']' | '[' | '}' | '{' | '<' | '>' | ',' | ';'
        )
}

/// Delete a single entry from disk.
/// For skills: removes the parent directory (e.g. ~/.claude/skills/{name}/).
/// For rules: removes the file.
/// For hooks: not supported (they live inside settings.json).
pub fn delete_entry(entry: &SkillEntry) -> Result<(), String> {
    match entry.surface {
        Surface::ClaudeSkill | Surface::AgentSkill => {
            // path points to SKILL.md, parent is the skill directory
            let skill_dir = entry
                .path
                .parent()
                .ok_or("Cannot determine skill directory")?;
            std::fs::remove_dir_all(skill_dir)
                .map_err(|e| format!("Failed to delete {}: {}", skill_dir.display(), e))
        }
        Surface::PiExtension => {
            let extension_root =
                pi_extension_root(entry).ok_or("Cannot determine extension root")?;
            if extension_root.is_dir() {
                std::fs::remove_dir_all(&extension_root).map_err(|e| {
                    format!("Failed to delete {}: {}", extension_root.display(), e)
                })
            } else {
                std::fs::remove_file(&entry.path)
                    .map_err(|e| format!("Failed to delete {}: {}", entry.path.display(), e))
            }
        }
        Surface::ClaudeHook => {
            Err("Hook deletion not supported — hooks live in settings.json".to_string())
        }
    }
}

fn pi_extension_root(entry: &SkillEntry) -> Option<PathBuf> {
    if entry.surface != Surface::PiExtension {
        return None;
    }

    let global_extension_dir = dirs::home_dir()?
        .join(".pi")
        .join("agent")
        .join("extensions")
        .join(&entry.name);
    if global_extension_dir.is_dir() {
        return Some(global_extension_dir);
    }

    if entry.path.file_name().and_then(|n| n.to_str()) == Some("index.ts") {
        return entry.path.parent().map(|p| p.to_path_buf());
    }

    Some(entry.path.clone())
}
