# ADR-0130: The secret scan had no rules

- **Status**: Accepted, except the history question at the end, which is the owner's
- **Date**: 2026-10-13
- **Commit**: (see the commit that adds this file)

## Context

Rule 18 ran `betterleaks dir .` and passed. Measured on 2026-10-13, `betterleaks config
check` reported **0 rules**: `.betterleaks.toml` held a title and an allowlist and no
`[extend]`, and betterleaks loads only a config's own rules unless it extends the
defaults. A token in GitHub's `ghp_` format, placed in a scratch directory beside that
config, was reported clean; with no config it was found. So every scan since the config
took that shape passed every file without looking.

Two more things surfaced on the way.

- `AUDITING.md` described a personal-name rule in the config. The config held one until
  `6578bc4` took the file out of tracking on 2026-08-18; the file tracked again later did
  not. The description quoted the name as a regular expression, so the document that
  claimed to protect the name was itself a place it was committed. `49befc1` removed that
  description. The commits before it carry the name, and they are on the public remote.
- The pre-commit hook lived untracked in `.git/hooks/` and exited 0 when `betterleaks`
  was missing, so on a machine without the tool nothing was scanned and nothing said so.
  Nothing scanned commit messages.

With the defaults switched on, the tree gave 8 findings, all `generic-password`: six
interface labels in the two locale files, and the two fixed passwords of the encryption
cross-checks. None was a secret.

## Decision

- `.betterleaks.toml` extends the defaults, and the audit fails when fewer than 100 rules
  load, so the shape that hid this fails loudly if it returns.
- The eight findings are allowlisted by file and by value, for `generic-password` only;
  a real token in the same files was shown to be found.
- The hooks are tracked in `scripts/hooks/`, fail closed, and a `commit-msg` hook scans
  the message.
- Personal data is matched by rules outside every repository
  (`~/.config/betterleaks/personal.toml`), read by the hooks and the audit when present.
  A rule naming a person cannot be committed without committing the name.
- The audit's personal pass reads history from `49befc1`, after the name left the tree.
  Full history gives 3 findings; from `49befc1`, none.

## Open: the history

The name is in public history. Removing it means rewriting every commit since 2026-08-09
and force-pushing `main`, which changes every hash, breaks every clone, and does not
reach copies GitHub has cached or anyone has forked; GitHub support can be asked to purge
cached views. That is the owner's decision, not this record's. Until it is made, the
audit starts at `49befc1` and this section is where the reason is written.
