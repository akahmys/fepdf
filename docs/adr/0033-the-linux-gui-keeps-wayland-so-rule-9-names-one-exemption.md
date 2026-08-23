# ADR-0033: The Linux GUI keeps Wayland, so Rule 9 names one exemption

- **Status**: Accepted
- **Date**: 2026-08-23
- **Commit**: cb58ca4

## Context

Rule 9 forbids a dependency that compiles C. Its check ran `cargo tree` with **no
`--target`**, so it answered "does this compile C on the machine running the audit" while
`CODING.md` called it exact. Widening it to the four targets this engine is built for
found a violation on the first run:

```text
cc v1.2.60
[build-dependencies]
└── wayland-backend v0.3.15
    ├── ashpd → rfd → fepdf-gui
    └── calloop-wayland-source → smithay-client-toolkit → winit → eframe → fepdf-gui
```

**This was not new. It was newly visible.** A Linux GUI build has compiled C for as long
as the GUI has had a Linux target, and Rule 9 reported `PASS` throughout.

`wayland-backend` is the only crate pulling `cc`, and `fepdf-gui` reaches it three ways —
through the file dialog and twice through the windowing stack. Removing one path buys
nothing, which is what makes this a decision about Wayland rather than about a dependency.

`cc` appears on **none** of Windows, macOS or wasm. This is one platform's backend, not a
habit.

## Decision

**The Linux GUI keeps Wayland.** An X11-only Linux GUI in 2026 is a worse product than a
rule kept clean, and Rule 9 exists to keep unaudited C out of the *engine* — the reason
it gives is that `cargo clippy`, the `unsafe` ban and the rest of RR-15 stop at the
language boundary. A display-server shim reached only by the desktop application is the
furthest thing from that reason, and the four crates below the facade compile no C on any
target.

**The exemption names `wayland-backend`, not `fepdf-gui(linux)`**, and that is the part
worth recording. Exempting the *member* would forgive whatever it acquires next: the GUI
would be free to take another C-compiling dependency and the check would keep saying
`PASS`. Naming the *cause* forgives Wayland and nothing else. Verified by removing the
exemption and watching the check name the culprit:
`fepdf-gui(x86_64-unknown-linux-gnu):wayland-backend`.

## Consequences

Rule 9 is now a stronger check with one written-down hole, where it was a weaker check
with an unwritten one. `CODING.md` carries the exemption with its reason, in the shape
Rule 5's exemptions already take.

**A target added to `RULE9_TARGETS` is a claim that the engine is built for it.** Haiku is
deliberately absent: `--target all` finds `chrono` → `iana-time-zone` →
`iana-time-zone-haiku`, and ADR-0024 drew the line at whether a build compiles foreign
source — that one never does on a platform anyone here builds for.

**The pattern is the same one this month keeps producing.** A check that names its places
keeps passing while what it should be checking grows beside it: the `Decision` row missed
`fepdf-render`, `verify_compliance.sh` missed `fepdf-script`, the stub row named two
crates, and Rule 9 named one target. Four instances, one shape — and the fix each time was
to derive the scope rather than list it.
