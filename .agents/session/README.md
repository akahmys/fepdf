# Ferruginous Session Memory (ELM)

This directory serves as the **External Long-term Memory (ELM)** for AI agents working on the Ferruginous project.

## Purpose
Since AI internal context is volatile and limited, all critical design decisions, architectural maps, and "lessons learned" from individual sessions must be persisted here.

## Structure
- **`decisions/`**: Records of major technical decisions (ADRs).
- **`context/`**: Snapshot of current architectural state and active "North Star" goals.
- **`history/`**: Chronological log of major milestones and resolved "friction points."

## Rule
Every session that results in a structural change or a protocol update MUST leave a trace here to ensure continuity for the next agent invocation.
