# Acrobat Pro feature list (third-party reference)

[`acrobat-features.toml`](acrobat-features.toml) is PrintCraft's checklist of Acrobat Pro's
offline feature set: 827 entries in 14 areas (A core … N misc), each with a tier (P0–P3), the
tier `N/A` marking the 23 that are Adobe-cloud-only. It is the reference to consult when
fepdf adds a feature, not a plan.

- **Source:** `parity/acrobat-features.toml` in
  [storytold/printcraft](https://github.com/storytold/printcraft), commit
  `fdfdc879d7f21f51bfd651169093643335a5298a` (v0.2.0 plus later commits, copied 2026-10-06).
- **Licence:** MIT OR Apache-2.0; the MIT notice is in [`LICENSE-MIT`](LICENSE-MIT).
- **Modified:** the fields that describe PrintCraft rather than Acrobat — `status`,
  `evidence`, `commands`, `tools`, `milestone`, `notes` — are removed, with `statuses` from
  the schema table and the header comment that explained them. `id`, `title`, `area` and
  `tier` are as published.
