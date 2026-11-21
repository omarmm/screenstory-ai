# SPEC-017 — Tauri Desktop Foundation

**Status:** Draft — requires product-owner review and approval  
**Milestone:** M1 — Desktop Foundation  
**Related:** PRD-001, ADR-001, ADR-010, ARCH-001, SPEC-015, SPEC-016

## 1. Purpose

Define the minimum production desktop foundation for Screenstory using Tauri v2, React, TypeScript, and Rust. This specification covers the application shell and its security/build boundaries only. It does not authorize production screen capture, AI analysis, timeline editing, or video export.

## 2. Goals

- Create a launchable Linux desktop application using the accepted stack.
- Keep frontend presentation separate from Rust application/domain logic.
- Establish a narrow, typed, validated command boundary.
- Apply least-privilege Tauri v2 capabilities.
- Establish a testable project workspace bootstrap boundary without implementing the complete project domain.
- Provide a reproducible development, test, and build workflow.
- Preserve the Python/Tkinter recorder as an external behavioral reference until capture parity is demonstrated.

## 3. Non-goals

- Porting the existing Python/Tkinter recorder.
- Implementing X11 or Wayland capture.
- Implementing FFmpeg process orchestration.
- Implementing Gemini/Ollama or any AI workflow.
- Implementing timeline editing, preview, or export.
- Implementing updater, telemetry, account, cloud sync, or remote project storage.
- Choosing final Linux distribution formats.
- Introducing a mandatory Python runtime or worker.

## 4. Application structure

The implementation should separate at least these areas:

- src/: React + TypeScript presentation.
- src-tauri/: Tauri configuration and Rust application entry point.
- Rust application modules: command handlers, domain/application services, typed errors, and infrastructure adapters.
- Tests: frontend tests and Rust unit/integration tests appropriate to the implemented boundaries.

Exact directory/module names may be refined during implementation, but responsibilities must remain separated. Avoid putting business rules directly in React components or putting UI concerns in Rust domain services.

## 5. Initial application experience

On launch, the app must:
- Display Screenstory branding and a usable application window.
- Render a minimal project dashboard/empty state.
- Clearly communicate that no project is currently open.
- Provide a visible action to create a project only if the project-bootstrap contract is implemented in this milestone; otherwise show the action as unavailable or defer it rather than faking success.
- Show application-level errors in a user-readable way without exposing stack traces or secrets.

The initial shell must remain responsive during native operations. Long-running work must not block the UI thread.

## 6. Native command boundary

- Expose only explicit application operations required by the current UI.
- Use typed request/response structures.
- Validate all inputs in Rust; do not trust frontend validation as a security boundary.
- Return normalized, user-safe errors with stable error codes and safe messages.
- Do not expose arbitrary shell execution, arbitrary process spawning, unrestricted filesystem access, or generic command passthrough to the frontend.
- Do not expose secrets, environment variables, internal paths, or stack traces in ordinary UI responses.
- Commands must have unit tests for validation and failure behavior.

The initial shell may expose a minimal health/version command to verify the bridge. Project creation is included only after the project-bootstrap contract is explicitly defined.

## 7. Tauri capabilities and security

- Use Tauri v2 capability configuration with least privilege.
- Deny permissions by default and grant only the APIs required by the current window and feature.
- Do not add shell, filesystem, dialog, process, or updater plugins speculatively.
- If a plugin is later required, document the exact operation, scope, permissions, and threat implications before enabling it.
- Keep frontend assets local; no remote frontend content is required for the initial shell.
- Follow SPEC-015 for security/privacy requirements.
- Treat data crossing the UI/native boundary as untrusted input.

## 8. Project workspace boundary

This milestone may define the project workspace service interface and path policy, but must not invent or prematurely freeze the full project file format.

Before implementing project creation, define:
- User-selected or application-managed project root behavior.
- Directory creation and collision handling.
- Atomic initialization and rollback behavior on failure.
- Path validation and symlink considerations.
- Project metadata schema/version.
- Tests for invalid paths, existing directories, permission errors, and partial initialization.

The project workspace remains local-first. Large media assets must not be stored inside SQLite.

## 9. Linux build prerequisites

- Use the official Tauri v2 Linux prerequisites as the baseline.
- Verify WebKitGTK 4.1 and native build dependencies on the actual Linux Mint development target before declaring the environment ready.
- Record OS version, architecture, Rust toolchain, Node/package-manager versions, and Tauri CLI version in verification evidence.
- Do not install system dependencies silently. Any system-level installation requires explicit user approval.
- Do not claim support for other Linux distributions based solely on a successful build on the initial target.

## 10. Quality and testing

- Rust formatting and lint checks must pass.
- Frontend type checking and lint checks must pass.
- Rust unit tests must cover command validation and error normalization.
- Frontend tests must cover the initial shell's key states.
- A development build must launch on the target Linux environment.
- A production build must complete on the target environment before M1 is marked verified.
- Tests must not require live network services.
- Record exact commands, versions, results, and known limitations in the task verification report.

## 11. Acceptance criteria

1. A Tauri v2 + React + TypeScript + Rust application builds successfully on the target Linux environment.
2. The application launches and renders the initial Screenstory shell.
3. The frontend can call a minimal typed Rust command and display its result.
4. Invalid command input is rejected by Rust and produces a normalized safe error.
5. Tauri capabilities are explicitly configured with least privilege; no broad shell or filesystem access is exposed.
6. The UI remains responsive while an asynchronous native operation is pending.
7. Rust tests, frontend type checks, frontend tests, and lint checks pass.
8. The project has a documented, reproducible development and build workflow.
9. No capture, AI, or export functionality is represented as implemented when it is not.
10. Verification evidence records the actual target environment and build results.

## 12. Implementation sequencing

- M1-T1 — Environment readiness: inspect OS/toolchain and verify Tauri v2 WebKitGTK 4.1 prerequisites. No application scaffold until this gate passes or an explicitly approved remediation plan exists.
- M1-T2 — Application scaffold: create the Tauri v2 + React + TypeScript + Rust skeleton and verify build/launch.
- M1-T3 — Native boundary: implement the minimal typed health/version command, normalized errors, and least-privilege capabilities.
- M1-T4 — Initial shell: implement the Screenstory empty-state dashboard and responsive loading/error states.
- M1-T5 — Quality gates: add/verify Rust and frontend checks, document repeatable commands, and record M1 evidence.

Each task must have its own handoff and verification evidence. A task cannot enter Ready until this specification is approved and its acceptance criteria/dependencies are explicit.

## 13. Open decisions

- Which package manager and lockfile policy will be selected for the React frontend?
- Which frontend test runner and component-testing approach will be used?
- Which Rust error representation will be standardized across command boundaries?
- Should project creation be included in M1 or deferred to the project-management specification?
- Which exact Linux Mint release and architecture will be the initial build baseline?
