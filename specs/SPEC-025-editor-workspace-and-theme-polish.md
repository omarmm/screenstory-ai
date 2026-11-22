# SPEC-025 — Editor Workspace and Theme Polish

**Status:** In progress  
**Branch:** `feat/ui-polish-timeline-themes`  
**Baseline:** `baseline/0.5.3-automatic-captions`

## 1. Goals

Improve the desktop editor layout and visual controls without changing recording, Gemini analysis, timeline persistence, zoom composition, caption timing, or FFmpeg export behavior.

## 2. Centered timeline workspace

- The timeline editor occupies the full content row rather than the narrow secondary column.
- The editor is horizontally centered with a readable maximum width.
- Tutorial steps, zoom suggestions, and captions remain vertically stacked in clear cards.
- The layout remains responsive on smaller windows.

## 3. Batch caption appearance

- Provide a shared appearance panel when a timeline contains captions.
- Allow applying one font size, text color, background color, and background opacity to every caption in a single action.
- Preserve each caption's text, timing, language, direction, position, and alignment when applying shared appearance.
- Keep all per-caption controls available after batch changes.
- Provide native color pickers for individual text and background colors and an opacity control for the background.
- Batch edits remain ordinary unsaved timeline edits and use the existing explicit Save Revision flow.

## 4. Appearance themes

- Provide a visible Dark/Light theme toggle in the application header.
- Persist the selected theme locally between application launches.
- Ensure panels, cards, text inputs, number inputs, color controls, selects, and native select options use readable foreground/background contrast in each theme.
- Keep keyboard focus indicators visible in both themes.
- Do not change exported video colors or caption styling when switching the application theme.

## 5. Acceptance criteria

1. The timeline editor is centered and spans the available content width instead of appearing as a narrow right-hand column.
2. Tutorial steps, zoom suggestions, and captions remain vertically ordered.
3. Applying shared caption appearance updates every caption's font size, text color, background color, and opacity without changing its text or timing.
4. Users can still edit any single caption after using shared appearance controls.
5. Individual caption text/background colors use native color pickers; background opacity remains adjustable.
6. Dark and Light themes can be switched from the header and the choice persists after restart.
7. Native dropdown menus and options remain readable in both themes.
8. Existing recording, analysis, zoom, versioned timeline save, and export flows remain unchanged.
