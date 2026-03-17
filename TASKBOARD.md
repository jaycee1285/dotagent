# dotagent — Skills/Hooks Explorer

## Project Summary

Desktop egui app for browsing, editing, and syncing Claude Code + Codex skills and hooks. Reads live from `~/.claude/` and `~/.codex/`, shows sync status against `~/repos/digtwin/` backup, and provides a quick editor for spot-check edits.

## Surfaces

| Surface | Source path | Content |
|---------|------------|---------|
| Claude Code skills (user) | `~/.claude/skills/*/SKILL.md` | ~853 skills |
| Claude Code hooks (user) | `~/.claude/settings.json` → hooks section | SessionStart, SessionEnd, etc. |
| Codex skills (user) | `~/.codex/skills/*/SKILL.md` | 7 skills (same dir structure as Claude) |
| Codex rules (user) | `~/.codex/rules/*` | Flat files (e.g. `default.rules`) |
| Backup (digtwin) | `~/repos/digtwin/claude/skills/`, `~/repos/digtwin/codex/skills/`, etc. | Copied backup |

Project-level skills are out of scope for the primary view.

## Design

- **egui 0.33** pinned
- **Font**: Spline Sans Mono Bold 20px (headings), Spline Sans 15px (body/buttons), Spline Sans 13px (small/paths), Spline Sans Mono 14px (code) — TTFs in `fonts/`
- **Icons**: egui-phosphor (regular variant) — Phosphor icons for buttons and actions
- **Colors**: Parse `~/.config/gtk-4.0/gtk.css` at startup for GTK named colors. Currently Ayu Light derived.
- **Syntax**: Load `~/.config/syntect/current.tmTheme` for skill/hook content highlighting via syntect
- **Spacing**: 8px base unit, Fibonacci tiers (8/16/24/40/64) per `~/repos/digtwin/design-basics.md`
- **Layout**: Sidebar tree + editor pane, maximizable editor, progressive disclosure

## Sync Status Indicators

| Color | Meaning |
|-------|---------|
| Green | Skill/hook exists in digtwin backup AND diff shows no difference |
| Yellow | Skill/hook exists in digtwin backup BUT content differs, a hard-coded external `.js`/`.ts` skill script is missing from backup, or Claude/Codex same-name skills differ by >=30% file size |
| Red | Skill/hook has no backup in digtwin |

## Digtwin Backup Structure

```
~/repos/digtwin/
├── claude/
│   ├── skills/{name}/SKILL.md
│   └── hooks/settings-hooks.json
└── codex/
    ├── skills/{name}/SKILL.md
    └── rules/{filename}
```

## Provenance / Grouping

Skills are grouped in sidebar by source/author parsed from YAML frontmatter:
- `source: self` or `author: John Curran` / `john-curran` → "Personal"
- `source: vibeship-spawner-skills (Apache 2.0)` → "vibeship-spawner-skills"
- GitHub URLs → shortened to `owner/repo`
- Codex skills with no metadata default to "Personal" (codex has no provenance system)
- Everything else → "Unknown source"

## Tasks

### Phase 1: Scaffold — DONE

- [x] **T1** `flake.nix` at repo root with Rust + egui deps for NixOS
- [x] **T2** `Cargo.toml` + `src/main.rs` — eframe window with GTK CSS color loading
- [x] **T3** Parse `~/.config/gtk-4.0/gtk.css` → `@define-color` values into theme struct
- [x] **T4** Load Spline Sans Mono + Spline Sans fonts into egui

### Phase 2: Skill/Hook Discovery — DONE

- [x] **T5** Scanner: walk `~/.claude/skills/*/SKILL.md`, parse frontmatter
- [x] **T6** Scanner: parse `~/.claude/settings.json` hooks section
- [x] **T7** Scanner: walk `~/.codex/skills/*/SKILL.md` and `~/.codex/rules/*`
- [x] **T8** Scanner: check `~/repos/digtwin/` backup state via `sync::backup_dest()`
- [x] **T9** Diff engine: compare source content against digtwin backup → green/yellow/red
- [x] **T9b** Parse YAML frontmatter (`source`, `metadata.author`) for provenance grouping

### Phase 3: Tree View + Navigation — DONE (scaffold)

- [x] **T10** Sidebar with top-level nodes: Claude Code Skills, Claude Code Hooks, Codex Skills, Codex Rules
- [x] **T10b** Skills grouped by source/author (Personal first, then alphabetical)
- [x] **T11** Each item shows name + sync status colored dot
- [x] **T12** Click item → loads content in editor pane
- [x] **T13** Search/filter bar (substring match on skill name)
- [x] **T21** Status bar: total count + synced/modified/unbackedup breakdown

### Phase 4: Selection + Actions — DONE

- [x] **T24** Rescan preserves selection by name (`rescan_preserving_selection()`, `rescan_after_delete()`)
- [x] **T25** Full directory backup for any skill (claude or codex), not just SKILL.md (`copy_dir_recursive()`)
- [x] **T25b** Skill backup also captures hard-coded external `/home/john/...` `.js` / `.ts` scripts referenced from `SKILL.md`, mirrored under `_external/` in the backed-up skill tree
- [x] **T26** Single-select → backup/delete via right-click context menu
- [x] **T27** Multi-select via checkboxes + Space toggle → batch backup/delete
- [x] **T28** Group-level actions via right-click on group headers (backup/delete all in group)
- [x] **T29** Delete with confirmation dialog, auto-advance to next entry

### Phase 5: Editor + Viewing — PARTIAL

