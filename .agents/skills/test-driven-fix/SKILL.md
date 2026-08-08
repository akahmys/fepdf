---
name: test-driven-fix
description: >-
  Unified protocol for bug diagnosis, harness-driven development (HDD),
  and debugging. Use when fixing bugs, building test harnesses, or
  troubleshooting rendering and parsing anomalies.
---

# Test-Driven Fix Protocol

> [!IMPORTANT]
> Combines the Scientific Fix Method, Harness-Driven Development (HDD), and systematic debugging into a single workflow. All fixes must be backed by mechanical proof.

---

## Phase 1: Diagnosis

### 1.1. Scientific Diagnosis
- Perform a diagnosis to narrow down the cause to a single hypothesis before any code changes.
- `task.md` must document the phenomena, hypotheses, and diagnostic results before implementation begins.

### 1.2. Hypothesis-Driven Verification
- **Rapid Branching**: Formulate multiple causes (Hypotheses) immediately. Do not fixate on a single path.
- **Fast Disproval**: Prioritize probes that can disprove a hypothesis within minutes.

### 1.3. Specification Alignment
- Every fix must be verified against ISO 32000-2 requirements via `pdf-spec-mcp`.
- The fix must cite the relevant ISO Clause and proof of compliance.

### 1.4. User Confirmation
- Present the diagnosis and proposed solution to the user and obtain explicit confirmation before implementing.

---

## Phase 2: Harness Construction (HDD)

### 2.1. Specification-First Design
- Extract requirements (shall/must) from ISO 32000-2 using `pdf-spec-mcp` prior to implementation.
- `implementation_plan.md` must cite specific ISO Clauses and their requirements.

### 2.2. Scaffold Harness (Fail-First)
- Build a failing test or diagnostic probe *before* implementing production logic.
- Strictly define expected inputs/outputs based on `docs/specs/` or ISO Clause information.
- Run `cargo test` with empty logic and confirm that it fails as intended.
- The Clause number must be included in the test name or comments.

### 2.3. Resolve Harness
- Write only the minimum necessary logic to make the harness PASS.
- Prohibit resolving borrowing errors using `.clone()`; instead, redesign the ownership structure.
- Tests become Green and no redundant functionality is included.

---

## Phase 3: Verification

### 3.1. Minimal Intervention
- Modify only the minimum necessary logic required to address the diagnosed cause.
- The resulting diff must be focused solely on the bug's root cause.

### 3.2. Regression Verification
- A fix is not complete until:
  - A reproduction test passes.
  - All existing tests in the workspace pass (`cargo test --workspace`).
  - The compliance audit passes with zero warnings (`./scripts/audit/verify_compliance.sh`).
- `walkthrough.md` must link to the passing test/audit results.

### 3.3. Atomic Interface Compliance
- When a shared trait or public interface is modified, all implementations and call-sites must be updated within the same task block.
- `cargo check --all` must pass after every interface-modifying task.

### 3.4. Fail-Fast Integration
- For features involving complex context propagation, a basic integration test must be executed early.

---

## Phase 4: Debugging Techniques

### 4.1. Visual Sincerity
- Never dismiss rendering glitches as "artifacts." Treat them as mathematical proofs of sign errors, scaling mismatches, or state-machine failures.
- Infer the faulty layer (CMap, Matrix, or Buffer) directly from visual evidence.

### 4.2. State Visualization
- Always log the **Total Accumulated State** (e.g., current CTM, total advance) rather than incremental deltas.
- Monitor state resets and reversals to pinpoint the exact operator causing state corruption.

### 4.3. Differential Debugging
- Compare "Working" vs. "Broken" cases using identical conditions and log formats.
- Isolate the smallest possible reproduction case to eliminate noise.

### 4.4. Layer Isolation
- **Physical**: Decryption, stream decompression, parsing (Symptoms: Corrupt bytes, syntax errors).
- **Semantic**: Refinement, resource mapping, sublimation (Symptoms: Invisible text, incorrect font, mojibake).

### 4.5. Raw Data Verification
- Always verify rendering bugs against the **Raw PDF Byte Stream** before trusting the IR.

### 4.6. External Structural Validation
- **Mutool Audit**: Use `mutool info` and `mutool clean` to verify page tree integrity.
- **QPDF Compliance**: Use `qpdf --check` for linearization and Xref/Trailer linkage validation.
- **Bitstream Verification**: Check for "overflow reading bit stream" warnings in `qpdf`.

### 4.7. Logging & Diagnostic Output Rules
- Never leave raw `println!` or `eprintln!` residues in production code.
- Always use standard `log::debug!` / `log::info!` / `log::warn!` / `log::error!` macros.

### 4.8. Tool-Chain Discipline
- Verify command signatures using `--help` before execution. Do not rely on positional inference.
- Prefer atomic, single-file edits when updating sensitive documents.

---

## Phase 5: Knowledge Conversion

### 5.1. Evidence Persistence
- Persist all test logs and proof-of-compliance artifacts to `walkthrough.md`.
- For graphics/rendering, "Visual Proof" (screenshots or PNGs) is mandatory.

### 5.2. Lessons Learned
- Record diagnostic patterns in `lessons_learned.md`.
- Promote institutional memory and prevent AI contexts from forgetting common pitfalls.
