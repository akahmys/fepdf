# ISO 32000 Compliance Strategy

> [!IMPORTANT]
> **Technical compliance is the singular raison d'être of Ferruginous.**
> All implementation choices must be objectively validated against ISO 32000-2:2020 using automated auditing and differential testing.

---

## 1. Specification Mapping (Static Auditing)
- **Rule**: Every logical component MUST be mapped to specific ISO 32000-2 Clauses.
- **Verification**: Dictionary structures MUST be validated against the **Arlington PDF Model**.
- **Criterion**: `implementation_plan.md` MUST cite specific Clause numbers.

## 2. Viewer Fidelity (Dynamic Validation)
- **Rule**: Validate all generated outputs against **Adobe Acrobat Reader** as the reference baseline.
- **Verification**: Outputs must be visually and structurally correct in Acrobat. "Error 135" or internal structural warnings are considered critical compliance failures.
- **Criterion**: Pass VeraPDF "Level 2.0" validation for all generated documents.

## 3. Persistent Compliance Logs
- **Rule**: All verification results MUST be persisted to the `.agents/session/` directory.
- **Verification**: Every feature or fix MUST include an evidence log (clause mappings, test outputs) in its `walkthrough.md`.
- **Criterion**: Completion is defined by the existence of verified compliance evidence.

## 4. Zero-Warning Quality Gate
- **Rule**: Maintain a "Zero Tolerance" policy for static analysis warnings.
- **Verification**: 100% pass rate for `clippy` and the internal `scripts/audit/verify_compliance.sh` suite.
- **Criterion**: The `main` branch must remain in a "Perfectly Green" state at all times.
- **Workspace Lints**: Workspace-wide clippy lints defined in `Cargo.toml` must be explicitly inherited by all member crates using `lints.workspace = true` to guarantee consistent enforcement across the workspace and prevent compilation failures under `-D warnings`.

## 5. Binary Data Sanitization
- **Rule**: Strictly isolate non-textual streams (Fonts, Images, ICCProfiles) from text-based processing.
- **Purpose**: Prevent irreversible data corruption caused by UTF-8 normalization or string-based substitution on binary payloads.
- **Compliance**: The refinery MUST utilize an explicit object-type-based denylist.

## 6. Pass 0: Physical Normalization
- **Rule**: All input documents MUST undergo "Pass 0 Physical Normalization" before semantic ingestion.
- **Purpose**: Resolve physical-layer anomalies (decryption, XRef repairs, residual `/Encrypt` dictionaries) to provide a "Plaintext PDF 2.0" baseline for the `PdfArena`.
- **Compliance**: Iterative (stack-based) walk of all raw objects is mandatory to ensure Acrobat compatibility.
- **Decryption & Key Derivation**: Decryption and key derivation for PDF 2.0 V5 (AES-256) security handlers MUST fully implement the multi-stage hashing specified in Clause 7.6.4.3.3 (Algorithm 2.A/3.A) with deterministic validation and key salts to protect against physical structure corruption and guarantee bit-perfect determinism.

## 7. Secrets & PII Guard
- **Rule**: Absolute prohibition of committing authentication tokens, keys, or PII.
- **Verification**: Mandatory execution of `scripts/audit/verify_secrets.sh` in the compliance suite.
- **Compliance**: Any secret leak is a catastrophic failure requiring history rewriting.
