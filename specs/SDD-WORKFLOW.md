# Screenstory — Specification-Driven Development Workflow

## Purpose

This document defines how product requirements, architecture, specifications, implementation tasks, and verification evidence are managed.

## Core rules

1. No implementation without an approved specification.
2. Every implementation task references one or more specification IDs.
3. Every task has explicit acceptance criteria.
4. Implement one active task at a time unless a specification explicitly permits parallel work.
5. Do not expand scope silently.
6. Record deviations and obtain approval before changing behavior.
7. Tests and verification evidence are required before closing a task.
8. Update `specs/project-state.md` whenever the active task, milestone, decisions, or blockers change.
9. Original recordings are immutable; editing is timeline-based and non-destructive.
10. AI output is untrusted input and must be validated before entering domain state.

## Specification lifecycle

`Draft → In Review → Approved → Implemented → Verified → Closed`

A specification may be amended only through a recorded change with rationale and impact analysis.

## Task lifecycle

`Proposed → Ready → In Progress → Implemented → Verified → Closed`

A task cannot move to Ready unless:
- Its parent specification is approved.
- Dependencies are identified.
- Acceptance criteria are testable.
- Required contracts are defined.

## Task handoff format

Each handoff must include:

- Task ID and title.
- Parent specification IDs.
- Approved base commit.
- Scope.
- Out-of-scope items.
- Acceptance criteria.
- Dependencies.
- Test plan.
- Verification evidence.
- Resulting commit and branch, when applicable.

## Definition of Done

A task is complete only when:
- Implementation matches the approved specification.
- Relevant tests pass.
- Existing regression tests pass.
- Failure cases are handled.
- Documentation is updated.
- No unrelated scope was introduced.
- Verification evidence is recorded.
- `project-state.md` is updated.

## Change control

Changes to capture behavior, AI contracts, timeline schema, rendering semantics, or privacy behavior require specification updates before implementation.

## AI safety and correctness

- AI-generated descriptions must not claim unobserved actions as facts.
- User-provided descriptions are context, not proof.
- AI responses must pass schema validation.
- Cloud transfer must follow explicit user consent and configured privacy policy.
- Credentials and sensitive values must never be included in prompts or logs intentionally.
