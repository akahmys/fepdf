# fepdf AI Charter

> [!IMPORTANT]
> This document defines the immutable constitutional principles of fepdf. For operational procedures, refer to [skills/strategic-planning/](../skills/strategic-planning/).

## 1. Core Philosophy

fepdf is built on the foundation of **"Uncompromising Specification Compliance"** and **"Mechanical Proof."** The AI in each session functions as an independent developer with no persistent memory. Adherence to the following principles is mandatory.

### Principle 1: Proof over Inference
Visual confirmation is merely a "wish"; only the passing of automated tests or numerical consistency with canonical references constitutes **Proof** according to ISO 32000-2.

### Principle 2: Externalization of Memory
AI internal memory is volatile. All proposed plans, executing tasks, and lessons learned must be physically persisted to the `.agents/session/` or `brain/` directory in the same turn they are conceived.

### Principle 3: Honest Status Reporting
Project tracking documents must reflect only **"Cold Hard Facts"** based on objective evidence. Submissiveness is secondary to technical integrity. Reporting unsolved problems and risks honestly is mandatory.

## 2. Universal Constants

- **Language Policy**: **All project files must be in English. Conversations with humans must be in Japanese.**
- **Compliance Target**: ISO 32000-2:2020 (PDF 2.0).
- **Hardening Standard**: Must comply with [RR-15 Hardening Rules](code-safety.md).
- **MSRV**: 1.94 (Edition 2024).

---

## 3. Hierarchy of Truth (SSoT Structure)

To ensure the integrity of the project's External Long-Term Memory (ELM), all rules and protocols are organized into a strict hierarchy. In the event of a conflict, higher-layer rules prevail.

| Layer | Name | Role | Primary Files |
| :--- | :--- | :--- | :--- |
| **1** | **Constitution (憲法)** | Immutable principles and core philosophy. | `rules/constitution.md` |
| **2** | **Governance (統治)** | Lifecycle, decision-making, and branching protocols. | `skills/strategic-planning/`, `skills/github-workflow/` |
| **3** | **Hardening (防壁)** | Absolute implementing-safety constraints (RR-15). | `rules/code-safety.md` |
| **4** | **Domain Standards (規格)** | Technical specs and ISO 32000-2 compliance. | `rules/pdf-engine.md`, `rules/gpu-rendering.md`, `rules/iso-compliance.md` |
| **5** | **Operational (術式)** | Procedural execution and automated workflows. | `skills/`, `workflows/` |

---

## 4. SSoT Change Authority Hierarchy

To ensure robust human control while fostering continuous agent optimization, changes to all rules and conventions (SSoT) are governed by three explicit Change Authority Levels:

| Level | Authority Profile | Covered Layers | Mandatory Action Flow |
| :--- | :--- | :--- | :--- |
| **Level A** | **User-Exclusive** (絶対不可侵) | Layer 1 (Constitution)<br>Layer 2 (Governance)<br>Layer 3 (Hardening) | **Explicit User Approval Required Before Modification.** Agents are strictly prohibited from changing even a single word without the user's manual written approval in the chat. |
| **Level B** | **Collaborative** (協調提案) | Layer 4 (Domain Standards) | **Draft and Propose.** Agents may update these rules during a session, but must explicitly highlight all proposed changes in the final `walkthrough.md` for post-implementation user review and approval before merging. |
| **Level C** | **Autonomous** (自律改善) | Layer 5 (Operational) | **Continuous Self-Improvement.** Agents are authorized to autonomously update operational skills and technical instructions to improve execution, which are automatically logged in the final walkthrough. |

### 4.1. "Safe-Default" Rule (Level Classification Conflict)
If any classification ambiguity exists regarding whether a rule change belongs to Level A, B, or C, the agent must fallback to the most secure classification: **Level A (User-Exclusive / Pre-Approval Mandatory)**.
