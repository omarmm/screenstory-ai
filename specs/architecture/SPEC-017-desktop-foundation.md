# SPEC-017 — Tauri Desktop Foundation

**Status:** Approved  
**Milestone:** M1 — Desktop Foundation  
**Related:** PRD-001, ADR-001, ADR-010, ARCH-001, SPEC-002, SPEC-015, SPEC-016

## 1. Purpose

Define the minimum production desktop foundation for Screenstory using Tauri v2, React, TypeScript, and Rust. This specification covers the application shell, packaging, dependency readiness, and security boundaries. It does not authorize production screen capture, AI analysis, timeline editing, or video export.

## 2. Goals

- Create a launchable Linux desktop application using the accepted stack.
- Keep frontend presentation separate from Rust application/domain logic.
- Establish a narrow, typed, validated command boundary.
- Apply least-privilege Tauri v2 capabilities.
- Detect required and optional runtime dependencies automatically.
- Ask the user before installing any missing optional runtime dependency.
- Use Linux package metadata to resolve mandatory libraries required before the app can launch.
- Establish a testable project workspace bootstrap boundary without implementing the complete project domain.
- Provide a reproducible CI-based build, test, and packaging workflow.
- Preserve the Python/Tkinter recorder as an external behavioral reference until capture parity is demonstrated.

## 3. Non-goals

- Asking the product owner to manually prepare or validate a development machine as a prerequisite to implementation.
- Installing Rust, Node.js, compilers, or development headers on end-user machines.
- Porting the existing Python/Tkinter recorder.
- Implementing X11 or Wayland capture in this milestone.
- Implementing FFmpeg process orchestration.
- Implementing Gemini/Ollama or any AI workflow.
- Implementing timeline editing, preview, or export.
- Implementing updater, telemetry, account, cloud sync, or remote project storage.
- Introducing a mandatory Python runtime or worker.

## 4. Application structure

The implementation should separate at least these areas:

- src/: React + TypeScript presentation.
- src-tauri/: Tauri configuration and Rust application entry point.
- Rust application modules: command handlers, domain/application services, typed errors, dependency readiness, and infrastructure adapters.
- Tests: frontend tests and Rust unit/integration tests appropriate to the implemented boundaries.

Exact directory/module names may be refined during implementation, but responsibilities must remain separated. Avoid putting business rules directly in React components or putting UI concerns in Rust domain services.

## 5. Initial application experience

On launch, the app must:
- Display Screenstory branding and a usable application window.
- Render a minimal project dashboard/empty state.
- Clearly communicate that no project is currently open.
- Provide a visible action to create a project only if the project-bootstrap contract is implemented in this milestone; otherwise defer it rather than faking success.
- Show application-level errors in a user-readable way without exposing stack traces or secrets.
- Run a dependency readiness check and show the status of relevant features.

The initial shell must remain responsive during native operations. Long-running work must not block the UI thread.

## 6. Native command boundary

- Expose only explicit application operations required by the current UI.
- Use typed request/response structures.
- Validate all inputs in Rust; do not trust frontend validation as a security boundary.
- Return normalized, user-safe errors with stable error codes and safe messages.
- Do not expose arbitrary shell execution, arbitrary process spawning, unrestricted filesystem access, or generic command passthrough to the frontend.
- Do not expose secrets, environment variables, internal paths, or stack traces in ordinary UI responses.
- Commands must have unit tests for validation and failure behavior.

The initial shell may expose a minimal health/version command and a typed dependency-readiness report.

## 7. Dependency readiness and installation policy

### 7.1 Dependency classes

Every dependency must be classified as one of:

1. **Build-time only:** Rust toolchain, Node.js, frontend package manager, compilers, linker, pkg-config, development headers, and Tauri build prerequisites. These belong in CI/developer build images, not on end-user devices. The end user must never be asked to install these to run Screenstory.
2. **Mandatory launch-time system libraries:** libraries required by the Tauri application process before its window can be created. These cannot reliably be checked by an in-app screen because the application may fail to launch if they are absent.
3. **Required feature runtime dependencies:** for example FFmpeg when recording, probing, frame extraction, or export is invoked. Check before enabling the affected feature.
4. **Optional feature dependencies:** utilities or integrations that enhance a feature but are not required for the core application. Explain the impact when absent.

