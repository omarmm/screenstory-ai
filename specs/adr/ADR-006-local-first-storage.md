# ADR-006 — Local-first Storage

**Status:** Approved

## Decision
Use SQLite for metadata and the local filesystem for media assets.

## Rationale
No database server, simple backup, and local ownership of recordings.

## Consequences
File/database consistency and recovery behavior require explicit specifications.
