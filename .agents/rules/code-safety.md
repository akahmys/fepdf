# Reliable Rust-15 (RR-15) Rulebook

> [!IMPORTANT]
> A set of 15 immutable safety constraints derived from NASA's "Power of 10," optimized for the Rust ownership model. These rules form the mandatory "Hardening Layer" of Ferruginous.

---

## 1. Function Complexity & Scale
- **Rule**: Limit effective logic to 50 lines.
- **Purpose**: Maintain precision of the borrow checker and minimize cognitive load.
- **Compliance**: All functions MUST stay within 50 effective logic lines. Blank lines and doc-comments are excluded. **Atomic Verification**: Functional verification MUST be performed at the same granularity as code changes; massive edits without intermediate verification are prohibited.
  - **Exception & Annotations**: Under strict conditions, specific functions may exceed the 50-line limit *only* if they are inherently indivisible and do not increase cognitive load. This is strictly restricted to:
    1. **Exhaustive Match Dispatchers**: A function that is almost entirely composed of a single, flat, non-nested `match` statement (such as standard AST parsing or serializing command dispatches). Maximum limit: **200 lines**.
    2. **GUI Layout/Declaration Functions**: Functions declaring sequential egui/UI structures where splitting would fragment the visual understanding of the layout tree. Maximum limit: **150 lines**.
  - **Safeguards Against Abuse**:
    1. **Mandatory Explicit Annotation**: The function declaration line *must* carry an explicit annotation on the same line: `// RR-15 Limit: [Dispatcher/GUI] - [Brief justification]`.
    2. **No Business Logic inside Arms**: In match dispatchers, the match arms must strictly delegate logic to sub-functions. Deep nesting (over 2 levels) or execution of complex algorithms in arms instantly invalidates the exception.
    3. **Absolute Ceiling**: Under no circumstances shall *any* function in the codebase exceed 200 lines of effective logic.

## 2. No-Panic Invariance
- **Rule**: Strict prohibition of `unwrap()`, `expect()`, and `panic!()` for input-dependent paths.
- **Purpose**: Eradicate runtime crashes originating from malformed or malicious data.
- **Compliance**: All data-dependent operations MUST utilize the `Result` type. Production code must have zero termination paths triggered by external inputs.
  - **Exception & Annotations**: For proven internal logical invariants (operations physically/mathematically guaranteed never to fail, such as guard-checked Option states or immediate post-creation caches), `unwrap()` or `expect()` may be used *only* if accompanied by a `// RR-15 Safe: [Detailed Reason]` comment on the same line.
  - **Safeguards Against Abuse**:
    1. **Strict Exclusion of Input Paths**: Under no circumstances shall an annotation be used to bypass `unwrap` checks on paths processing external, file-dependent, or environment-dependent data.
    2. **Locality Constraint**: The safety must be provable within the immediate local execution context (same function or struct invariants).
    3. **Mandatory Audit**: Any commit introducing a `// RR-15 Safe` annotation must undergo mandatory logical validation during code review to prevent lazy error swallowing.

## 3. Memory Safety Isolation
- **Rule**: Total prohibition of `unsafe` blocks in core engine layers.
- **Purpose**: Guarantee 100% compiler-verified memory safety.
- **Compliance**: The `unsafe` keyword count MUST be zero in `crates/ferruginous-sdk` and `crates/ferruginous-render`.

## 4. Logical Path Flattening
- **Rule**: Prefer early returns (`?` operator). Limit control-flow nesting to 2 levels.
- **Purpose**: Prevent "Indentation Hell" and simplify code auditability.
- **Compliance**: Control flow MUST be linear and transparent.

## 5. Exhaustive Pattern Matching
- **Rule**: Prohibit wildcard patterns (`_`) in `match` statements.
- **Purpose**: Leverage the compiler to enforce handling of future enum variants.
- **Compliance**: All `match` expressions MUST explicitly account for all known variants. In high-level interpreters, the dispatch of the `Command` IR enum MUST be exhaustive and SHOULD be implemented in a flat, consolidated match loop to ensure visual auditability and prevent hidden state loss in sub-handlers.

## 6. Stack Overflow Prevention
- **Rule**: Prohibit unbounded recursion. Use explicit heap-allocated stacks.
- **Purpose**: Eradicate stack exhaustion in deep PDF object graphs.
- **Compliance**: Any tree traversal MUST use an explicit `Vec`-based stack or a hard-coded depth limit (e.g., 64).

## 7. Mutable State Prohibition
- **Rule**: Total prohibition of global mutable state (`static mut`).
- **Purpose**: Eliminate data races and non-deterministic execution order.
- **Compliance**: Shared state MUST be managed via synchronization primitives or strict ownership transfer.

## 8. Type-Level Correctness
- **Rule**: Utilize enums to make invalid states physically unrepresentable.
- **Purpose**: Shift runtime assertions to compile-time proofs.
- **Compliance**: Minimize runtime "Guard Clauses" by enforcing invariants through the type system.

## 9. Ownership-First Design
- **Rule**: Prefer owned data over complex lifetime references.
- **Purpose**: Prevent "Lifetime Pollution" and architectural rigidity.
- **Compliance**: Structs and signatures SHOULD be self-contained whenever possible.

## 10. Bit-Perfect Determinism
- **Rule**: Prohibit non-deterministic collections (`HashMap`/`HashSet`). Use `BTreeMap`/`BTreeSet`.
- **Purpose**: Guarantee identical output bytes for identical input bytes.
- **Compliance**: Internal iteration order MUST be fixed.

## 11. Domain-Specific Error Handling
- **Rule**: Prohibit `String` errors. Use concrete Enum types via `thiserror`.
- **Purpose**: Ensure programmatic recoverability and precise traceability.
- **Compliance**: Every failure mode MUST be a named enum variant with mandatory context (e.g., `pos`, `handle`).

## 12. Invariant Enforcement
- **Rule**: Distinguish between **Stable Handles** (`Handle<Object>`) and **Volatile Handles**.
- **Purpose**: Prevent dangling references after refinery passes.
- **Compliance**: Persistent models MUST NOT store volatile handles. Use `assert!` ONLY for internal logical impossibilities.

## 13. Zero-Swallowing Policy
- **Rule**: Prohibit silent error discarding (`.ok()`, `_`).
- **Purpose**: Early detection of latent bugs.
- **Compliance**: Every `Result` MUST be evaluated. Unhandled `RawOperator` instances MUST trigger explicit warnings.

## 14. Locality of Declaration
- **Rule**: Declare variables immediately before use. Minimize scope.
- **Purpose**: Reduce variable "life-span" and prevent accidental reuse.
- **Compliance**: Keep initialization and consumption distances minimal.

## 15. Explicit Allocation
- **Rule**: Prohibit `.clone()` for the sole purpose of satisfying the borrow checker.
- **Purpose**: Visualize and minimize inefficient memory overhead.
- **Compliance**: Use `.clone()` ONLY when logical duplication of data is truly intended.
