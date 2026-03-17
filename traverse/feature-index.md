---
id: dotagent-feature-index
kind: index
authority: []
mutates: []
observes:
  - traverse-node-docs
persists_to: []
depends_on:
  - dotagent-app-shell
  - scanner-discovery
  - sync-boundary
  - syntax-highlighter
  - gtk-theme-loader
staleness_risks: []
entrypoints:
  - traverse/app-shell.md
  - traverse/scanner-discovery.md
  - traverse/sync-boundary.md
  - traverse/syntax-highlighter.md
  - traverse/gtk-theme-loader.md
---

# Dotagent Feature Index

## Purpose
Provides a quick locality map for later agents so they can choose the right neighborhood without re-walking the whole repo.

## Feature Neighborhoods
- Browse, select, edit, and trigger actions: [App Shell](app-shell.md)
- Build the cross-surface entry model and provenance grouping: [Scanner Discovery](scanner-discovery.md)
- Backup/delete operations and green-yellow-red status meaning: [Sync Boundary](sync-boundary.md)
- Read/edit syntax coloring from syntect: [Syntax Highlighter](syntax-highlighter.md)
- Device theme ingestion from GTK CSS: [GTK Theme Loader](gtk-theme-loader.md)

## Notes
- The highest-risk authority boundary is the sync layer because it defines both the backup layout and status semantics.
- The highest-risk staleness surfaces are startup-cached theme/highlighter state and any scanner projection derived from a filesystem that changes after launch.
