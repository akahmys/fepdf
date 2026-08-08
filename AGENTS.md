# Project Agents & Governance

This project adheres to the [Antigravity IDE](https://antigravity.google) conventions for autonomous agentic development.

## ⚖️ Core Constitution
Global project principles and the Hierarchy of Truth are defined in [.agents/rules/constitution.md](.agents/rules/constitution.md).

## 🛡️ Hardening & Quality Gates
- **Code Safety (RR-15)**: Strict implementation safety rules in [.agents/rules/code-safety.md](.agents/rules/code-safety.md).
- **ISO Compliance**: ISO 32000-2 compliance strategy in [.agents/rules/iso-compliance.md](.agents/rules/iso-compliance.md).
- **Naming Conventions**: Identifier and API conventions in [.agents/rules/naming-conventions.md](.agents/rules/naming-conventions.md).

## 🏗️ Domain-Specific Rules (Glob-Activated)
- **PDF Engine** (core, macros): Pipeline constraints in [.agents/rules/pdf-engine.md](.agents/rules/pdf-engine.md). Design spec: [docs/specs/core-pipeline.md](docs/specs/core-pipeline.md).
- **SDK Engine** (sdk): Interpretation & serialization constraints in [.agents/rules/sdk-engine.md](.agents/rules/sdk-engine.md). Design spec: [docs/specs/sdk-pipeline.md](docs/specs/sdk-pipeline.md).
- **GPU Rendering** (render): Rendering constraints in [.agents/rules/gpu-rendering.md](.agents/rules/gpu-rendering.md). Design spec: [docs/specs/rendering.md](docs/specs/rendering.md).
- **Desktop UI** (GUI, CLI): Interface design protocol in [.agents/rules/desktop-ui.md](.agents/rules/desktop-ui.md).

## 🛠️ Skills
- **Strategic Planning**: Session management and pre-implementation review in [.agents/skills/strategic-planning/](.agents/skills/strategic-planning/).
- **Test-Driven Fix**: Bug diagnosis, HDD, and debugging in [.agents/skills/test-driven-fix/](.agents/skills/test-driven-fix/).
- **Code Audit**: RR-15 and ISO compliance auditing in [.agents/skills/code-audit/](.agents/skills/code-audit/).
- **PDF Production**: Production-ready PDF generation in [.agents/skills/pdf-production/](.agents/skills/pdf-production/).
- **Team Orchestration**: Multi-agent roles and delegation in [.agents/skills/team-orchestration/](.agents/skills/team-orchestration/).
- **Retrospective**: Post-development analysis and friction resolution in [.agents/skills/retrospective/](.agents/skills/retrospective/).
- **GitHub Workflow**: Branch, PR, and merge governance in [.agents/skills/github-workflow/](.agents/skills/github-workflow/).
- **Codebase Exploration**: Discovery protocol in [.agents/skills/codebase-exploration/](.agents/skills/codebase-exploration/).

## 🔄 Workflows
Slash-command triggered workflows are located in [.agents/workflows/](.agents/workflows/).
