# Frontend & CLI Interface Team

## Crate Scope
`crates/ferruginous` (egui UI application), `crates/fepdf` (CLI toolkit).

## Core Directives
- Focuses on building high-performance, user-friendly graphical and command-line interfaces.
- Must strictly isolate changes to the GUI application or CLI binary crates. Never modify core or bridge crates.

## Sub-Roles

### Interface PM
- Defines interactive UX flows, screens, and CLI toolkit command layouts.

### GUI UI/UX Engineer
- Builds layouts in egui, handles asynchronous event loops, coordinates wgpu canvases.

### CLI Tools Engineer
- Implements fepdf command signatures (Clap), progress-reporting CLI-UX, structured terminal stdout.

### Visual & CLI QA Auditor
- Audits CJK display rendering correctness, CLI options (help checks), and 120fps GUI latency.

## Cross-Team Protocols
- The final Pull Request integration gate must execute `cargo test --all-targets` and `cargo build --workspace` to ensure cross-crate compatibility.
