# SPEC-002 — Linux Screen and Window Capture

**Status:** Draft — technical spike required

## Scope
Capture a selected desktop display or application window on Linux.

The product is Linux distribution-agnostic. Linux Mint with Cinnamon on X11 is the initial validation environment only; it is not a runtime or product restriction. Pop!_OS, Ubuntu, Fedora, and other compatible distributions remain in scope, subject to capability validation.

## Architecture Decisions

- Provide a unified capture-source interface to the recording domain.
- Select capture implementations by detected display-server capabilities, not by Linux distribution name or version.
- Keep X11 and Wayland implementations isolated behind separate adapters.
- X11 adapter: evaluate FFmpeg `x11grab` for full-display capture and window capture. Window identification, geometry changes, and resizing behavior must be validated in the technical spike.
- Wayland adapter: use XDG Desktop Portal ScreenCast for source selection/permission flow and PipeWire for receiving the stream.
- Detect required runtime capabilities and dependencies at startup; report missing or unsupported capabilities clearly instead of assuming a specific distro package layout.
- Provide a dependency-readiness check before recording. Identify missing required and optional components and explain which feature requires each component.
- When a required dependency is missing, offer an assisted installation/remediation flow appropriate to the detected distribution/package manager. The user must explicitly approve installation before any system changes or privilege escalation.
- Never silently run package-manager commands, request administrator privileges without user action, or install unrelated packages.
- If automatic installation is unsupported or fails, show exact actionable manual instructions and allow the user to retry the readiness check.
- Prefer application-bundled or app-managed dependencies only where packaging, security updates, architecture support, and FFmpeg licensing obligations have been reviewed. Do not assume bundling is always preferable.
- Distinguish mandatory runtime dependencies from optional diagnostic/development tools. For example, `xwininfo` may be used in the spike but must not become an application runtime dependency unless separately justified.
- Treat Linux Mint Cinnamon X11 as the first test environment, not the only supported environment.
- Do not require a specific Linux distribution release as an architectural constraint.

## Requirements

- Expose a unified capture-source interface.
- Wayland implementation uses XDG Desktop Portal ScreenCast and PipeWire.
- X11 implementation is isolated behind a separate adapter.
- Enumerate sources where the platform API permits.
- Request and handle user permissions.
- Report unsupported source types and portal failures clearly.
- Capture lifecycle must not depend on AI or network services.
- Global keyboard/mouse hooks are not required.
- Support distribution-agnostic runtime capability detection.
- Keep platform-specific dependencies and behavior inside the corresponding adapter.
- Detect missing dependencies before a capture attempt and communicate their impact.
- Provide user-consented dependency installation or clear manual remediation.
- Re-check capabilities after installation and report success or failure.

## Open questions

- Which minimum versions/capabilities of FFmpeg, XDG Desktop Portal, and PipeWire are required?
- Which window-selection capabilities are exposed by portal implementations on target Wayland desktops?
- Which PipeWire stream formats and frame rates are available across target environments?
- What packaging strategy best handles dependencies across distributions?
- Which dependencies should be bundled, app-managed, or provided by the host package manager?
- Which package managers and installation paths will the MVP officially support?
- How will Screenstory request and scope elevated privileges safely through supported package-manager mechanisms?
- How will X11 window identity and geometry be tracked if the selected window moves or resizes?
- Which additional distributions/desktops should be included in the compatibility test matrix after the initial Linux Mint environment?

## Acceptance criteria

- Full desktop recording works on the initial target X11 environment.
- Window recording works on X11, including documented behavior when the target window moves or resizes.
- Wayland full-desktop recording works on at least one validated portal/PipeWire environment before claiming Wayland support.
- Window recording works on Wayland where the portal implementation supports it; unsupported cases are reported clearly.
- X11 and Wayland capture are tested independently.
- At least one additional Linux distribution is included in compatibility validation before claiming broad distro support.
- Permission denial and stream interruption are handled.
- Original output is playable and metadata is recorded.
- Missing runtime capabilities produce actionable diagnostics.
- Missing required dependencies are detected before capture and the user is offered a consent-based remediation path.
- No dependency installation or privilege escalation occurs without explicit user approval.

## Technical Spike

No production implementation is authorized by this specification alone. The technical spike must validate:
1. X11 full-display capture using the proposed X11 adapter.
2. X11 window capture and behavior under move/resize.
3. Required runtime dependencies and capability detection.
4. Output video playability and metadata.
5. A separate Wayland proof-of-capability using XDG Desktop Portal and PipeWire.
6. Dependency detection behavior when FFmpeg is absent, including a user-facing remediation recommendation. The spike must not perform unattended system installation.

The spike results must be recorded before this specification can move to Approved.
