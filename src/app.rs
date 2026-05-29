use crate::scanner::{self, SkillEntry, Surface, SyncStatus};
use crate::syntax::SyntaxHighlighter;
use crate::theme::GtkTheme;
use egui_phosphor::regular as icons;
use std::collections::HashSet;

const SIDEBAR_LABEL_LIMIT: usize = 80;

// ---------------------------------------------------------------------------
// Navigation model
// ---------------------------------------------------------------------------

/// A node in the sidebar's navigable tree (flat list reflecting visible order).
#[derive(Clone, Debug)]
enum NavNode {
    /// A top-level surface header (e.g. "Claude Code Skills").
    SurfaceHeader(Surface),
    /// A group header within a grouped surface (e.g. "Personal (12)").
    GroupHeader {
        surface: Surface,
        label: String,
        indices: Vec<usize>,
    },
    /// An individual entry (index into `entries`).
    Entry(usize),
}

impl NavNode {
    fn entry_index(&self) -> Option<usize> {
        match self {
            NavNode::Entry(i) => Some(*i),
            _ => None,
        }
    }

    /// Check if this node matches a surface header.
    fn is_surface(&self, s: Surface) -> bool {
        matches!(self, NavNode::SurfaceHeader(x) if *x == s)
    }

    /// Check if this node matches a group header.
    fn is_group(&self, s: Surface, lbl: &str) -> bool {
        matches!(self, NavNode::GroupHeader { surface, label, .. } if *surface == s && label == lbl)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FocusPane {
    Sidebar,
    Viewer,
}

// ---------------------------------------------------------------------------
// Action model
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum PendingAction {
    Delete(Vec<usize>),
    Backup(Vec<usize>),
}

/// Where the user is trying to navigate when dirty save dialog appears.
#[derive(Clone)]
enum DirtyNavTarget {
    /// Switching to sidebar (losing edit focus).
    Sidebar,
    /// Selecting a different entry.
    Entry(usize),
}

#[derive(Clone)]
enum ContextTarget {
    Item(usize),
    Group(String, Vec<usize>),
}

// ---------------------------------------------------------------------------
// Stable egui IDs for collapsibles
// ---------------------------------------------------------------------------

fn surface_col_id(surface: Surface) -> egui::Id {
    egui::Id::new("nav_surface").with(surface.label())
}

fn group_col_id(surface: Surface, label: &str) -> egui::Id {
    egui::Id::new("nav_group").with(surface.label()).with(label)
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct DotagentApp {
    pub theme: GtkTheme,
    entries: Vec<SkillEntry>,
    selected_idx: Option<usize>,
    loaded_content: Option<String>,
    filter: String,
    last_sync_message: Option<String>,
    checked: HashSet<usize>,
    context_target: Option<ContextTarget>,
    context_menu_pos: egui::Pos2,
    pending_action: Option<PendingAction>,
    /// Flat ordered list of visible sidebar nodes (rebuilt each frame).
    nav_order: Vec<NavNode>,
    /// Cursor position in nav_order.
    nav_cursor: usize,
    /// Which pane has keyboard focus.
    focus_pane: FocusPane,
    /// Show keybindings help overlay.
    show_help: bool,
    /// Nav cursor moved this frame (triggers scroll-to).
    nav_moved: bool,
    /// Editor state.
    editing: bool,
    /// Mutable edit buffer (only valid when editing == true).
    edit_buffer: String,
    /// Content as loaded from disk (for dirty comparison).
    original_content: Option<String>,
    /// Pending dirty-save dialog: where the user wants to go.
    dirty_nav_target: Option<DirtyNavTarget>,
    /// Pending keyboard scroll delta for the viewer (pixels, applied once).
    viewer_scroll_delta: f32,
    /// Request focus on the editor TextEdit next frame.
    editor_wants_focus: bool,
}

impl DotagentApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let theme = GtkTheme::load();

        let mut visuals = egui::Visuals::light();
        visuals.override_text_color = Some(theme.view_fg());
        visuals.panel_fill = theme.view_bg();
        visuals.window_fill = theme.card_bg();
        visuals.extreme_bg_color = theme.view_bg();
        visuals.faint_bg_color = theme.headerbar_bg();
        visuals.selection.bg_fill = theme.accent();
        visuals.selection.stroke = egui::Stroke::new(1.0, theme.accent_fg());
        visuals.widgets.noninteractive.bg_fill = theme.card_bg();
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, theme.card_fg());
        visuals.widgets.hovered.bg_fill = theme.shade();
        visuals.widgets.active.bg_fill = theme.accent();

        cc.egui_ctx.set_visuals(visuals);
        load_fonts(&cc.egui_ctx);

        let entries = scanner::scan_all();

        Self {
            theme,
            entries,
            selected_idx: None,
            loaded_content: None,
            filter: String::new(),
            last_sync_message: None,
            checked: HashSet::new(),
            context_target: None,
            context_menu_pos: egui::Pos2::ZERO,
            pending_action: None,
            nav_order: Vec::new(),
            nav_cursor: 0,
            focus_pane: FocusPane::Sidebar,
            show_help: false,
            nav_moved: false,
            editing: false,
            edit_buffer: String::new(),
            original_content: None,
            dirty_nav_target: None,
            viewer_scroll_delta: 0.0,
            editor_wants_focus: false,
        }
    }

    // -- helpers -------------------------------------------------------------

    fn sync_color(&self, status: SyncStatus) -> egui::Color32 {
        match status {
            SyncStatus::Synced => self.theme.success(),
            SyncStatus::Modified => self.theme.warning(),
            SyncStatus::Unbackedup => self.theme.error(),
            SyncStatus::Unknown => self.theme.shade(),
        }
    }

