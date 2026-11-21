# SPEC-002 — Linux Screen and Window Capture

**Status:** Draft — technical spike required

## Scope
Capture a selected desktop display or application window on Linux.

## Requirements
- Expose a unified capture-source interface.
- Wayland implementation uses XDG Desktop Portal ScreenCast and PipeWire.
- X11 implementation is isolated behind a separate adapter.
- Enumerate sources where the platform API permits.
- Request and handle user permissions.
- Report unsupported source types and portal failures clearly.
- Capture lifecycle must not depend on AI or network services.
- Global keyboard/mouse hooks are not required.

## Open questions
- Which Linux distribution and desktop environment are the primary target?
- What window-selection capabilities are exposed by its portal implementation?
- Which PipeWire stream formats and frame rates are available?
- What packaging/runtime dependencies are required?

## Acceptance criteria
- Full desktop recording works on the target Wayland environment.
- Window recording works where supported by the portal.
- X11 capture is tested independently.
- Permission denial and stream interruption are handled.
- Original output is playable and metadata is recorded.
