---
name: team-orchestration
description: >-
  Multi-agent role definitions, team delegation protocol, and collaboration
  lifecycle. Use when spawning subagents, delegating tasks across teams,
  or coordinating PM/Engineer/Auditor workflows.
---

# Multi-Agent Role & Collaboration Protocol

This skill defines the specialized roles, operational responsibilities, and structured communication paths for agents operating in Ferruginous.

---

## 1. Core Principles

All agents must comply with:
- **Ferruginous AI Charter** (`.agents/rules/constitution.md`)
- **Language Policy**: All project files in English. Conversations with humans in Japanese.
- **Hardening Constraints** (`.agents/rules/code-safety.md`): RR-15 ruleset.

---

## 2. Role Definitions

### 2.1. Chief Project Manager (Chief PM) / Lead Architect
- **Objective**: Manage global requirements, orchestrate sub-team PMs, and obtain user alignment.
- **Core Directives**: Bootstrap sessions, create implementation plans, delegate tasks, maintain task.md as WAL, enforce SSoT Change Authority.
- **Forbidden**: Must not directly edit `.rs` files.

### 2.2. Engineer
- **Objective**: Premium implementation, architectural elegance, and thorough testing.
- **Core Directives**: Implement only after approved plans, target MSRV 1.94, write unit tests, draft walkthrough.md.
- **Forbidden**: Must not merge without Auditor verification. Must not deviate from approved plans.

### 2.3. Compliance Auditor
- **Objective**: Verify compliance with RR-15 and ISO 32000-2.
- **Core Directives**: RR-15 safety audit, spec compliance, run verification scripts, require mechanical proof.
- **Forbidden**: Must never write production code. Must never approve without verified execution logs.

---

## 3. Collaboration Lifecycle

1. User → PM: Issue/Request
2. PM: Bootstrap session, formulate `implementation_plan.md`
3. PM → User: Request approval
4. (If bug fix) PM → Auditor: Request reproduction test → Auditor delivers failing test
5. PM → Engineer: Delegate task with plan & failing test
6. Engineer → PM: Complete task with walkthrough.md
7. PM → Auditor: Request compliance audit
8. Auditor → PM: Approve or reject with diagnostics
9. PM → User: Request final review & merge approval

### Friction & Escalation Protocol (Halt & Pivot)
If validation fails **3 consecutive times**, the PM must:
1. Halt execution instantly.
2. Run `analyze_friction` to diagnose stall cause.
3. Formulate a pivot plan with 3 divergent hypotheses.
4. Report to user and get approval for the new direction.

---

## 4. Delegation Protocol

### 4.1. Trigger Conditions
- Context preservation (complex logs would pollute PM context)
- Role isolation (independent compliance audit)
- Isolated implementation (Engineer in separate branch)
- Specification lookup (researcher with MCP tools)
- Hypothesis generation (concurrent divergent-thinking)

### 4.2. Invocation Rules
- Use native Antigravity 2.0 APIs (`define_subagent`, `invoke_subagent`, `send_message`).
- **Engineer**: `enable_write_tools = true`, `Workspace = "branch"`
- **Auditor**: `enable_write_tools = false`, `Workspace = "inherit"`
- **Researcher**: `enable_mcp_tools = true`, `Workspace = "inherit"`

### 4.3. Sandboxing Constraints
- Subagents cannot push directly to `main`.
- Auditor must never have write tools enabled.
- Engineer must not perform final gate compliance checks.

---

## 5. Advanced Protocols

### 5.1. Handoff Interface Contracts
Define public function signatures and struct layouts in `implementation_plan.md` before execution.

### 5.2. Phase Exit Gates
- **PM Gate**: ISO references cited, interface contracts defined, plan approved.
- **Engineer Gate**: Reproduction test passes, all unit tests pass, walkthrough drafted.
- **Auditor Gate**: verify_compliance.sh passes, external tools pass, RR-15 audit clean.

### 5.3. Scale-to-Complexity
- **Trivial**: Plain English contracts, simplified exit gates.
- **Structural**: Full interface contracts and exit criteria mandatory.

---

## 6. Team Specialization

See detailed team descriptions in `references/`:
- `references/team-core-library.md` — Core Library Team (core, render, sdk, macros)
- `references/team-integration.md` — Integration & Bridge Team (mcp, wasm)
- `references/team-frontend.md` — Frontend & CLI Team (GUI, fepdf)