- [x] **T14** Syntect-highlighted read view using `~/.config/syntect/current.tmTheme` via LayoutJob (src/syntax.rs)
- [x] **T15** Basic edit mode: E toggle, mutable TextEdit, save to disk via Ctrl+S
- [x] **T16** Dirty state: `~` indicator in warning color, save/discard/cancel dialog on nav-away
- [x] **T16b** Syntax highlighting in `(e)dit` mode, using `/home/john/repos/Ferrite/src/markdown/syntax.rs` as the referent for syntect-backed editor rendering

### Phase 6: Hooks UX

- [ ] **T30** Hook entries should inline the resolved shell script content cleanly in the viewer (treat hook body + script body as first-class readable content, not an awkward dump)
- [ ] **T31** Hook display should show the script name/path prominently, not just the raw trigger metadata
- [ ] **T31b** Hook viewer should make the JSON vs resolved script sections visually distinct and easy to skim

### Phase 7: Visibility / Mismatch Signals

- [ ] **T17** Yellow-state validator for hard-coded external skill scripts: if `SKILL.md` references `/home/john/.../*.js|ts`, mark yellow when that mirrored script is missing from the backed-up skill tree in digtwin
- [ ] **T18** Claude/Codex same-name mismatch indicator: mark yellow when both exist but their primary `SKILL.md` file sizes differ by >=30%
- [ ] **T19** Surface-level explanation of yellow state should distinguish: content diff, external script missing from backup, or Claude/Codex substantive mismatch

### Phase 8: Polish — PARTIAL

- [x] **T20** Keyboard shortcuts: Ctrl+S (filter/save), Ctrl+H (help overlay), arrows + Left/Right collapse, Space (toggle checkbox), E (edit), Esc (exit edit/clear filter), Ctrl+Left/Right (pane switch in non-edit; word skip in edit)
- [x] **T20b** Ctrl+B (backup shortcut), Ctrl+F (alternate search shortcut)
- [x] **T22** Window title reflects current selection (ViewportCommand::Title)
- [ ] **T23** Migrate from CollapsingState to egui_ltreeview for proper tree behavior
- [x] **T32** Focus outline on active pane (accent stroke on active panel frame)
- [ ] **T33** `~/.dotagent/` index for scan speed optimization
- [x] **T34** Edit mode shortcut protection (kb_active gated on editor_focused; TextEdit gets all keys except Ctrl+S/Esc)
- [x] **T35** Phosphor icons on all action buttons (Edit/Save/Backup/Delete/Rescan)
- [x] **T36** Font sizes per design-basics.md (heading 20px bold mono, body 15px, small 13px, code 14px)
- [x] **T37** Viewer pane header rework: title+dirty line, path line, icon button bar
- [x] **T38** Background fix: panel_fill uses view_bg (#fafafa) not window_bg (#e8e9ea)
- [x] **T39** Status bar editing indicator (EDITING / ~ EDITING) with accent/warning color
- [x] **T40** Keyboard scroll in viewer read mode (Up/Down, 40px step, via ScrollArea state)
- [ ] **T41** Bulk backup action for all red/yellow entries currently in view (fast human "mash B once" cleanup pass)

## Architecture

```
src/
  main.rs          — eframe app entry, mod declarations
  app.rs           — top-level App struct, update loop, UI rendering, font loading
  syntax.rs        — SyntaxHighlighter (OnceLock singleton): tmTheme → LayoutJob
  theme.rs         — GTK CSS parser → egui color mappings
  sync.rs          — backup/restore/delete operations, copy_dir_recursive
  scanner/
    mod.rs         — SkillEntry, SkillMeta, Surface, SyncStatus, frontmatter parser
    claude.rs      — ~/.claude/ skill + hook discovery, hook script resolution
    codex.rs       — ~/.codex/ skill + rule discovery
    digtwin.rs     — sync status checker using sync::backup_dest paths
```

## Key Files Outside This Repo

| Path | Role |
|------|------|
| `~/.config/gtk-4.0/gtk.css` | Color source of truth (always has `@define-color` vars) |
| `~/.config/syntect/current.tmTheme` | Syntax highlighting theme (live, always current) |
| `~/repos/digtwin/design-basics.md` | Design manifesto: typography, spacing, color rules, layout |
| `~/repos/Ferrite/` | Reference for egui patterns (egui 0.28, font loading, syntect). UI is ugly — use for code patterns only. |
| `~/repos/dotagent/egui_ltreeview/` | Tree view widget (egui 0.33). API: `TreeView::new(id).show(ui, \|builder\| { builder.dir/leaf/close_dir })` |

## Smoke Test

```sh
nix develop -c cargo run
```

Expect: window opens with view_bg (#fafafa) background, Ayu Light GTK colors, sidebar shows ~861 items grouped by provenance. Heading in bold Spline Sans Mono 20px, body in Spline Sans 15px. Phosphor icons on all action buttons. Syntax-highlighted read view via tmTheme. Edit mode (E) with cursor auto-focus, Ctrl+Left/Right word skip, full shortcut protection. Focus outline on active pane. Status bar shows EDITING / ~ EDITING state. Headerbar shows filename with dirty marker. 27.7MB in dev mode.

## Session Notes

- Codex has ZERO provenance tracking (no author, source, install date, or database). Skills with no metadata default to "Personal" for codex surface.
- 853 Claude skills, ~721 have no source/author metadata ("Unknown source"), 54 from vibeship-spawner-skills, ~12 personal.
- GTK CSS and tmTheme are guaranteed stable formats — John's theme changer always outputs the same variable set.
- `~/repos/digtwin/` is a personal GitHub repo backed up by systemd timer every 2 hours. Primary value of this app is keeping skills synced there.
- Human smoke in this session passed for edit-mode syntax highlighting and external hard-coded skill-script backup capture; no user notes beyond one fixed Ferrite path typo.
