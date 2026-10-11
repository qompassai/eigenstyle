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
- [x] 3. voice-mcp: stdio JSON-RPC server, 6 tools, protocol tests, smoke.
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
- Stage 3 (2026-10-11): voice-mcp server done. cargo test -p voice-mcp:
  3 unit (handshake/catalog, adversarial protocol, strict args) + 2
  binary smoke (full client round trip incl. ingest → dry-run →
  recompute → check with holdout delta 1.196; adversarial frames).
  All pass. Two test-harness bugs fixed en route (shared temp dir race;
  a hand-counted word expectation — the engine's 70 was right).
- Stage 4 (2026-10-11): voice-tui done — 5 screens over a
  terminal-independent App state machine; `--snapshot <screen>` headless
  render via TestBackend. cargo test -p voice-tui: 6 pass (seed list +
  toggle, add flow, holdout check CLOSE, fingerprint preview→apply +
  correction cycle, snapshot contains seed titles). Snapshot artifact:
  screenshots/tui-corpus-snapshot.txt.
- Stage 5 (2026-10-11): voice-gui done — gpui-ce =0.2.2 +
  gpui_ce_platform =0.1.0 (wayland+x11). Rendering probe first: Xvfb/X11
  presents black with this build; weston headless panics gpui's wayland
  client; cage (wlroots) headless works. Real GUI smoke under cage:
  corpus view screenshot verified visually (6 samples, 7,674 words,
  Python-exact per-piece counts) — screenshots/gui-corpus.png;
  fingerprint view verified (bands CLOSE ≤ 1.779 / DRIFTING ≤ 2.668 =
  the recorded bands; per-genre values match the profile) —
  screenshots/gui-fingerprint.png. Text entry = click-to-focus,
  append/backspace (gpui-ce 0.2 TextField absent; stated limit).
- Stage 6 (2026-10-11): cargo fmt clean; clippy --workspace
  --all-targets -D warnings: 0 errors (11 mechanical lints fixed);
  cargo test --workspace: 33 tests, 0 failures (core 18+4, mcp 3+2,
  tui 6). Release build of all three binaries clean.
- IDE gate (2026-10-11): headless nvim on primo, live diver config,
  workspace files open — clients attached include rust_analyzer,
  bacon-ls, crates-lsp; diagnostics: errors=0, warnings=0. (Four
  harness attempts preceded the pass: headless nvim over SSH exits
  without a pty, and a `-S` script runs after VimEnter — final shape:
  `script -qec` pty + top-level defer_fn.)
