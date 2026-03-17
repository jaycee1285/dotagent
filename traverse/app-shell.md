---
id: dotagent-app-shell
kind: ui-surface
authority:
  - scan-all-result
  - selected-entry-state
mutates:
  - selected-entry-state
  - edit-buffer
  - focus-pane
  - pending-action-state
observes:
  - gtk-theme-colors
  - syntax-highlighter
  - scanner-surfaces
  - sync-status
persists_to:
  - skill-files-on-disk
  - digtwin-backup-tree
depends_on:
  - scanner-discovery
  - sync-boundary
  - syntax-highlighter
  - gtk-theme-loader
staleness_risks:
  - in-memory-loaded-content
  - nav-order-derived-from-collapse-state
entrypoints:
  - src/app.rs
  - src/main.rs
---

# Dotagent App Shell

## Purpose
Owns the egui application state and the main operator workflow: scan entries, navigate grouped surfaces, view content, edit content, and trigger backup/delete actions. This is the coordination surface where scanner results, sync status, theme colors, and syntax highlighting get turned into the actual UI.

## Scope of Touch
Safe to edit when changing:
- sidebar navigation behavior
- viewer/header/status bar presentation
- keyboard shortcuts and focus rules
- edit-mode UX

Risky to edit when changing:
- selection preservation across rescans/deletes
- save/dirty guard behavior
- action routing to backup/delete
- any logic that assumes scanner/sync semantics

## Authority Notes
This surface is authoritative for transient UI state only. It does not own source-of-truth skill content, hook content, or backup truth; those live on disk and in the scanner/sync modules.

## Links
- [Scanner Discovery](scanner-discovery.md)
- [Sync Boundary](sync-boundary.md)
- [Syntax Highlighter](syntax-highlighter.md)
- [GTK Theme Loader](gtk-theme-loader.md)
