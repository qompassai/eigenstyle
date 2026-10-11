# voice-mcp — build progress

Commissioned 2026-10-11 (Matt): full voice-profile MCP server with TUI and
GPUI GUI, add-writing flows, tooling adapted from HumanifyMe / brandvoice-mcp /
book-writer-mcp / Eden / Arcitext. Seed: `~/workspace/skills/matt-voice/`
(Python stylometry + checker = parity reference).

## Stages

- [x] 0. Scaffold: workspace (voice-core, voice-mcp, voice-tui, voice-gui),
  toolchain pin 1.99.0, seed corpus + fingerprint + profile imported,
  Python parity fixtures generated (per-piece + pooled fingerprints,
  checker deltas: JAS holdout 1.196, Cureus 1.088, synthetic 1.253).
- [x] 1. voice-core: stylometry port, corpus store, fingerprint build
  (LOO bands), checker (+ integrity + avoid-list), exemplars, URL fetch.
- [x] 2. Parity gate: Rust engine vs Python fixtures on the seed corpus.
- [ ] 3. voice-mcp: stdio JSON-RPC server, 6 tools, protocol tests, smoke.
- [ ] 4. voice-tui: ratatui front-end, snapshot smoke.
- [ ] 5. voice-gui: gpui-ce 0.2.2 front-end, Xvfb smoke + screenshot.
- [ ] 6. Gates: cargo test totals, clippy -D warnings, fmt, IDE gate on
  primo (headless nvim, live diver config), README, local commits.

## Decisions / bounds

- Persistence: JSON store under `$VOICE_MCP_HOME` (default
  `~/.local/share/voice-mcp/`), atomic temp+rename writes, append-only
  samples (exclude-from-fingerprint flag instead of deletion).
- Sample bounds: text ≤ 256 KiB, ≥ 10 tokens; draft check ≤ 1 MiB,
  < 200 words = UNRELIABLE verdict (mirrors the Python checker).
- Fingerprint recompute needs ≥ 2 included pieces (LOO calibration is
  undefined below that) — fail closed with a structured error.

## Log

- Stage 1–2 (2026-10-11): voice-core complete; cargo test -p voice-core:
  18 unit + 4 parity integration tests, all pass on primo (1.99.0).
  Parity: per-piece + pooled fingerprints match Python within one
  rounding unit (counts exact); built fingerprint == recorded
  fingerprint.json (features identical, reference/bands/LOO within
  0.002); checker deltas on holdouts match Python (1.196 / 1.088 /
  1.253). One real bug found+fixed by the gate: word counts must be
  taken after ligature normalization (7693 vs 7674 before the fix).
