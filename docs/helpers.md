# Named helper scripts (Phase 7, Stage 1)

**Status:** implemented 2026-11-23 as the safe first slice of Phase 7. Helpers are
owner-registered scripts. Synthetic input and model-driven helper selection are not part
of this slice.

## Where they live

```sh
buddy helper dir
```

The path is `$BUDDY_DATA_DIR/helpers` (normally `~/.local/share/buddy/helpers`). Put one
`.json` manifest per helper there. Buddy never creates a helper; the owner writes the
manifest and owns the script.

## Manifest

```json
{
  "name": "project-tests",
  "description": "Run the tests in a project folder",
  "arguments": ["path"],
  "command": ["/usr/bin/cargo", "test"],
  "cwd": "{path}",
  "timeout_secs": 300,
  "risk": "changes",
  "dry_run": ["/usr/bin/cargo", "test", "--no-run"]
}
```

- `command` is an argv array, never a shell string. Arguments are separate.
- `{name}` placeholders are replaced only with declared argument values.
- `cwd` is optional and must be inside the helper folder or an approved `buddy folders`
  directory. A relative command path is resolved against the manifest folder.
- `timeout_secs` is 1–600; the process is killed when it expires.
- `risk` is informational (`looks`, `changes`, `sensitive`); execution is gated by the
  sensitive `run_helper` skill, so **every helper run asks first**.
- `dry_run` is optional. Without it, the dry run prints the real command.

## Commands

```sh
buddy helpers                         # list names, arguments and descriptions
buddy helper show NAME                # print the manifest
buddy helper dry-run NAME --arg k=v   # show the argv and cwd; execute nothing
buddy helper run NAME --arg k=v       # asks for a yes, then runs without a shell
buddy approve                         # approve the pending helper run
buddy result                          # full captured output
```

Output is captured and trimmed to 4000 characters; the journal records the helper name
and outcome, not the full output. A helper has no automatic undo.

## Security boundary

- No shell, no `eval`, no command string interpolation.
- The executable is resolved and must be a real file.
- The working directory is confined to the helper folder or the approved folders.
- Helper runs always ask; `run_helper` is a sensitive skill, so it cannot be set to
  `allow`.
- A helper can still do whatever its script does. Only register scripts you wrote or read.
## Model selection

The local model receives one tool per registered helper (`helper_0`, `helper_1`, …) with
that helper's declared argument schema and description. A selected helper is converted
to the sensitive `run_helper` skill; it still waits for the user's yes. The model never
receives a shell command tool or an approval bypass. Synthetic input remains a later,
separate decision.
