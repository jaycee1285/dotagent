# Smoke Human 2026-03-17

## What Changed
- Added syntax highlighting in raw `(e)dit` mode using the existing tmTheme-backed syntect pipeline.
- Extended skill backup so hard-coded external `/home/john/...` `.js` / `.ts` script paths referenced from `SKILL.md` are copied into the backup tree under `_external/`.

## Command

```sh
nix develop -c cargo run
```

## What John Tested
- Entered edit mode on a skill/rule and checked that the editable text remained syntax highlighted.
- Backed up a Codex skill with hard-coded external script paths (`bookmarktriage` / `insieve` workflow) and inspected the resulting digtwin backup.

## John Observed
- Human smoke passed.
- No notes.
- Follow-up comment after the external script backup check: "Smoke success. That's a big one, thank you."

## Passed
- Edit-mode syntax highlighting worked in the running app.
- External hard-coded skill-script backup capture worked.

## Failed Or Suspicious
- None reported by John in this session.

## Current Gaps
- Hook UX cleanup is still pending.
- Yellow mismatch indicators are still planned work, not shipped behavior.
