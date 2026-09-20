# Retrospective

- Reusing the package metadata store preserved Skill paths and IDE links.
- Enforcing single ownership in the backend prevents UI or stale-client mistakes from creating duplicate membership.
- A virtual Uncategorized group avoids creating special persistent IDs and naturally covers newly created/imported Skills.
- Keeping display order separate from immutable UUIDs allows one-based numbering and drag reordering without changing identity.
- The Projects module and project install branch were removed end-to-end, reducing the frontend bundle and navigation surface.