    fn surface_entries(&self, surface: Surface) -> Vec<(usize, &SkillEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.surface == surface
                    && (self.filter.is_empty()
                        || e.name.to_lowercase().contains(&self.filter.to_lowercase()))
            })
            .collect()
    }

    fn grouped_surface_entries(&self, surface: Surface) -> Vec<(String, Vec<usize>)> {
        use std::collections::BTreeMap;
        let filtered = self.surface_entries(surface);
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (idx, entry) in filtered {
            let group = sidebar_group_label(entry);
            groups.entry(group).or_default().push(idx);
        }
        let mut result: Vec<_> = groups.into_iter().collect();
        result.sort_by(|a, b| match (a.0.as_str(), b.0.as_str()) {
            ("Personal", _) => std::cmp::Ordering::Less,
            (_, "Personal") => std::cmp::Ordering::Greater,
            _ => a.0.to_lowercase().cmp(&b.0.to_lowercase()),
        });
        result
    }

    fn status_summary(&self) -> (usize, usize, usize, usize) {
        let total = self.entries.len();
        let synced = self
            .entries
            .iter()
            .filter(|e| e.sync_status == SyncStatus::Synced)
            .count();
        let modified = self
            .entries
            .iter()
            .filter(|e| e.sync_status == SyncStatus::Modified)
            .count();
        let unbackedup = self
            .entries
            .iter()
            .filter(|e| e.sync_status == SyncStatus::Unbackedup)
            .count();
        (total, synced, modified, unbackedup)
    }

    fn is_dirty(&self) -> bool {
        self.editing && self.original_content.as_deref() != Some(&self.edit_buffer)
    }

    fn select_entry(&mut self, idx: usize) {
        self.selected_idx = Some(idx);
        let entry = &self.entries[idx];
        let content = if let Some(ref c) = entry.content {
            Some(c.clone())
        } else {
            std::fs::read_to_string(&entry.path).ok()
        };
        self.loaded_content = content.clone();
        self.original_content = content;
        // Exit edit mode when switching entries
        self.editing = false;
        self.edit_buffer.clear();
    }

    fn enter_edit_mode(&mut self) {
        if let Some(ref content) = self.loaded_content {
            self.edit_buffer = content.clone();
            self.original_content = Some(content.clone());
            self.editing = true;
            self.editor_wants_focus = true;
        }
    }

    fn apply_dirty_nav(&mut self, target: &DirtyNavTarget) {
        match target {
            DirtyNavTarget::Sidebar => {
                self.focus_pane = FocusPane::Sidebar;
            }
            DirtyNavTarget::Entry(idx) => {
                self.select_entry(*idx);
            }
        }
    }

    fn exit_edit_mode(&mut self, save: bool) {
        if save && self.is_dirty() {
            // Save to disk
            if let Some(idx) = self.selected_idx {
                let entry = &self.entries[idx];
                if let Err(e) = std::fs::write(&entry.path, &self.edit_buffer) {
                    self.last_sync_message = Some(format!("Save error: {}", e));
                } else {
                    self.last_sync_message = Some(format!("Saved '{}'", entry.name));
                    // Update loaded content to match
                    self.loaded_content = Some(self.edit_buffer.clone());
                    self.original_content = Some(self.edit_buffer.clone());
                }
            }
        } else if !save {
            // Discard changes — restore original
            self.loaded_content = self.original_content.clone();
        }
        self.editing = false;
        self.edit_buffer.clear();
    }

    /// After delete/rescan, try to keep the cursor in the same position so the
    /// next item is selected. Falls back to preserving by name, then clamping.
    fn rescan_after_delete(&mut self, deleted_names: &[String]) {
        // Remember cursor position and what was at the next slot
        let old_cursor = self.nav_cursor;

        self.entries = scanner::scan_all();
        self.checked.clear();

        // Try to select the entry that was at the same cursor position.
        // We can't rebuild nav_order yet (needs egui ctx), so just pick by name:
        // find the first non-deleted entry that was after the deleted one.
        if let Some(ref name) = self.selected_idx.and_then(|i| {
            if deleted_names.contains(
                &self
                    .entries
                    .get(i)
                    .map(|e| e.name.clone())
                    .unwrap_or_default(),
            ) {
                None
            } else {
                Some(self.entries[i].name.clone())
            }
        }) {
            self.selected_idx = self.entries.iter().position(|e| &e.name == name);
        } else {
            // Selected item was deleted — clear and let nav_cursor handle it
            self.selected_idx = None;
            self.loaded_content = None;
        }

        // Stash old_cursor so the next frame's nav rebuild can pick the right entry
        self.nav_cursor = old_cursor;
    }

    fn rescan_preserving_selection(&mut self) {
        let selected_name = self
            .selected_idx
            .and_then(|i| self.entries.get(i).map(|e| e.name.clone()));
        self.entries = scanner::scan_all();
        self.checked.clear();
        if let Some(name) = selected_name {
            self.selected_idx = self.entries.iter().position(|e| e.name == name);
            if let Some(idx) = self.selected_idx {
                let entry = &self.entries[idx];
                self.loaded_content = if let Some(ref c) = entry.content {
                    Some(c.clone())
                } else {
                    std::fs::read_to_string(&entry.path).ok()
                };
            } else {
                self.loaded_content = None;
            }
        } else {
            self.selected_idx = None;
            self.loaded_content = None;
        }
    }

    fn do_backup(&self, indices: &[usize]) -> String {
        let mut ok = 0usize;
        let mut errs = Vec::new();
        for &i in indices {
            if let Some(entry) = self.entries.get(i) {
                match crate::sync::backup_entry(entry) {
                    Ok(_) => ok += 1,
                    Err(e) => errs.push(format!("{}: {}", entry.name, e)),
                }
            }
        }
        if errs.is_empty() {
            format!("Backed up {} items", ok)
        } else {
            format!(
                "Backed up {}, {} errors: {}",
                ok,
                errs.len(),
                errs.join("; ")
            )
        }
    }

    fn do_delete(&self, indices: &[usize]) -> String {
        let mut ok = 0usize;
        let mut errs = Vec::new();
        for &i in indices {
            if let Some(entry) = self.entries.get(i) {
                match crate::sync::delete_entry(entry) {
                    Ok(()) => ok += 1,
                    Err(e) => errs.push(format!("{}: {}", entry.name, e)),
                }
            }
        }
        if errs.is_empty() {
            format!("Deleted {} items", ok)
        } else {
            format!("Deleted {}, {} errors: {}", ok, errs.len(), errs.join("; "))
        }
    }

    fn action_indices(&self, context_indices: &[usize]) -> Vec<usize> {
        if self.checked.is_empty() {
            context_indices.to_vec()
        } else {
            let mut set: HashSet<usize> = self.checked.iter().copied().collect();
            for &i in context_indices {
                set.insert(i);
            }
            let mut v: Vec<usize> = set.into_iter().collect();
            v.sort();
            v
        }
    }

    // -- nav order -----------------------------------------------------------

    /// Build the flat visible-order list by reading egui collapse state.
    fn build_nav_order(&self, ctx: &egui::Context) -> Vec<NavNode> {
        let mut order = Vec::new();

        // Grouped surfaces
        for surface in [
            Surface::ClaudeSkill,
            Surface::AgentSkill,
            Surface::PiExtension,
            Surface::ClaudeHook,
        ] {
            let groups = self.grouped_surface_entries(surface);
            order.push(NavNode::SurfaceHeader(surface));

            let surface_open = egui::collapsing_header::CollapsingState::load_with_default_open(
                ctx,
                surface_col_id(surface),
                matches!(surface, Surface::AgentSkill | Surface::PiExtension),
            )
            .is_open();

            if surface_open {
                for (label, indices) in &groups {
                    order.push(NavNode::GroupHeader {
                        surface,
                        label: label.clone(),
                        indices: indices.clone(),
                    });

                    let default_open = label == "Personal" || indices.len() <= 10;
                    let group_open =
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ctx,
                            group_col_id(surface, label),
                            default_open,
                        )
                        .is_open();

                    if group_open {
                        for &idx in indices {
                            order.push(NavNode::Entry(idx));
                        }
                    }
                }
            }
        }

        order
    }

    /// Sync nav_cursor to match a given entry index (e.g. after mouse click).
    fn sync_cursor_to_entry(&mut self, entry_idx: usize) {
        if let Some(pos) = self
            .nav_order
            .iter()
            .position(|n| matches!(n, NavNode::Entry(i) if *i == entry_idx))
        {
            self.nav_cursor = pos;
        }
    }

    /// Select whatever entry is at nav_cursor (if it's an Entry node).
    fn sync_selection_to_cursor(&mut self) {
        if let Some(NavNode::Entry(idx)) = self.nav_order.get(self.nav_cursor) {
            self.select_entry(*idx);
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

/// Paint a cursor indicator bar on the left edge of a response rect.
fn paint_cursor_bar(ui: &mut egui::Ui, resp: &egui::Response, color: egui::Color32) {
    let rect = resp.rect;
    let bar = egui::Rect::from_min_max(
        egui::pos2(rect.left() - 4.0, rect.top()),
        egui::pos2(rect.left() - 1.0, rect.bottom()),
    );
    ui.painter()
        .rect_filled(bar, egui::CornerRadius::ZERO, color);
}

fn sidebar_group_label(entry: &SkillEntry) -> String {
    if entry.surface == Surface::ClaudeHook {
        let label = entry
            .name
            .split_once(':')
            .map(|(prefix, _)| prefix.trim())
            .unwrap_or(entry.name.as_str());
        return truncate_sidebar_label(label);
    }

    truncate_sidebar_label(&entry.group_label())
}

fn sidebar_entry_label(entry: &SkillEntry) -> String {
    truncate_sidebar_label(&entry.name)
}

fn truncate_sidebar_label(label: &str) -> String {
    let mut chars = label.chars();
    let truncated: String = chars.by_ref().take(SIDEBAR_LABEL_LIMIT).collect();
    if chars.next().is_some() {
        format!("{}...", truncated)
    } else {
        truncated
    }
}

impl DotagentApp {
    fn render_entry_list(
        ui: &mut egui::Ui,
        items: &[(usize, &SkillEntry)],
        selected_idx: Option<usize>,
        nav_cursor_entry: Option<usize>,
        nav_moved: bool,
        theme: &GtkTheme,
        checked: &mut HashSet<usize>,
        new_selection: &mut Option<usize>,
        context_target: &mut Option<ContextTarget>,
    ) {
        for (idx, entry) in items {
            let is_selected = selected_idx == Some(*idx);
            let is_cursor = nav_cursor_entry == Some(*idx);
            let dot_color = match entry.sync_status {
                SyncStatus::Synced => theme.success(),
                SyncStatus::Modified => theme.warning(),
                SyncStatus::Unbackedup => theme.error(),
                SyncStatus::Unknown => theme.shade(),
            };

            let row_resp = ui.horizontal(|ui| {
                // Checkbox
                let mut is_checked = checked.contains(idx);
                if ui.checkbox(&mut is_checked, "").changed() {
                    if is_checked {
                        checked.insert(*idx);
                    } else {
                        checked.remove(idx);
                    }
                }

                // Sync dot
                let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, dot_color);

                // Selectable label — highlight if selected OR if cursor is here
                let entry_label = sidebar_entry_label(entry);
                let label_resp =
                    ui.add(egui::Button::new(entry_label).selected(is_selected || is_cursor));
                label_resp.clone().on_hover_text(&entry.name);
                if label_resp.clicked() {
                    *new_selection = Some(*idx);
                }
                if label_resp.secondary_clicked() {
                    *context_target = Some(ContextTarget::Item(*idx));
                }
            });

            // Scroll to keep cursor visible
            if is_cursor && nav_moved {
                row_resp.response.scroll_to_me(Some(egui::Align::Center));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Main update loop
// ---------------------------------------------------------------------------

impl eframe::App for DotagentApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = self.theme.view_bg();
        let n = |v: u8| v as f32 / 255.0;
        [n(c.r()), n(c.g()), n(c.b()), n(c.a())]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // -- Window title -------------------------------------------------------
        let title = match self.selected_idx.and_then(|i| self.entries.get(i)) {
            Some(entry) => format!("{} — dotagent", entry.name),
            None => "dotagent — Agent Resources Explorer".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // -- Rebuild nav order from current collapse state -------------------
        self.nav_order = self.build_nav_order(ctx);
        if self.nav_cursor >= self.nav_order.len() && !self.nav_order.is_empty() {
            self.nav_cursor = self.nav_order.len() - 1;
        }

        // If we have no selection but cursor is on an entry, select it
        // (handles post-delete advancement)
        if self.selected_idx.is_none() && !self.nav_order.is_empty() {
            if let Some(NavNode::Entry(idx)) = self.nav_order.get(self.nav_cursor) {
                self.select_entry(*idx);
            } else {
                // Cursor is on a header after delete — scan forward for next entry
                for i in self.nav_cursor..self.nav_order.len() {
                    if let NavNode::Entry(idx) = &self.nav_order[i] {
                        self.nav_cursor = i;
                        self.select_entry(*idx);
                        break;
                    }
                }
            }
        }

        // -- Keyboard input --------------------------------------------------
        let key_up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let key_down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));
        let key_left = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));
        let key_right = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
        let ctrl_left = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::ArrowLeft));
        let ctrl_right = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::ArrowRight));
        let key_b = ctx.input(|i| i.key_pressed(egui::Key::B));
        let key_d = ctx.input(|i| i.key_pressed(egui::Key::D));
        let key_c = ctx.input(|i| i.key_pressed(egui::Key::C));
        let key_x = ctx.input(|i| i.key_pressed(egui::Key::X));
        let key_esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let key_space = ctx.input(|i| i.key_pressed(egui::Key::Space));
        let ctrl_h = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::H));
        let ctrl_s = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S));
        let ctrl_f = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F));
        let ctrl_b = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::B));
        let key_e = ctx.input(|i| i.key_pressed(egui::Key::E));

        let filter_id = egui::Id::new("sidebar_filter");
        let filter_focused = ctx.memory(|m| m.focused().map_or(false, |id| id == filter_id));
        // When editing or filter focused, suppress global shortcuts so the
        // TextEdit gets Ctrl+Left/Right (word skip), arrows, letters, etc.
        let editor_focused = self.editing && self.focus_pane == FocusPane::Viewer;
        let kb_active = !filter_focused && !editor_focused;

        // -- Filter focus: Ctrl+S to open (sidebar), Esc to close ------------
        if (ctrl_s || ctrl_f) && !filter_focused && self.focus_pane == FocusPane::Sidebar {
            ctx.memory_mut(|m| m.request_focus(filter_id));
        }
        if filter_focused && (key_esc || (key_x && self.filter.is_empty())) {
            self.filter.clear();
            ctx.memory_mut(|m| m.surrender_focus(filter_id));
        }

        // -- Editor keybinds (viewer pane) -----------------------------------
        if kb_active && self.focus_pane == FocusPane::Viewer && !self.editing {
            if key_e && self.selected_idx.is_some() {
                self.enter_edit_mode();
            }
        }
        // Ctrl+S in viewer = save
        if ctrl_s && self.focus_pane == FocusPane::Viewer && self.editing {
            self.exit_edit_mode(true);
        }
        // Esc in viewer while editing = exit edit (prompt if dirty)
        // (not gated on kb_active — must work even in editor-focused mode)
        if key_esc && self.editing && self.focus_pane == FocusPane::Viewer {
            if self.is_dirty() {
                self.dirty_nav_target = Some(DirtyNavTarget::Sidebar);
            } else {
                self.exit_edit_mode(false);
            }
        }

        // -- Viewer keyboard scroll (read mode) --------------------------------
        self.viewer_scroll_delta = 0.0;
        if kb_active && self.focus_pane == FocusPane::Viewer && !self.editing {
            let scroll_step = 40.0; // pixels per keypress
            if key_up {
                self.viewer_scroll_delta = -scroll_step;
            }
            if key_down {
                self.viewer_scroll_delta = scroll_step;
            }
        }

        // -- Dirty save dialog -----------------------------------------------
        // Dialog keys are not gated on kb_active — dialog owns the frame.
        if let Some(ref target) = self.dirty_nav_target.clone() {
            let confirm = key_c;
            let discard = key_d;
            let cancel = key_x;

            let mut open = true;
            egui::Window::new("Unsaved Changes")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("You have unsaved changes.\n\n(C) Save & continue  |  (D) Discard  |  E(x)it");
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui.button("(C) Save").clicked() || confirm {
                            self.exit_edit_mode(true);
                            self.apply_dirty_nav(target);
                            self.dirty_nav_target = None;
                        }
                        if ui.button("(D) Discard").clicked() || discard {
                            self.exit_edit_mode(false);
                            self.apply_dirty_nav(target);
                            self.dirty_nav_target = None;
                        }
                        if ui.button("E(x)it").clicked() || cancel {
                            self.dirty_nav_target = None;
                        }
                    });
                });
            if !open {
                self.dirty_nav_target = None;
            }
            return;
        }

        // -- Help overlay toggle ---------------------------------------------
        if kb_active && ctrl_h {
            self.show_help = !self.show_help;
        }
        if self.show_help {
            if kb_active && key_esc {
                self.show_help = false;
            }
            egui::Window::new("Keybindings")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    egui::Grid::new("help_grid")
                        .num_columns(2)
                        .spacing([24.0, 4.0])
                        .show(ui, |ui| {
                            let row = |ui: &mut egui::Ui, key: &str, desc: &str| {
                                ui.label(egui::RichText::new(key).strong().monospace());
                                ui.label(desc);
                                ui.end_row();
                            };
                            ui.label(egui::RichText::new("Navigation").strong());
                            ui.end_row();
                            row(ui, "Up / Down", "Move cursor in sidebar tree");
                            row(ui, "Left", "Collapse header / go to parent");
                            row(ui, "Right", "Expand header / focus viewer");
                            row(ui, "Ctrl+Left", "Focus sidebar pane");
                            row(ui, "Ctrl+Right", "Focus viewer pane");
                            ui.end_row();
                            ui.label(egui::RichText::new("Selection").strong());
                            ui.end_row();
                            row(ui, "Space", "Toggle checkbox (entry or group)");
                            row(ui, "Click", "Select entry / focus pane");
                            ui.end_row();
                            ui.label(egui::RichText::new("Filter").strong());
                            ui.end_row();
                            row(ui, "Ctrl+S / Ctrl+F", "Focus search/filter box");
                            row(ui, "Esc", "Clear filter and return to tree");
                            ui.end_row();
                            ui.label(egui::RichText::new("Actions").strong());
                            ui.end_row();
                            row(ui, "B / Ctrl+B", "Backup selected/checked to digtwin");
                            row(ui, "D", "Delete selected/checked (with confirm)");
                            row(ui, "C", "Confirm dialog action");
                            row(ui, "X / Esc", "Cancel dialog / close menu");
                            ui.end_row();
                            ui.label(egui::RichText::new("Editor").strong());
                            ui.end_row();
                            row(ui, "E", "Enter edit mode (viewer pane)");
                            row(ui, "Ctrl+S", "Save changes (viewer) / search (sidebar)");
                            row(ui, "Esc", "Exit edit mode (prompts if dirty)");
                            ui.end_row();
                            ui.label(egui::RichText::new("Other").strong());
                            ui.end_row();
                            row(ui, "Ctrl+H", "Toggle this help overlay");
                        });
                });
            return;
        }

        // -- Pane switching (with dirty guard) --------------------------------
        if kb_active && ctrl_left {
            if self.is_dirty() {
                self.dirty_nav_target = Some(DirtyNavTarget::Sidebar);
            } else {
                if self.editing {
                    self.exit_edit_mode(false);
                }
                self.focus_pane = FocusPane::Sidebar;
            }
        }
        if kb_active && ctrl_right {
            self.focus_pane = FocusPane::Viewer;
        }

        // -- Space to toggle checkbox at cursor ------------------------------
        if kb_active && key_space && self.focus_pane == FocusPane::Sidebar {
            if let Some(node) = self.nav_order.get(self.nav_cursor).cloned() {
                match node {
                    NavNode::Entry(idx) => {
                        if self.checked.contains(&idx) {
                            self.checked.remove(&idx);
                        } else {
                            self.checked.insert(idx);
                        }
                    }
                    NavNode::GroupHeader { indices, .. } => {
                        let all_checked = indices.iter().all(|i| self.checked.contains(i));
                        for &i in &indices {
                            if all_checked {
                                self.checked.remove(&i);
                            } else {
                                self.checked.insert(i);
                            }
                        }
                    }
                    NavNode::SurfaceHeader(_) => {
                        // No-op for surface headers (too broad)
                    }
                }
            }
        }

        // -- Confirmation dialog ---------------------------------------------
        if let Some(ref action) = self.pending_action.clone() {
            let (title, body, indices) = match action {
                PendingAction::Delete(indices) => {
                    let names: Vec<String> = indices
                        .iter()
                        .filter_map(|&i| self.entries.get(i).map(|e| e.name.clone()))
                        .collect();
                    let count = names.len();
                    let preview = if count <= 5 {
                        names.join(", ")
                    } else {
                        format!("{}, ... and {} more", names[..5].join(", "), count - 5)
                    };
                    (
                        "Confirm Delete".to_string(),
                        format!(
                            "Delete {} items?\n\n{}\n\nThis removes files from disk.\n\n(C)onfirm  |  E(x)it",
                            count, preview
                        ),
                        indices.clone(),
                    )
                }
                PendingAction::Backup(indices) => {
                    let count = indices.len();
                    (
                        "Confirm Backup".to_string(),
                        format!(
                            "Backup {} items to ~/repos/digtwin/?\n\n(C)onfirm  |  E(x)it",
                            count
                        ),
                        indices.clone(),
                    )
                }
            };

            // Dialog keys not gated on kb_active — dialog owns the frame.
            let confirm = key_c;
            let cancel = key_x || key_esc;

            let mut open = true;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(&body);
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        if ui.button("(C)onfirm").clicked() || confirm {
                            let is_delete =
                                matches!(&self.pending_action, Some(PendingAction::Delete(_)));
                            let msg = match &self.pending_action {
                                Some(PendingAction::Delete(_)) => self.do_delete(&indices),
                                Some(PendingAction::Backup(_)) => self.do_backup(&indices),
                                None => String::new(),
                            };
                            self.last_sync_message = Some(msg);
                            self.pending_action = None;
                            if is_delete {
                                let names: Vec<String> = indices
                                    .iter()
                                    .filter_map(|&i| self.entries.get(i).map(|e| e.name.clone()))
                                    .collect();
                                self.rescan_after_delete(&names);
                            } else {
                                self.rescan_preserving_selection();
                            }
                        }
                        if ui.button("E(x)it").clicked() || cancel {
                            self.pending_action = None;
                        }
                    });
                });
            if !open {
                self.pending_action = None;
            }
            return;
        }

        // -- Sidebar keyboard nav --------------------------------------------
        self.nav_moved = false;
        if kb_active && self.focus_pane == FocusPane::Sidebar && !self.nav_order.is_empty() {
            // Don't handle plain arrows if ctrl is held (those are pane switches)
            let ctrl = ctx.input(|i| i.modifiers.ctrl);

            if key_down && !ctrl {
                if self.nav_cursor + 1 < self.nav_order.len() {
                    self.nav_cursor += 1;
                    self.sync_selection_to_cursor();
                    self.nav_moved = true;
                }
            }
            if key_up && !ctrl {
                if self.nav_cursor > 0 {
                    self.nav_cursor -= 1;
                    self.sync_selection_to_cursor();
                    self.nav_moved = true;
                }
            }
            if key_left && !ctrl {
                match &self.nav_order[self.nav_cursor] {
                    NavNode::SurfaceHeader(surface) => {
                        // Collapse this surface
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ctx,
                                surface_col_id(*surface),
                                true,
                            );
                        state.set_open(false);
                        state.store(ctx);
                    }
                    NavNode::GroupHeader { surface, label, .. } => {
                        // Collapse this group
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ctx,
                                group_col_id(*surface, label),
                                true,
                            );
                        state.set_open(false);
                        state.store(ctx);
                    }
                    NavNode::Entry(idx) => {
                        // Go to parent group/surface header
                        let entry_surface = self.entries[*idx].surface;
                        let entry_group = self.entries[*idx].group_label();
                        // Scan backward for matching GroupHeader or SurfaceHeader
                        for i in (0..self.nav_cursor).rev() {
                            match &self.nav_order[i] {
                                NavNode::GroupHeader { surface, label, .. }
                                    if *surface == entry_surface && *label == entry_group =>
                                {
                                    self.nav_cursor = i;
                                    break;
                                }
                                NavNode::SurfaceHeader(s) if *s == entry_surface => {
                                    self.nav_cursor = i;
                                    break;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            if key_right && !ctrl {
                match &self.nav_order[self.nav_cursor] {
                    NavNode::SurfaceHeader(surface) => {
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ctx,
                                surface_col_id(*surface),
                                true,
                            );
                        state.set_open(true);
                        state.store(ctx);
                    }
                    NavNode::GroupHeader { surface, label, .. } => {
                        let mut state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ctx,
                                group_col_id(*surface, label),
                                true,
                            );
                        state.set_open(true);
                        state.store(ctx);
                    }
                    NavNode::Entry(_) => {
                        // On an entry, Right switches to viewer pane
                        self.focus_pane = FocusPane::Viewer;
                    }
                }
            }
        }

        // -- Context menu popup ----------------------------------------------
        let mut context_action: Option<PendingAction> = None;
        if let Some(ref target) = self.context_target.clone() {
            let menu_id = egui::Id::new("entry_context_menu");
            let ctx_indices: Vec<usize> = match target {
                ContextTarget::Item(i) => vec![*i],
                ContextTarget::Group(_, indices) => indices.clone(),
            };
            let action_indices = self.action_indices(&ctx_indices);
            let label = match target {
                ContextTarget::Item(i) => self
                    .entries
                    .get(*i)
                    .map(|e| e.name.clone())
                    .unwrap_or_default(),
                ContextTarget::Group(name, _) => name.clone(),
            };

            let has_hooks = action_indices.iter().any(|&i| {
                self.entries
                    .get(i)
                    .map_or(false, |e| e.surface == Surface::ClaudeHook)
            });

            if kb_active && (key_b || ctrl_b) {
                context_action = Some(PendingAction::Backup(action_indices.clone()));
                self.context_target = None;
            } else if kb_active && key_d && !has_hooks {
                context_action = Some(PendingAction::Delete(action_indices.clone()));
                self.context_target = None;
            } else if kb_active && (key_x || key_esc) {
                self.context_target = None;
            }

            if self.context_target.is_some() {
                egui::Area::new(menu_id)
                    .order(egui::Order::Foreground)
                    .fixed_pos(self.context_menu_pos)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.set_min_width(200.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} ({})",
                                    label,
                                    action_indices.len()
                                ))
                                .strong(),
                            );
                            ui.separator();
                            if ui
                                .button(format!("{} (B)ackup to digtwin", icons::ARCHIVE))
                                .clicked()
                            {
                                context_action =
                                    Some(PendingAction::Backup(action_indices.clone()));
                                self.context_target = None;
                            }
                            if !has_hooks {
                                if ui
                                    .button(
                                        egui::RichText::new(format!(
                                            "{} (D)elete from disk",
                                            icons::TRASH
                                        ))
                                        .color(self.theme.error()),
                                    )
                                    .clicked()
                                {
                                    context_action =
                                        Some(PendingAction::Delete(action_indices.clone()));
                                    self.context_target = None;
                                }
                            } else {
                                ui.add_enabled(
                                    false,
                                    egui::Button::new(format!(
                                        "{} Delete (hooks not supported)",
                                        icons::TRASH
                                    )),
                                );
                            }
                            ui.separator();
                            if ui.button("E(x)it").clicked() {
                                self.context_target = None;
                            }
                        });
                    });
            }
        }

        // Keyboard B/D when no context menu is open
        if self.context_target.is_none() && self.pending_action.is_none() && kb_active {
            let target_indices = if !self.checked.is_empty() {
                let mut v: Vec<usize> = self.checked.iter().copied().collect();
                v.sort();
                Some(v)
            } else if let Some(idx) = self.selected_idx {
                Some(vec![idx])
            } else {
                None
            };

            if let Some(indices) = target_indices {
                if key_b || ctrl_b {
                    context_action = Some(PendingAction::Backup(indices));
                } else if key_d {
                    let has_hooks = indices.iter().any(|&i| {
                        self.entries
                            .get(i)
                            .map_or(false, |e| e.surface == Surface::ClaudeHook)
                    });
                    if !has_hooks {
                        context_action = Some(PendingAction::Delete(indices));
                    }
                }
            }
        }

        if let Some(action) = context_action {
            match action {
                PendingAction::Delete(_) => self.pending_action = Some(action),
                PendingAction::Backup(ref indices) => {
                    let msg = self.do_backup(indices);
                    self.last_sync_message = Some(msg);
                    self.rescan_preserving_selection();
                }
            }
            self.context_target = None;
        }

        // -- Headerbar -------------------------------------------------------
        let header_file_label = self.selected_idx.map(|i| {
            let name = &self.entries[i].name;
            if self.is_dirty() {
                format!("~{}", name)
            } else {
                name.clone()
            }
        });
        let header_dirty = self.is_dirty();
        egui::TopBottomPanel::top("headerbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("dotagent");
                ui.separator();
                if let Some(ref file_label) = header_file_label {
                    let color = if header_dirty {
                        self.theme.warning()
                    } else {
                        self.theme.view_fg()
                    };
                    ui.add(
                        egui::Label::new(egui::RichText::new(file_label).color(color))
                            .truncate(),
                    );
                } else {
                    ui.label("Agent Resources Explorer");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(format!("{} Rescan", icons::ARROWS_CLOCKWISE))
                        .clicked()
                    {
                        self.entries = scanner::scan_all();
                        self.selected_idx = None;
                        self.loaded_content = None;
                        self.checked.clear();
                        self.last_sync_message = Some("Rescanned all surfaces".to_string());
                    }
                });
            });
        });

        // -- Status bar ------------------------------------------------------
        let checked_count = self.checked.len();
        let (total, synced, modified, unbackedup) = self.status_summary();
        let is_editing = self.editing;
        let is_dirty = self.is_dirty();
        egui::TopBottomPanel::bottom("statusbar")
            .min_height(24.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if checked_count > 0 {
                        ui.strong(format!("{} selected", checked_count));
                        ui.separator();
                        if ui
                            .button(format!("{} Backup selected", icons::ARCHIVE))
                            .clicked()
                        {
                            let indices: Vec<usize> = self.checked.iter().copied().collect();
                            let msg = self.do_backup(&indices);
                            self.last_sync_message = Some(msg);
                            self.rescan_preserving_selection();
                        }
                        let has_hooks = self.checked.iter().any(|&i| {
                            self.entries
                                .get(i)
                                .map_or(false, |e| e.surface == Surface::ClaudeHook)
                        });
                        if !has_hooks {
                            if ui
                                .button(
                                    egui::RichText::new(format!(
                                        "{} Delete selected",
                                        icons::TRASH
                                    ))
                                    .color(self.theme.error()),
                                )
                                .clicked()
                            {
                                let indices: Vec<usize> = self.checked.iter().copied().collect();
                                self.pending_action = Some(PendingAction::Delete(indices));
                            }
                        }
                        if ui.button("Clear selection").clicked() {
                            self.checked.clear();
                        }
                        ui.separator();
                    }

                    // Left section: pane + counts
                    let pane_label = match self.focus_pane {
                        FocusPane::Sidebar => "SIDEBAR",
                        FocusPane::Viewer => "VIEWER",
                    };
                    ui.label(
                        egui::RichText::new(format!("[{}]", pane_label))
                            .small()
                            .weak(),
                    );
                    ui.separator();
                    ui.label(format!("{} items", total));
                    ui.separator();
                    ui.colored_label(self.theme.success(), format!("{} synced", synced));
                    ui.separator();
                    ui.colored_label(self.theme.warning(), format!("{} modified", modified));
                    ui.separator();
                    ui.colored_label(self.theme.error(), format!("{} unbackedup", unbackedup));

                    // Center: editing indicator (right-aligned to fill remaining space)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(ref msg) = self.last_sync_message {
                            ui.label(egui::RichText::new(msg).small().weak());
                        }
                        if is_editing {
                            let edit_text = if is_dirty { "~ EDITING" } else { "EDITING" };
                            let color = if is_dirty {
                                self.theme.warning()
                            } else {
                                self.theme.accent()
                            };
                            ui.label(egui::RichText::new(edit_text).strong().color(color));
                        }
                    });
                });
            });

        // -- Sidebar ---------------------------------------------------------
        // Snapshot the cursor node for highlight matching during render
        let cursor_node = self.nav_order.get(self.nav_cursor).cloned();
        let nav_cursor_entry = cursor_node.as_ref().and_then(|n| n.entry_index());
        let cursor_color = self.theme.accent();

        // -- Focus outlines ----------------------------------------------------
        let focus_stroke = egui::Stroke::new(2.0, self.theme.accent());
        let no_stroke = egui::Stroke::NONE;
        let sidebar_frame = egui::Frame::side_top_panel(ctx.style().as_ref()).stroke(
            if self.focus_pane == FocusPane::Sidebar {
                focus_stroke
            } else {
                no_stroke
            },
        );
        let viewer_frame = egui::Frame::central_panel(ctx.style().as_ref())
            .fill(self.theme.view_bg())
            .stroke(if self.focus_pane == FocusPane::Viewer {
                focus_stroke
            } else {
                no_stroke
            });

        egui::SidePanel::left("sidebar")
            .default_width(300.0)
            .resizable(true)
            .frame(sidebar_frame)
            .show(ctx, |ui| {
                // Click in sidebar → focus sidebar
                if ui.rect_contains_pointer(ui.max_rect())
                    && ctx.input(|i| i.pointer.any_click())
                {
                    self.focus_pane = FocusPane::Sidebar;
                }

                ui.horizontal(|ui| {
                    ui.label("Filter:");
                    let filter_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .id(filter_id)
                            .desired_width(ui.available_width()),
                    );
                    // Show hint when not focused and empty
                    if !filter_resp.has_focus() && self.filter.is_empty() {
                        let rect = filter_resp.rect;
                        ui.painter().text(
                            rect.left_center() + egui::vec2(4.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            "Ctrl+S to search",
                            egui::FontId::proportional(12.0),
                            self.theme.shade(),
                        );
                    }
                });
                ui.add_space(8.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut new_selection = None;
                    let mut new_context_target = self.context_target.clone();

                    // -- Grouped surfaces ------------------------------------
                    for surface in [
                        Surface::ClaudeSkill,
                        Surface::AgentSkill,
                        Surface::PiExtension,
                        Surface::ClaudeHook,
                    ] {
                        let groups = self.grouped_surface_entries(surface);
                        let total: usize = groups.iter().map(|(_, v)| v.len()).sum();
                        let header = format!("{} ({})", surface.label(), total);
                        let is_cursor_here = cursor_node
                            .as_ref()
                            .map_or(false, |n| n.is_surface(surface));

                        let surface_state =
                            egui::collapsing_header::CollapsingState::load_with_default_open(
                                ctx,
                                surface_col_id(surface),
                                matches!(surface, Surface::AgentSkill | Surface::PiExtension),
                            );
                        let nav_moved = self.nav_moved;
                        surface_state
                            .show_header(ui, |ui| {
                                let resp = ui.label(egui::RichText::new(&header).strong());
                                if is_cursor_here {
                                    paint_cursor_bar(ui, &resp, cursor_color);
                                    if nav_moved {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                }
                            })
                            .body(|ui| {
                                for (group_label, item_indices) in &groups {
                                    let all_checked = !item_indices.is_empty()
                                        && item_indices
                                            .iter()
                                            .all(|i| self.checked.contains(i));
                                    let group_header = format!(
                                        "{} ({})",
                                        group_label,
                                        item_indices.len()
                                    );
                                    let default_open = group_label == "Personal"
                                        || item_indices.len() <= 10;

                                    let group_state =
                                        egui::collapsing_header::CollapsingState::load_with_default_open(
                                            ctx,
                                            group_col_id(surface, group_label),
                                            default_open,
                                        );
                                    let is_cursor_group = cursor_node
                                        .as_ref()
                                        .map_or(false, |n| n.is_group(surface, group_label));
                                    group_state
                                        .show_header(ui, |ui| {
                                            let mut group_checked = all_checked;
                                            if ui
                                                .checkbox(&mut group_checked, "")
                                                .changed()
                                            {
                                                for &i in item_indices {
                                                    if group_checked {
                                                        self.checked.insert(i);
                                                    } else {
                                                        self.checked.remove(&i);
                                                    }
                                                }
                                            }
                                            let resp = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(&group_header)
                                                        .italics(),
                                                )
                                                .sense(egui::Sense::click()),
                                            );
                                            if is_cursor_group {
                                                paint_cursor_bar(ui, &resp, cursor_color);
                                                if nav_moved {
                                                    resp.scroll_to_me(Some(egui::Align::Center));
                                                }
                                            }
                                            if resp.secondary_clicked() {
                                                new_context_target =
                                                    Some(ContextTarget::Group(
                                                        group_label.clone(),
                                                        item_indices.clone(),
                                                    ));
                                            }
                                        })
                                        .body(|ui| {
                                            let items_for_list: Vec<(usize, &SkillEntry)> =
                                                item_indices
                                                    .iter()
                                                    .filter_map(|&i| {
                                                        self.entries
                                                            .get(i)
                                                            .map(|e| (i, e))
                                                    })
                                                    .collect();
                                            Self::render_entry_list(
                                                ui,
                                                &items_for_list,
                                                self.selected_idx,
                                                nav_cursor_entry,
                                                self.nav_moved,
                                                &self.theme,
                                                &mut self.checked,
                                                &mut new_selection,
                                                &mut new_context_target,
                                            );
                                        });
                                }
                            });
                    }

                    // Capture pointer position when context menu first opens
                    let had_context = self.context_target.is_some();
                    self.context_target = new_context_target;
                    if self.context_target.is_some() && !had_context {
                        self.context_menu_pos = ctx.input(|i| {
                            i.pointer.interact_pos().unwrap_or(egui::Pos2::ZERO)
                        });
                    }

                    // Mouse click selection (with dirty guard)
                    if let Some(idx) = new_selection {
                        if self.is_dirty() {
                            self.dirty_nav_target = Some(DirtyNavTarget::Entry(idx));
                        } else {
                            if self.editing {
                                self.exit_edit_mode(false);
                            }
                            self.select_entry(idx);
                            self.sync_cursor_to_entry(idx);
                            self.focus_pane = FocusPane::Sidebar;
                        }
                    }
                });
            });

        // -- Viewer pane -----------------------------------------------------
        egui::CentralPanel::default()
            .frame(viewer_frame)
            .show(ctx, |ui| {
                // Click in viewer → focus viewer
                if ui.rect_contains_pointer(ui.max_rect()) && ctx.input(|i| i.pointer.any_click()) {
                    self.focus_pane = FocusPane::Viewer;
                }

                if let Some(idx) = self.selected_idx {
                    // Copy entry data upfront to avoid borrow conflicts in closures
                    let entry_name = self.entries[idx].name.clone();
                    let entry_path = self.entries[idx].path.clone();
                    let entry_sync = self.entries[idx].sync_status;
                    let entry_group = self.entries[idx].group_label();

                    // -- Line 1: Title | Dirty/Sync status --
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&entry_name).heading())
                                .truncate(),
                        );
                        if self.is_dirty() {
                            ui.label(
                                egui::RichText::new(" ~")
                                    .strong()
                                    .color(self.theme.warning()),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let status_text = match entry_sync {
                                SyncStatus::Synced => "Synced",
                                SyncStatus::Modified => "Modified",
                                SyncStatus::Unbackedup => "No backup",
                                SyncStatus::Unknown => "Unknown",
                            };
                            ui.colored_label(self.sync_color(entry_sync), status_text);
                        });
                    });

                    // -- Line 2: Path --
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(entry_path.display().to_string())
                                    .small()
                                    .weak(),
                            )
                            .truncate(),
                        );
                        if entry_group != "Unknown source" {
                            ui.label(
                                egui::RichText::new(format!("  ({})", entry_group))
                                    .small()
                                    .weak(),
                            );
                        }
                    });

                    ui.add_space(4.0);

                    // -- Line 3: Action buttons with icons --
                    ui.horizontal(|ui| {
                        let editing = self.editing;
                        // (E)dit / View toggle
                        if !editing {
                            if ui
                                .button(format!("{} (E)dit", icons::PENCIL_SIMPLE))
                                .clicked()
                            {
                                self.enter_edit_mode();
                            }
                        } else {
                            if ui
                                .button(format!("{} (E)dit", icons::PENCIL_SIMPLE))
                                .clicked()
                            {
                                // Already editing — treat as no-op or exit
                                if !self.is_dirty() {
                                    self.exit_edit_mode(false);
                                }
                            }
                        }

                        // (S)ave
                        let save_enabled = editing && self.is_dirty();
                        if ui
                            .add_enabled(
                                save_enabled,
                                egui::Button::new(format!("{} (S)ave", icons::FLOPPY_DISK)),
                            )
                            .clicked()
                        {
                            self.exit_edit_mode(true);
                        }

                        // (B)ackup
                        if ui.button(format!("{} (B)ackup", icons::ARCHIVE)).clicked() {
                            match crate::sync::backup_entry(&self.entries[idx]) {
                                Ok(dest) => {
                                    self.last_sync_message = Some(format!(
                                        "Backed up '{}' → {}",
                                        entry_name,
                                        dest.display()
                                    ));
                                    self.rescan_preserving_selection();
                                }
                                Err(e) => {
                                    self.last_sync_message = Some(format!("Error: {}", e));
                                }
                            }
                        }
                    });
                    ui.separator();

                    // -- Content area --
                    let scroll_delta = self.viewer_scroll_delta;
                    let scroll_out = egui::ScrollArea::vertical().show(ui, |ui| {
                        if self.editing {
                            let editor_id = egui::Id::new("viewer_editor");
                            let highlighter = SyntaxHighlighter::get();
                            let path_for_layout = entry_path.clone();
                            let mut layouter =
                                move |ui: &egui::Ui,
                                      text: &dyn egui::TextBuffer,
                                      wrap_width: f32| {
                                    let mut job =
                                        highlighter.highlight(text.as_str(), &path_for_layout);
                                    job.wrap.max_width = wrap_width;
                                    ui.fonts_mut(|fonts| fonts.layout_job(job))
                                };
                            let te_resp = ui.add(
                                egui::TextEdit::multiline(&mut self.edit_buffer)
                                    .id(editor_id)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .code_editor()
                                    .layouter(&mut layouter),
                            );
                            if self.editor_wants_focus {
                                te_resp.request_focus();
                                self.editor_wants_focus = false;
                            }
                        } else if let Some(ref content) = self.loaded_content {
                            let highlighter = SyntaxHighlighter::get();
                            let job = highlighter.highlight(content, &entry_path);
                            ui.add(egui::Label::new(job).selectable(true));
                        }
                    });
                    // Apply keyboard scroll delta
                    if scroll_delta != 0.0 {
                        let new_offset = (scroll_out.state.offset.y + scroll_delta).max(0.0);
                        let mut state = scroll_out.state;
                        state.offset.y = new_offset;
                        state.store(ui.ctx(), scroll_out.id);
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Select a skill, hook, rule, or extension to view");
                    });
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