### 7.2 Mandatory launch-time libraries and packaging

- The primary Linux Mint distribution artifact should be a Debian package (.deb).
- Declare mandatory Tauri runtime dependencies in package metadata so the system package manager resolves them during installation, before first launch.
- For Tauri v2 Debian packaging, verify the generated package declares the required WebKitGTK 4.1 and GTK runtime libraries. Include tray-related runtime libraries only if the application actually enables a tray.
- Do not rely on a post-launch prompt to repair a missing WebKitGTK library: the app may not start far enough to display that prompt.
- Build release artifacts in a controlled CI environment using a documented Linux baseline. The product owner's personal machine is not a build prerequisite or manual validation gate.
- If a mandatory dependency cannot be satisfied by the supported package repositories, fail installation clearly or provide a separately approved packaging strategy; never silently download arbitrary system libraries.

### 7.3 Runtime detection

- The Rust dependency service owns runtime checks; React only displays typed results.
- Check dependencies at startup and again immediately before a feature needs them.
- Detect executable presence, version, required capabilities, and actual invocation health where practical; a binary merely existing on PATH is not sufficient evidence.
- Return a structured report containing dependency ID, feature, status (available/missing/unsupported/error), detected version, impact, and supported remediation.
- Do not run privileged checks or installation commands during ordinary startup.
- Cache results only for the current app session or invalidate them after a package operation; never assume a dependency remains available indefinitely.

### 7.4 User-consented installation

- When a required feature dependency is missing, show a clear explanation of the feature affected, package name(s), approximate download/installation impact when available, and the proposed system changes.
- Offer Install and Not now. The user must explicitly choose Install before any package-manager process or privilege escalation.
- On Linux Mint, use the supported APT/dpkg integration path. Prefer the system's package-management service when available; otherwise use a fixed, allowlisted apt operation through a PolicyKit-mediated authorization flow.
- Never use a shell command string, shell interpreter, arbitrary package names from the UI, or an unrestricted command bridge. Construct argument arrays from an application-maintained allowlist.
- Do not add third-party repositories, PPAs, signing keys, or download packages from arbitrary URLs automatically.
- Show progress and the package manager's meaningful result. Handle cancellation, authentication denial, package-manager locks, unavailable repositories, network failure, and partial failure.
- After installation, rerun the dependency check and enable the feature only when verification succeeds.
- If installation is declined or fails, keep unrelated application features usable and provide a clear manual remediation path.
- Installation must not be attempted repeatedly in a loop. Ask again only after a deliberate user action or a later feature invocation.
- Record package operations without storing secrets or unnecessary personal data.

### 7.5 Security

- Dependency installation is a privileged boundary and must be implemented in Rust behind a narrow, auditable service.
- The frontend cannot choose arbitrary executables, package names, flags, paths, or commands.
- Tauri capabilities remain least-privilege; do not expose a generic shell plugin to the frontend.
- User approval is required for each package transaction; prior approval for one dependency does not authorize future unrelated installs.
- Follow SPEC-015 for security/privacy requirements.

## 8. Tauri capabilities and security

- Use Tauri v2 capability configuration with least privilege.
- Deny permissions by default and grant only the APIs required by the current window and feature.
- Do not add shell, filesystem, dialog, process, or updater plugins speculatively.
- If a plugin is later required, document the exact operation, scope, permissions, and threat implications before enabling it.
- Keep frontend assets local; no remote frontend content is required for the initial shell.
- Treat data crossing the UI/native boundary as untrusted input.

## 9. Project workspace boundary

This milestone may define the project workspace service interface and path policy, but must not invent or prematurely freeze the full project file format.

Before implementing project creation, define:
- User-selected or application-managed project root behavior.
- Directory creation and collision handling.
- Atomic initialization and rollback behavior on failure.
- Path validation and symlink considerations.
- Project metadata schema/version.
- Tests for invalid paths, existing directories, permission errors, and partial initialization.

The project workspace remains local-first. Large media assets must not be stored inside SQLite.

