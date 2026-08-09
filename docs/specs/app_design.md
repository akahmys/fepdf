# Technical Specification: fepdf UI

> [!IMPORTANT]
> A PDF viewing and physical editing solution that integrates the parsing capabilities of `fepdf-sdk` with Vello rendering, achieving usability and functionality that surpasses professional standards.

## 1. Design Philosophy

This project extends and applies **[RR-15](../../.antigravity/rules/hardening.md)** and **[HDD](../../.antigravity/rules/pipeline.md)** to the application layer. Rendering utilizes hardware acceleration via `vello` and adopts a Pull-type Architecture where the Engine generates a `DisplayList` and the Renderer consumes it asynchronously.

### 1.1 The 4 Pillars of Excellence

1. **Zero-Latency Rendering**: A resolution-independent, zero-latency drawing experience powered by GPU computation (Vello).
2. **Engineering-Grade Precision**: CAD-level snapping and real-scale measurement based on vector data.
3. **Semantic Editor**: Intuitive structural editing and tag correction via an object-oriented UI.
4. **Transparent Governance**: Complete visualization and guarantee of specification compliance and security.

## 2. System Architecture

### 2.1 Rendering & Analysis Flow

1. **Parse**: Read PDF 2.0 and construct the `Catalog` using the `loader`.
2. **Validate**: Perform dictionary validation using `arlington`.
3. **Interpret**: Scan content streams using the `engine`.
4. **Transform**: Convert from PDF user space to device coordinates using `kurbo::Affine`.
5. **State Update**: Update the `GraphicsState` stack.
6. **Issue**: Accumulate `DrawOp`s into the `DisplayList`.
7. **Render**: Backends (Vello, etc.) render commands to the GPU via the `RenderBackend` interface.

### 2.2 Technology Stack

- **Language & Foundation**: Rust 1.94 / Edition 2024
- **Rendering Engine**: `Vello` (Next-generation vector graphics renderer)
- **Graphics API**: `WGPU` 28.0 (WebGPU compliant)
- **UI Framework**: `egui`/`eframe` 0.33.1
- **Window Management**: `winit` 0.30

## 3. UI/UX Strategy: "Contextual Magic"

- **Menu-less Workflow**: Abandon large fixed menus in favor of a context-oriented UX that presents optimal operation options upon object selection.
- **Infinite Workspace**: Manage multiple PDFs on a single canvas, allowing for free orchestration of pages and elements.

## 4. Coordinate Transformation Model

$$P_{screen} = M_{zoom} \cdot M_{pan} \cdot M_{pdf\_to\_screen} \cdot P_{user}$$

- **$M_{pdf\_to\_screen}$**: Y-axis inversion and origin offset based on MediaBox.
- **$M_{zoom}$ / $M_{pan}$**: User-driven transformations (zoom/pan).

## 5. UI Components (fepdf-ui)

- **Canvas**: Integrates PDF vector data and dynamic overlays (measurement lines, snap points, etc.) using `RenderBackend`.
- **Control**: Modeless floating menus powered by `egui`.
- **Inspector**: Visualization editor for Arlington validation results and Tag structures.
- **System Bridge**: Abstracts OS-dependent operations (e.g., file handling) via the `SystemBridge` trait.
- **Editor Bridge**: Propagates destructive operations from the UI to the SDK's `PdfEditor` for incremental saving.

## 6. Quality Guardrails

- **Audit**: Safety auditing via `verify_compliance.sh`.
- **Fail-safe**: Continue rendering in safe mode even upon specification violations, assisting the user in structural restoration.
