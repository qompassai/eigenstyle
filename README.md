# voice-mcp — your voice profile as a tool

A Rust workspace that turns Matt Porter's measured writing fingerprint
(built 2026-10-11 in the `matt-voice` skill) into a living system: an
MCP server agents can call, a terminal UI, and a GPUI desktop GUI — all
over one core library, all fed by a corpus you keep adding to.

The fingerprint is *measured*, not vibes: sentence-length distribution,
function-word profile, punctuation rates, parentheticals, hyphenated
compounds, hedge/assertive balance, first-person rate — the same
features, tokenizer, rounding, and leave-one-out band calibration as
the skill's Python reference. A parity test suite proves the Rust
engine reproduces the Python numbers on the seed corpus (features and
bands identical; checker deltas on the held-out pieces match: JAS
1.196, Cureus 1.088, synthetic 1.253).

## The three front-ends

| Front-end | Binary | Run it |
|---|---|---|
| MCP server (stdio) | `voice-mcp` | `cargo run --release -p voice-mcp` (add `--home <dir>` to choose the store) |
| Terminal UI | `voice-tui` | `cargo run --release -p voice-tui` |
| Desktop GUI | `voice-gui` | `cargo run --release -p voice-gui` (Wayland session) |

All three share one store: `$VOICE_MCP_HOME`, else
`~/.local/share/voice-mcp/`. First open seeds it from the skill's
corpus (6 pieces, 7,674 words), fingerprint, and readable profile.

## MCP tools

| Tool | What it does |
|---|---|
| `check_draft` | Score a draft (text or file): verdict CLOSE / DRIFTING / OFF-VOICE / UNRELIABLE, delta, top contributing features, avoid-list hits. With `source_text`/`source_file_path`, also accounts for every number and URL from the source — dropped numbers and dropped/broken URLs are reported. |
| `get_exemplars` | Verbatim exemplar paragraphs from included samples of one genre (the retrieval bank agents draft against). |
| `get_profile` | The readable profile + fingerprint summary (effective values, which are user-corrected), bands, avoid list. |
| `ingest_sample` | Add writing (exactly one of `text` / `file_path` / `url`). Title, genre, and authorship (`sole`/`blended`) are required — attribution is never silently defaulted. Never recomputes the fingerprint. |
| `list_corpus` | The corpus manifest: metadata, word counts, included flags, totals. |
| `recompute_fingerprint` | Recompute from included samples and re-derive the bands. `dry_run: true` previews the induced feature deltas and band shifts without saving — review before you apply. |

Protocol: newline-delimited JSON-RPC 2.0 over stdio, the estate's
research-mcp pattern. Fail-closed throughout: malformed frames,
unknown tools, strict argument schemas, named size bounds everywhere.

## Adding your writing

- **MCP**: an agent calls `ingest_sample` (or you ask for a URL fetch —
  the core fetches it, bounded, HTML reduced to text).
- **TUI**: `Add writing` screen (key `2`) — title, genre, authorship
  toggle, source, then a file path (Enter loads it) or pasted text;
  Ctrl+S saves.
- **GUI**: `Add writing` tab — same fields as clickable boxes; the
  authorship button toggles sole/blended; **Add sample** saves.

In every flow the same rules hold: samples are append-only (exclude a
sample from the fingerprint instead of deleting history), and the
fingerprint never changes silently — recompute is explicit, with a
dry-run preview first.

## Correcting the fingerprint

The fingerprint dashboard (TUI `Fingerprint` screen, GUI `Fingerprint`
tab) lists every feature's mean/SD. Select one, type a corrected mean,
apply: the value is stored as a *user correction* overlaid on the
computed reference. Corrections survive recompute, are marked wherever
they show, and can be cleared wholesale to restore computed values
exactly.

## Patterns adapted from same-kind tools

- **HumanifyMe** — the editable, human-readable fingerprint, and the
  deterministic post-draft verify gate (dropped numbers / broken URLs
  in `check_draft`).
- **brandvoice-mcp** — ingest → chunk → exemplar retrieval bank
  (`get_exemplars` serves corpus paragraphs by genre).
- **book-writer-mcp / Eden** — per-genre style profiles (the dashboard
  shows pooled per-genre stats), and the builder flow where induced
  rules are shown for correction *before* they are saved (recompute
  dry-run preview → apply).
- **Arcitext** — tone-fit via an explicit avoid list, enforced by the
  checker and editable in the Profile view.

## Bounds (named, enforced)

Sample text ≤ 256 KiB and ≥ 10 tokens; draft ≤ 1 MiB, < 200 words =
UNRELIABLE verdict (integrity/avoid checks still run); URL fetch ≤
1 MiB, 15 s, HTTP(S) only, ≤ 3 redirects; JSON-RPC line ≤ 4 MiB;
fingerprint build needs ≥ 2 included pieces (leave-one-out calibration
is undefined below that) and fails closed otherwise.

## Platform notes

- Toolchain pinned to Rust 1.99.0 (`rust-toolchain.toml`), matching the
  estate. GPUI is pinned exactly: `gpui-ce = =0.2.2` +
  `gpui_ce_platform = =0.1.0` (features wayland, x11).
- GUI rendering is verified on Wayland (primo's desktop is Hyprland;
  the smoke runs under a headless wlroots compositor). Under Xvfb/X11
  this GPUI build presents black frames — use a Wayland session.
- GUI text entry is click-to-focus, append/backspace only (no
  selection/IME yet) — a stated limit, tracked in PROGRESS.md.

## Layout

```
crates/voice-core   corpus store, stylometry, fingerprint build, checker, fetch
crates/voice-mcp    stdio MCP server (6 tools)
crates/voice-tui    ratatui TUI (voice-tui --snapshot <screen> renders headlessly)
crates/voice-gui    GPUI GUI
screenshots/        smoke evidence (GUI corpus + fingerprint views, probe)
```