## 10. Build and delivery workflow

- Build frontend and Rust artifacts in CI; do not require the end user to install development toolchains.
- Use official Tauri v2 Linux build prerequisites in the CI/build image.
- Produce a .deb package as the primary Linux Mint distribution artifact.
- Verify generated package metadata includes mandatory launch-time runtime dependencies.
- Keep build-time development dependencies out of end-user package dependencies.
- Record build environment, toolchain versions, package metadata, artifact checksums, and test results in CI evidence.
- Do not claim compatibility with Linux distributions outside the declared support matrix.

## 11. Quality and testing

- Rust formatting and lint checks must pass.
- Frontend type checking and lint checks must pass.
- Rust unit tests must cover command validation, dependency classification, and error normalization.
- Dependency manager tests must use a fake package-manager adapter; unit tests must never modify the host system.
- Integration tests must cover missing dependencies, user decline, approval, cancellation, package-manager lock, failure, and post-install recheck.
- Frontend tests must cover the initial shell and dependency states.
- CI must produce a development build and a production .deb artifact.
- Tests must not require live network services except explicitly isolated packaging/integration checks.
- Record exact commands, versions, results, and known limitations in the task verification report.

## 12. Acceptance criteria

1. A Tauri v2 + React + TypeScript + Rust application builds in CI.
2. The application launches on a supported Linux Mint installation with mandatory runtime libraries declared in the .deb metadata.
3. The frontend can call a minimal typed Rust command and display its result.
4. Invalid command input is rejected by Rust and produces a normalized safe error.
5. Tauri capabilities are explicitly configured with least privilege; no broad shell or filesystem access is exposed.
6. Runtime dependency readiness is checked and represented as structured, feature-specific status.
7. Missing optional/feature dependencies trigger a user-consent prompt before any package operation.
8. Declining installation leaves unrelated features usable.
9. Approved installation uses only fixed allowlisted package operations through the supported privilege boundary.
10. The app rechecks the dependency after installation and enables the feature only after successful verification.
11. No in-app check is relied upon for mandatory libraries that must exist before the Tauri window can launch.
12. Rust tests, frontend type checks, frontend tests, lint checks, and CI packaging checks pass.
13. No capture, AI, or export functionality is represented as implemented when it is not.

## 13. Implementation sequencing

- M1-T1 — CI/build baseline: define the reproducible Tauri build environment and target package baseline. No manual setup or validation task is assigned to the product owner.
- M1-T2 — Application scaffold: create the Tauri v2 + React + TypeScript + Rust skeleton and build it in CI.
- M1-T3 — Native boundary: implement typed commands, normalized errors, and least-privilege capabilities.
- M1-T4 — Dependency readiness: implement dependency registry, structured checks, package-manager adapter, explicit consent UI, and safe post-install verification.
- M1-T5 — Initial shell: implement the Screenstory empty-state dashboard and dependency status.
- M1-T6 — Quality and packaging gates: verify tests, .deb metadata, and reproducible CI artifacts.

Each task must have its own handoff and verification evidence. A task cannot enter Ready until this specification is approved and its acceptance criteria/dependencies are explicit.

## 14. Approved implementation choices

- Frontend package manager: pnpm, with a committed lockfile and pinned package-manager version.
- Frontend test runner: Vitest with React Testing Library for component behavior.
- Rust command error model: application-owned typed error codes and safe messages; use thiserror for internal error composition. Do not serialize arbitrary error strings or stack traces to the frontend.
- Project creation: defer to SPEC-001. M1 provides a truthful empty-state shell and does not fake project creation.
- Initial Linux support baseline: Linux Mint 21.3+ (Ubuntu 22.04 base) and Linux Mint 22.x (Ubuntu 24.04 base), x86_64, subject to CI package/build verification. Revisit if a required WebKitGTK 4.1 package is unavailable in the official base repositories.
- Primary distribution format: .deb. Evaluate Flatpak only after the .deb path is working and dependency/portal behavior is understood.

## 15. Approval note

The specification is approved for implementation sequencing. Approval authorizes M1 tasks only; each task must still meet its own handoff, test, and verification requirements.
