# ADR-002 — Linux Capture Strategy

**Status:** Proposed for technical sign-off

## Decision
Wayland-first capture using XDG Desktop Portal and PipeWire, with a separate X11 compatibility adapter.

## Rationale
Modern Linux support and isolation of display-server-specific behavior.

## Consequences
Window capture behavior varies by portal and desktop environment. User permission may be required. Capture must be validated on the actual target system. Global keyboard/mouse monitoring is not required for MVP.
