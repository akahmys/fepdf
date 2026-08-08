---
name: codebase-exploration
description: >-
  Standard procedure for exploring the Ferruginous codebase using
  conceptual and mechanical analysis. Use when investigating code
  structure, finding implementations, or preparing for changes.
---

# Codebase Exploration Protocol

Standard procedure for exploring the Ferruginous codebase using conceptual and mechanical analysis.

---

## 1. Search Hierarchy

1. **Conceptual Discovery (`ccc search`)**: Use semantic vector search for high-level exploration of concepts (e.g., "font reconstruction," "handle stability").
2. **Identifier Pinpointing (`grep_search`)**: Use literal search for specific constants, trait names, or error variants identified in Phase 1.
3. **AST Verification (`view_file`)**: Directly inspect the implementation to verify logical flow and borrow-checker constraints.

## 2. Mechanical Accuracy Guard

To prevent execution failures and preserve turn efficiency:

- **Literal Matching**: `TargetContent` for file edits MUST be a 100% exact character-for-character copy of the source file.
- **Sanitization**: Line numbers and diagnostic prefixes from tools MUST be stripped before creating a patch.
- **Whitespace Fidelity**: Trailing spaces and indentation MUST be preserved exactly to avoid breaking the tool's matching logic.

## 3. Findings Externalization

- Discoveries made via semantic search MUST be recorded in the `implementation_plan.md` to bridge the gap between "concept" and "code."
- Non-obvious conceptual links found during discovery must be formalized as comments or documentation updates.
