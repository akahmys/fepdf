# ADR-0004: Rule B makes the GPU dependency explicit, not absent

- **Status**: Accepted, correcting a claimed consequence
- **Date**: 2026-08-10
- **Commit**: `a8fd15a`

## Context

Rule B — a crate defining a contract does not depend on its implementations — was
motivated by `RenderBackend` being defined in `fepdf-render`, the Vello/wgpu crate,
while two of its three implementations lived in `fepdf-sdk`. The SDK therefore
depended on a GPU crate to obtain a trait definition, and every SDK consumer
inherited `vello` + `wgpu` transitively.

The migration plan stated the effect as "drops `vello`/`wgpu` from MCP and WASM".
Checking before implementing showed that to be wrong: `fepdf-mcp` has a render tool
and calls `render_page_to_file`. It rasterises, so it needs the GPU stack.

## Decision

Move the contract into `fepdf-content` and make rasterisation an opt-in feature of
the SDK rather than a transitive certainty. `fepdf-cli` and `fepdf-mcp` enable it
because they genuinely rasterise; `fepdf-wasm` does not.

Rule B's effect is stated as making the dependency **explicit and chosen**, not
absent. Measured:

```
fepdf-wasm     3 transitive GPU dependencies -> 0
fepdf-content                                   0
fepdf-cli / fepdf-mcp                           3, opted in
```

## Consequences

- A consumer that only interprets content streams — text extraction, geometry
  collection — links no GPU stack. That was the real objective; "MCP loses wgpu" was
  never it.
- `--features render` is now part of how the CLI and MCP are built, including in the
  `debug-tools` feature chain, which forwards through the SDK.
- The correction is worth recording mainly because the claim was written into the
  architecture document as a fact before it was checked. The same pattern produced
  ADR-0001, ADR-0002 and ADR-0005.
