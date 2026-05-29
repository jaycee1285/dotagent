use super::{parse_frontmatter, SkillEntry, Surface, SyncStatus};

/// Scan ~/.agents/skills/*/SKILL.md for agent skills.
pub fn scan_skills() -> Vec<SkillEntry> {
    let base = dirs::home_dir()
        .unwrap_or_default()
        .join(".agents")
        .join("skills");

    let mut entries = Vec::new();

    if !base.exists() {
        return entries;
    }

    if let Ok(reader) = std::fs::read_dir(&base) {
        for entry in reader.flatten() {
            let dir_path = entry.path();
            if !dir_path.is_dir() {
                continue;
            }
            let skill_file = dir_path.join("SKILL.md");
            if skill_file.exists() {
                let name = dir_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let meta = std::fs::read_to_string(&skill_file)
                    .map(|c| parse_frontmatter(&c))
                    .unwrap_or_default();

                entries.push(SkillEntry {
                    name,
                    surface: Surface::AgentSkill,
                    path: skill_file,
                    content: None,
                    sync_status: SyncStatus::Unknown,
                    meta,
                });
            }
        }
    }

    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    entries
}
