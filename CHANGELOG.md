# Changelog — eigenstyle

## 1.0.0 — 2026-10-28

The 1.0 release line for the estate MCP fleet (fleet-to-1.0 program,
G-E1/G-E2).

- Version line: `eigenstyle` (MCP server) 0.2.0 → 1.0.0;
  `eigenstyle-gui` and `eigenstyle-tui` 0.1.0 → 1.0.0 (workspace
  version). `eigenstyle-core` stays at 0.1.0 — already published on
  crates.io; it does not move with this release.
- Packaging: the workspace dependency on `eigenstyle-core` now
  carries an explicit `version = "0.1.0"` requirement (path-only
  dependencies cannot be published), and `eigenstyle-core`'s own
  version is pinned explicitly at 0.1.0 in its manifest.
  `cargo publish --dry-run` passes for `eigenstyle`,
  `eigenstyle-gui`, and `eigenstyle-tui` from this tree.
- No tool-surface change from 0.2.0: the same six tools
  (`check_draft`, `get_exemplars`, `get_profile`, `ingest_sample`,
  `list_corpus`, `recompute_fingerprint`), same schemas, same
  on-disk corpus/fingerprint formats.
- Docs brought current with the migrated surface: README protocol
  section now describes the rmcp / modern-era (2026-07-28) wire
  instead of the pre-migration hand-rolled framing; COMPLIANCE.md
  standing updated (per-row Inspector-sweep statuses unchanged —
  no new sweep has been run).
- `UTILIZATION.md` added: a real scripted stdio session against
  the 1.0.0 binary exercising the voice tools against the in-tree
  fixtures (the estate writing workflow is this server's consumer).
- Registry `server.json` updated to 1.0.0.
- Re-gated on both estate machines at 1.0.0 (tests, fmt, clippy
  `-D warnings`); see `UTILIZATION.md` and the gate record in the
  landing report.

## 0.2.0 — 2026-10-28

- WS1 migration (estate MCP standardization, D-113): the server
  moved from hand-rolled stdio framing to rmcp 3.8.0, modern
  protocol era only (2026-07-28). Sessions open with
  `server/discover`; every request carries its protocol version in
  `_meta`; legacy `initialize` contact is refused with the typed
  `-32022` unsupported-version error. Tool names, schemas, and
  behavior are unchanged; corpus and fingerprint formats unchanged.

## 0.1.0 — 2026-10-11

- First build: `eigenstyle-core` (corpus store, stylometry,
  fingerprint build with leave-one-out bands, draft checker with
  integrity and avoid-list checks, exemplar retrieval, bounded URL
  fetch) with a Python-parity gate against the matt-voice skill's
  reference numbers; the six-tool stdio MCP server; the ratatui
  TUI; and the GPUI desktop GUI. Seed corpus: 6 pieces, 7,674
  words.
