# fepdf Rebirth Roadmap (v2.2)

The "Rebirth" project aims to establish the world's most robust and ISO-compliant PDF 2.0 toolkit, culminating in the **fepdf Flagship GUI Editor**.

---

## [COMPLETED] Phase 1 - 22: Foundation, Refinery & UA-2 Bridge
**Goal**: Build a high-purity PDF 2.0 data model and comprehensive diagnostic engine.

- [x] **Core Engine (M1-M62)**: Established Vello rendering, CJK hardening, and the fepdf Unified Architecture (FUA).
- [x] **The Refinery (M63-M65)**: Implemented high-speed, parallel ingestion with the Typed Arena and Handle system.
- [x] **UA-2 Engine (M66-M67)**: Finalized logical structure tree traversal and Matterhorn Protocol auditing.

## [COMPLETED] Phase 23: fepdf Deployment & Rendering Stabilization
**Goal**: Achieve absolute rendering parity and finalize the production-grade CLI toolkit.

- [x] **Rendering Stabilization**: Resolved complex graphics state leakage (EndPath `n` handling) and achieved parity for advanced technical manuals (Intel SDM baseline).
- [x] **High-Fidelity Color & Operator Hardening**: Implemented exhaustive operator dispatching (Rule 5) and high-fidelity color state preservation (Gray/RGB/CMYK/Lab).
- [x] **Type 3 Font Pipeline**: Normalized loading and metrics parsing for legacy CJK Type 3 fonts with `CharProcs` support.
- [x] **Active Refinement**: Implemented Unicode-native normalization and lossless color state serialization in the upgrade pipeline.
- [x] **Governance & Hardening**: Achieved 100% RR-15 compliance across the SDK and Render crates.

## [COMPLETED] Phase 24: High-Performance Interactive Canvas (The Glass Phase)
**Goal**: Deliver a 120fps fluid PDF viewing experience optimized for modern GPUs.

- [x] **M68: Vello-Native egui Integration**:
    - [x] **Direct Paint Callbacks**: Transition from headless texture blitting to direct egui-wgpu paint callbacks for zero-latency interaction.
    - [x] **Retained Mode Rendering**: Implement scene-graph caching to minimize GPU redraws for static pages.
- [x] **M68b: Viewport Virtualization**:
    - [x] **Adaptive Tiling**: Implement tiled rendering for extreme zoom levels and large-format documents.
    - [x] **Dynamic Memory Management**: Implement viewport-aware object sublimation and precipitation.
- [x] **M68c: Selection & Interaction Engine**:
    - [x] **Sub-pixel Hit Testing**: Precision text and path selection with real-time visual feedback.
    - [x] **Semantic Highlighting**: Integrated rendering for annotations, links, and search results.

## [COMPLETED] Phase 25: The Semantic Forge (The Structure Phase)
**Goal**: Treat PDF as a logical structure tree first, and a visual representation second.

- [x] **M69: Universal Structure Tree (UST)**:
    - [x] **Interactive Tag Explorer**: A sidebar for visualizing and editing the logical structure (Tags/UA-2).
    - [x] **Hierarchy Manipulation**: UI for semantic re-tagging and structure re-ordering.
- [x] **M69b: Remediation Assistant**:
    - [x] **Matterhorn Dashboard**: Real-time accessibility auditing with visual pointers to compliance errors.
    - [x] **Alt-Text Studio**: A dedicated interface for managing alternative descriptions for complex figures.

## [COMPLETED] Phase 26: Professional Production & Security
**Goal**: Industrial-strength document integrity and output control.

- [x] **M70: The Forge Core**: Implemented PAdES-compliant signatures and Full Linearization with Hint Table generation. [COMPLETED]
- [x] **M70b: Production Studio GUI**: Visual controls for Linearization, Vacuum, and Standard Compliance (PDF/A-4).
- [x] **M70c: Atomic Redaction**: Secure UI for permanent visual and physical removal of sensitive stream data.

## [COMPLETED] Phase 27: Multi-Language & Global Workspace
**Goal**: Native translation interfaces and user settings panel.

- [x] **M71: Workspace Localization & Commands**:
    - [x] **i18n Translation Dictionary**: Implemented external JSON dictionary loading supporting English and Japanese interfaces dynamically.
    - [x] **Command Palette & Preferences**: Added quick command palette navigation and visual user preference settings toggles.

---

*Roadmap updated: 2026-06-04 (Phase 24-27 Completed & GUI Desktop Editor Overhauled)*
