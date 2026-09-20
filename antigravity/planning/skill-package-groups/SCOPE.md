# Scope

## In scope

- Remove Projects and Skill packages tabs.
- Create, rename, expand/collapse, reorder, and delete packages in My Skills.
- Assign one or many selected Skills to exactly one package or Uncategorized.
- Assign a single Skill from its More menu.
- Default newly created/imported Skills to Uncategorized.
- Persist UUID, display order, name, description, membership, and timestamps.

## Out of scope

- Moving Skill files on disk when grouping.
- Nested packages.
- Synchronizing UI expansion state across devices.

## Acceptance criteria

- No Projects or standalone package navigation is rendered.
- Every visible Skill is under exactly one group.
- Package numbering starts at 1 and follows drag order.
- A Skill cannot remain in two package records after any save.
- Existing multi-package data migrates deterministically to one owner.
- Build and full Rust test suite pass.
