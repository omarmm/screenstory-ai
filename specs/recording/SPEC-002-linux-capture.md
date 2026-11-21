# SPEC-002 — Linux Screen and Window Capture

**Status:** Draft — X11 mechanism selected for implementation spike; Wayland validation pending

## Scope
Capture a selected desktop display or application window on Linux.

The product is Linux distribution-agnostic. Linux Mint with Cinnamon on X11 is the initial validation environment only; it is not a runtime or product restriction. Pop!_OS, Ubuntu, Fedora, and other compatible distributions remain in scope, subject to capability validation.

## Architecture Decisions

- Provide a unified capture-source interface to the recording domain.
- Select capture implementations by detected display-server capabilities, not by Linux distribution name or version.
- Keep X11 and Wayland implementations isolated behind separate adapters.
- **X11 display capture:** use an X11 display/root-window capture path. FFmpeg `x11grab` has already produced a playable full-display recording in the initial Linux Mint + Cinnamon + X11 environment.
- **X11 window capture:** use the XComposite extension through an X11-native adapter. Do not use FFmpeg `x11grab -window_id` as the production window-capture mechanism. The FFmpeg documentation explicitly states that enlarging a captured window does not add the newly exposed area and that closing/unmapping the window ends capture; local spike results also showed errors during resize/minimize.
- The XComposite approach is an established Linux window-capture mechanism used by OBS Studio's Linux X11 Window Capture source. This is an architectural reference, not permission to copy or link OBS implementation code. Any dependency or code reuse requires a separate license review.
- The X11 adapter must identify windows using stable window identity (not title alone), observe window lifecycle/geometry changes, and reacquire the selected window when it is temporarily unavailable where the platform permits.
- A minimized or destroyed X11 window may not provide capturable content. The product must represent temporary unavailability or termination explicitly; it must not silently substitute the desktop or another window.
- Wayland adapter: use XDG Desktop Portal ScreenCast for source selection/permission flow and PipeWire for receiving the stream. Window-source availability depends on the portal/compositor implementation.
- Keep encoding/muxing separate from source acquisition. FFmpeg may remain the encoder/muxer where its runtime capabilities are detected and validated; it is not the X11 window-selection/capture mechanism.
- Detect required runtime capabilities and dependencies at startup; report missing or unsupported capabilities clearly instead of assuming a specific distro package layout.
- Provide a dependency-readiness check before recording. Identify missing required and optional components and explain which feature requires each component.
- When a required dependency is missing, offer an assisted installation/remediation flow appropriate to the detected distribution/package manager. The user must explicitly approve installation before any system changes or privilege escalation.
- Never silently run package-manager commands, request administrator privileges without user action, or install unrelated packages.
- If automatic installation is unsupported or fails, show exact actionable manual instructions and allow the user to retry the readiness check.
- Prefer application-bundled or app-managed dependencies only where packaging, security updates, architecture support, and licensing obligations have been reviewed. Do not assume bundling is always preferable.
- Distinguish mandatory runtime dependencies from optional diagnostic/development tools. For example, `xwininfo` may be used in diagnostics but must not become an application runtime dependency unless separately justified.
- Treat Linux Mint Cinnamon X11 as the first test environment, not the only supported environment.
- Do not require a specific Linux distribution release as an architectural constraint.

## Requirements

- Expose a unified capture-source interface.
- X11 display capture and X11 window capture use separate source modes behind the X11 adapter.
- X11 window capture is based on XComposite, not FFmpeg `x11grab -window_id` or a continuously recalculated screen crop.
- X11 window capture tracks the selected window across movement and resizing, and reports minimized/destroyed/unavailable states accurately.
- Wayland implementation uses XDG Desktop Portal ScreenCast and PipeWire.
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

- Which minimum X11/XComposite/XCB capabilities and runtime libraries are required on supported distributions?
- Which XComposite window states can be captured reliably, especially minimized and off-workspace windows?
- Which native implementation/dependency has an acceptable license and maintenance profile for the Rust/Tauri application?
- Which minimum versions/capabilities of XDG Desktop Portal and PipeWire are required?
- Which window-selection capabilities are exposed by portal implementations on target Wayland desktops?
- Which PipeWire stream formats and frame rates are available across target environments?
- What packaging strategy best handles dependencies across distributions?
- Which dependencies should be bundled, app-managed, or provided by the host package manager?
- Which package managers and installation paths will the MVP officially support?

## Acceptance criteria

- Full desktop recording works on the initial target X11 environment.
- X11 window capture uses XComposite and produces a playable recording of the selected window.
- X11 window capture follows movement and handles resize without terminating or retaining stale dimensions.
- Occlusion by another window does not cause unrelated desktop content to replace the selected window capture.
- Minimized, destroyed, and temporarily unavailable windows produce defined, user-visible capture states; no silent fallback to another source occurs.
- Wayland full-desktop recording works on at least one validated portal/PipeWire environment before claiming Wayland support.
- Window recording works on Wayland where the portal implementation supports it; unsupported cases are reported clearly.
- X11 and Wayland capture are tested independently.
- At least one additional Linux distribution is included in compatibility validation before claiming broad distro support.
- Permission denial and stream interruption are handled.
- Original output is playable and metadata is recorded.
- Missing runtime capabilities produce actionable diagnostics.
- No dependency installation or privilege escalation occurs without explicit user approval.

## Technical Spike

The initial FFmpeg `x11grab -window_id` spike is conditionally unsuitable for production window capture. Its full-display path passed, but the window path did not satisfy resize/minimize lifecycle requirements. The next X11 spike must validate the XComposite-based mechanism before production integration.

The XComposite spike must validate:
1. XComposite extension and required XCB capabilities on Linux Mint + Cinnamon + X11.
2. Enumerating and selecting a top-level application window by stable identity.
3. Capturing the selected window while it is partially or fully occluded.
4. Following movement and resizing while preserving a playable output.
5. Defined behavior for minimize, restore, close, and window recreation.
6. Cursor behavior and output frame dimensions.
7. Runtime dependency detection and a license review for any selected third-party library.

A separate Wayland proof-of-capability must validate XDG Desktop Portal and PipeWire. No production implementation is authorized until the revised capture specification is approved and the XComposite spike has recorded evidence.
