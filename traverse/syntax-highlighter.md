---
id: syntax-highlighter
kind: module
authority:
  - ~/.config/syntect/current.tmTheme
mutates: []
observes:
  - source-file-extension
  - file-content
  - syntect-theme-file
persists_to: []
depends_on:
  - syntect-defaults
staleness_risks:
  - once-lock-theme-cache
entrypoints:
  - src/syntax.rs
  - src/app.rs
---

# Syntax Highlighter

## Purpose
Owns syntax-colored rendering for both read mode and raw edit mode. It builds `egui::text::LayoutJob` output from file content using syntect and the user’s current `~/.config/syntect/current.tmTheme`.

## Scope of Touch
Safe to edit when changing:
- extension-to-syntax matching
- text formatting produced for egui
- editor/viewer syntax rendering behavior

Risky to edit when changing:
- theme loading and fallback behavior
- global caching assumptions
- any path-dependent syntax inference used by the editor

## Authority Notes
The theme file on disk is the visual source of truth, but the loaded theme is cached in a `OnceLock` for the lifetime of the process. A theme change on disk will not be reflected until the app restarts.

## Links
- [App Shell](app-shell.md)
- [GTK Theme Loader](gtk-theme-loader.md)