fn packaged_font_candidates(name: &str) -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            candidates.push(bin_dir.join("../share/dotagent/fonts").join(name));
            candidates.push(bin_dir.join("../fonts").join(name));
        }
    }

    candidates.push(std::path::PathBuf::from("fonts").join(name));
    candidates
}

fn read_packaged_font(name: &str) -> Option<Vec<u8>> {
    packaged_font_candidates(name)
        .into_iter()
        .find_map(|path| std::fs::read(path).ok())
}

fn load_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Some(mono_data) = read_packaged_font("spline-sans-mono-latin-400-normal.ttf") {
        fonts.font_data.insert(
            "SplineSansMono".to_owned(),
            egui::FontData::from_owned(mono_data).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "SplineSansMono".to_owned());
    }

    let has_mono_bold =
        if let Some(mono_bold_data) = read_packaged_font("spline-sans-mono-latin-700-normal.ttf")
    {
        fonts.font_data.insert(
            "SplineSansMono-Bold".to_owned(),
            egui::FontData::from_owned(mono_bold_data).into(),
        );
        // Register bold mono as its own family for headings
        fonts
            .families
            .entry(egui::FontFamily::Name("MonoBold".into()))
            .or_default()
            .push("SplineSansMono-Bold".to_owned());
        true
    } else {
        false
    };

    if let Some(sans_data) = read_packaged_font("spline-sans-latin-400-normal.ttf") {
        fonts.font_data.insert(
            "SplineSans".to_owned(),
            egui::FontData::from_owned(sans_data).into(),
        );
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "SplineSans".to_owned());
    }

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    ctx.set_fonts(fonts);

    // Text style sizes per design-basics.md:
    //   Menu items: min 16px, Other UI text: min 13px
    //   Monospace bold for headings/strong labels, sans for body/menus
    use egui::{FontFamily, FontId, TextStyle};
    let mut style = (*ctx.style()).clone();
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(
            20.0,
            if has_mono_bold {
                FontFamily::Name("MonoBold".into())
            } else {
                FontFamily::Monospace
            },
        ),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(14.0, FontFamily::Monospace),
    );
    ctx.set_style(style);
}
