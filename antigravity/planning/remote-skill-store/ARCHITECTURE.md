# Architecture

Each provider is implemented as a separate Rust command and response parser. The Vue store selects exactly one provider per search and caches first-page results for ten minutes. Remote records carry separate download and detail URLs so hosted ClawHub ZIP artifacts can be installed without turning the View action into a download.

Provider behavior:

- ClawHub: anonymous `/api/v1/search`, native ClawHub results only, hosted ZIP download endpoint.
- SkillsMP: anonymous documented search API with pagination and daily-quota metadata.
- skills.sh: anonymous compatibility `/api/search`, GitHub repository downloads, no pagination guarantee.
