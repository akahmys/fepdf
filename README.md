# fepdf: High-Fidelity PDF 2.0 Processing Engine

**fepdf** is an experimental, high-fidelity PDF processing platform engineered with Rust. It achieves **ISO 32000-2:2020** compliance through a deterministic, hardware-accelerated architecture designed to master the complexity of modern and legacy PDF structures.

The project strictly adheres to the **RR-15 (Reliable Rust-15)** hardening protocol—a mission-critical safety standard ensuring memory safety (`unsafe_code = "forbid"`), bit-perfect determinism, and absolute reliability.

---

## 🏛️ System Architecture & Governance

Detailed system design and governance standards are documented across the following authoritative guides:

- 🏛️ **[ARCHITECTURE.md](ARCHITECTURE.md)**: **System Design & Layering Rules** (the three rules that place code, target crate topology and its migration status, `PdfArena` invariants, Sublimation Pipeline Pass 0/1/2, Vello GPU compute renderer).
- 📘 **[AGENTS.md](AGENTS.md)**: **Project Constitution** (Truth hierarchy, core operating principles).
- 📋 **[PLANNING.md](PLANNING.md)**: **Planning & Discovery** (Implementation plans, codebase discovery protocols).
- 💻 **[CODING.md](CODING.md)**: **Coding Standards** (RR-15 rules, function limits, Rust 2024 standards).
- 🛡️ **[AUDITING.md](AUDITING.md)**: **Security & Compliance** (Static audits, `cargo-deny` license checks, `betterleaks` PII protection).
- 📜 **[docs/adr/](docs/adr/README.md)**: **Decision Records** (decisions that were contested, reversed, or rest on a measurement).
- 🧪 **[TESTING.md](TESTING.md)**: **Testing Strategy** (Workspace unit tests, Vello visual regression, MSRV 1.94+).

---

## 🚀 Key Capabilities

- **Hardened Core Engine (`fepdf-model`)**: Fully audited **Syntax**, **Font**, and **Model** subsystems. Hardened against Zip Bomb streams, recursion limits, cyclic object resolution, and out-of-bounds panics while strictly observing ISO 32000-2:2020.
- **Interactive Desktop GUI (`fepdf`)**: Built with **egui** + **wgpu** + **Vello**. Features 120fps canvas interaction, Japanese/CJK system font loading, CAD measurement tools, accessibility tagging, and atomic redaction.
- **Universal CLI (`fepdf`)**: Command-line toolkit for structural auditing, PDF 2.0 re-production, font glyph tracing, and document repair.
- **AI-Native MCP Bridge (`fepdf-mcp`)**: Implementation of the **Model Context Protocol**, enabling AI assistants to perform direct PDF structural diagnostics.
- **WebAssembly Runtime (`fepdf-wasm`)**: WebAssembly bindings for running the fepdf engine inside web browser environments.

---

## ⚙️ Development & Verification Commands

```bash
# Run full compliance audit (RR-15, Clippy, cargo-deny, betterleaks)
make audit

# Run Cargo-native license check
cargo deny check licenses

# Run full workspace unit tests
cargo test --workspace

# Run GPU visual regression tests
python3 scripts/visual_regression.py
```

---

## 📜 License

- **MIT License**
- Designed for technically sound compliance with the ISO 32000-2:2020 standard.
