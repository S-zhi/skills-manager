# Skill package groups

Move Skill packages into the My Skills page as ordered, collapsible groups. Remove the Projects module and standalone package tab while preserving managed Skill files and existing package data.

## Decisions

- Package membership is metadata only; Skill directories are not moved.
- `Uncategorized` is a virtual group and has no persisted package record.
- A Skill UUID belongs to at most one persisted package.
- Package IDs remain immutable UUIDs; visible package numbers are one-based display order.
- Dragging the blue handle reorders persisted packages and never reorders/deletes Skills.
- Deleting a package returns its members to Uncategorized.

## Delivery

1. Upgrade package persistence and enforce single membership/order.
2. Replace the standalone package UI with grouped My Skills UI.
3. Remove Projects navigation, modals, state, and project install choice.
4. Add migration and behavior tests; run frontend and Rust verification.
