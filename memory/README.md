# Project memory

This directory is the durable, versioned memory for Claude Code sessions working on
Buddy. It is tracked in git so it survives session restarts, scratch-directory wipes,
machine changes and fresh clones.

- `MEMORY.md` is the index: one line per memory, with a relative link.
- Every other `.md` file holds one fact and starts with YAML frontmatter: `name`,
  `description`, and `metadata.type` (`user`, `feedback`, `project` or `reference`).
- Write in English. Do not store secrets, credentials, tokens or personal data.
- Keep each entry short and link related entries with `[[name]]`.
- The longer-form project record lives in `docs/decisions.md` (binding decisions and
  standing requirements) and `docs/development.md` (machine, tools and working
  agreements). Prefer those for detail and this directory for recall-sized facts.
- Claude Code's per-project memory directory under `~/.claude/projects/` keeps a pointer
  to this directory; the content here is authoritative.

## Adding a memory

1. Create `memory/<kebab-case-name>.md` with the frontmatter above.
2. Add one line to `memory/MEMORY.md`.
3. Update or delete an existing entry instead of creating a duplicate.
4. Never record what the code, `CLAUDE.md` or git history already records, unless the
   non-obvious reason behind it is the point.
