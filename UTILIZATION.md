# Utilization — eigenstyle 1.0.0 (D-116 evidence)

Eigenstyle's consumer is the estate's own writing workflow
(Matt-voice drafting: pull the profile and exemplars, draft against
them, then run the deterministic checker before anything ships in
Matt's name). The session below is that workflow, run for real
against the 1.0.0 release binary — a scripted modern-era stdio
client, a scratch store seeded from the in-tree corpus, and the
in-tree JAS holdout fixture as the draft under check.

- Date: 2026-10-28 (night run, fleet-to-1.0 program G-E2)
- Binary: `eigenstyle 1.0.0` (release build of the 1.0.0 tree)
- Client shape: `server/discover` first, per-request
  `_meta["io.modelcontextprotocol/protocolVersion"] = "2026-07-28"`
  on every call — the modern-era pattern the fleet's clients use.
- Store: fresh scratch home; the server seeded it from the
  in-tree corpus on first open.

## Transcript summary (9/9 PASS)

| Step | Call | Result |
| --- | --- | --- |
| 1 | `server/discover` | `supportedVersions == ["2026-07-28"]` exactly |
| 2 | `server/discover` serverInfo | `{"name": "eigenstyle", "version": "1.0.0"}` |
| 3 | `tools/list` | the six voice tools: `check_draft`, `get_exemplars`, `get_profile`, `ingest_sample`, `list_corpus`, `recompute_fingerprint` |
| 4 | `list_corpus` | seed corpus live: 6 samples, 7,674 included words |
| 5 | `get_profile` | readable profile + fingerprint summary + avoid list returned (the drafting brief an agent writes against) |
| 6 | `get_exemplars` (`genre: "scholarly"`, limit 2) | 2 verbatim exemplar paragraphs returned (the retrieval bank) |
| 7 | `check_draft` (text = `crates/eigenstyle-core/tests/fixtures/jas_holdout.txt`) | delta **1.1962** — matches the Python-parity fixture delta 1.196; verdict bands close_max 1.779 / drifting_max 2.668 (verdict CLOSE); avoid-list hit reported (`pivotal` ×2); integrity clean (no dropped numbers/URLs) |
| 8 | `recompute_fingerprint` (`dry_run: true`) | preview returned with bands before/after identical (1.779 / 2.668) and no changed features — nothing saved |
| 9 | legacy probe: `initialize` with `protocolVersion: "2025-06-18"`, no `_meta` | refused with typed error `-32022` "Unsupported protocol version", `supported: ["2026-07-28"]` |

Driver exit code 0 (`RESULT: ALL PASS`). No step was stubbed or
mocked: every call hit the release binary over stdio against the
real seeded store, and step 7's delta is checked against the
parity fixture the core test suite also gates on.
