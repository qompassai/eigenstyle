# Roadmap — repo (mcp) — v0.1 → v1.0
<!-- bloch:roadmap schema=1 kind=mcp lane=general baseline=0.1.0 target=1.0.0 -->

How to work this roadmap (any agent): run `bloch task start R-xxx --agent <you>` before working a task, `bloch task done R-xxx` when you believe it is complete — done runs every `verify:` command and flips the task green only if all pass; there is no override. `bloch task fail R-xxx --error "..."` records a failure. Fixes learned the hard way land in `.ai/memory/lessons.md` via the inbox/promote flow and are surfaced in every HANDOFF.

## Phase 1 — Baseline truth
- [x] R-101 Build is green from a clean checkout
  - verify: cargo build
  - evidence: 1 verify commands, all exit 0 at 2026-10-29T04:46:40Z
- [x] R-102 Existing tests pass at the baseline
  - verify: cargo test
  - evidence: 1 verify commands, all exit 0 at 2026-10-29T04:46:42Z
- [x] R-103 Repo identity, baseline version, and this roadmap are recorded
  - verify: test -f .ai/roadmap.md
  - evidence: 1 verify commands, all exit 0 at 2026-10-29T04:46:42Z

## Phase 2 — Core complete
- [ ] R-201 Tool surface is contracted: every tool has a complete schema and annotations from birth
  - verify: cargo test
  - evidence:
- [ ] R-202 Tool count stays within the declared budget (<= 25 tools)
  - verify: cargo test
  - evidence:
- [ ] R-203 Typed error and refusal behaviour is implemented in server code, never left to client hints
  - verify: cargo test
  - evidence:

## Phase 3 — Tests and gates
- [ ] R-301 Validation tests cover the happy path end to end (half the test split)
  - verify: cargo test
  - evidence:
- [ ] R-302 Adversarial tests cover malformed, oversized, and hostile inputs (half the test split)
  - verify: cargo test
  - evidence:
- [ ] R-303 Refusal and typed-error behaviour is covered by tests, including oversized payloads
  - verify: cargo test
  - evidence:

## Phase 4 — Hardening
- [ ] R-401 MCP Inspector conformance pass is recorded as evidence
  - verify: cargo test
  - evidence:
- [ ] R-402 Client matrix is verified with at least two real clients
  - verify: cargo test
  - evidence:
- [ ] R-403 Declared-vs-detected access (EFLO) check passes: no tool exceeds its annotations
  - verify: cargo clippy -- -D warnings
  - evidence:
- [ ] R-404 Registry/release packaging builds from a clean checkout
  - verify: cargo build
  - evidence:

## Phase 5 — Docs
- [ ] R-501 README documents install, usage, and verification
  - verify: test -f README.md
  - evidence:
- [ ] R-502 CHANGELOG records every user-facing change
  - verify: test -f CHANGELOG.md
  - evidence:
- [ ] R-503 Tool reference documents every tool, schema, annotation, and error kind
  - verify: test -f README.md
  - evidence:

## Phase 6 — Release v1
- [ ] R-601 Version is bumped to 1.0.0 and the changelog records the release
  - verify: grep -q 'version = "1.0.0"' Cargo.toml
  - evidence:
- [ ] R-602 Clean-environment install and run is verified
  - verify: cargo build
  - evidence:
- [ ] R-603 Release artefacts are built from a clean checkout and checksummed
  - verify: cargo build
  - evidence:

