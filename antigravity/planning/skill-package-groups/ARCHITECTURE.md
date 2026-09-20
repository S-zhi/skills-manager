# Architecture

The existing `.metadata/skill-packages.json` store remains authoritative. Schema v2 adds package `position`; save operations remove assigned Skill UUIDs from every other package before updating the target. A reorder command persists the complete package-ID order under revision control.

`LocalPanel.vue` owns presentation state and loads the package store. It derives virtual Uncategorized plus ordered persisted groups, renders the existing Skill cards inside each group, and invokes package commands for mutations. Expansion state stays local because it is presentation-only.

Project UI/state imports are removed from `App.vue`. The install modal becomes IDE-only while the existing backend project helpers remain untouched to avoid an unrelated destructive migration.
