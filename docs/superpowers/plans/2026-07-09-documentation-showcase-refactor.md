# Documentation Showcase Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor ytmtui documentation into an English-first, PT-BR-complete showcase and guide set that is more visual, detailed, accurate, and easier to navigate.

**Architecture:** This is a documentation-only refactor. The README files become high-impact showcases and docs hubs; long-form guidance moves into focused Markdown files under `docs/`; architecture and troubleshooting docs are refreshed to match the current code.

**Tech Stack:** GitHub Markdown/HTML, shields.io badges, readme-typing-svg, existing PNG screenshots in `docs/screenshots/`, shell verification with `rg` and `git diff --check`.

## Global Constraints

- English is the primary language.
- PT-BR counterparts must be complete, not partial summaries.
- No Rust source changes.
- No workflow or release automation changes.
- No generated static documentation site.
- No fake screenshots, fake metrics, or unverified claims.
- No private/local-only assets.
- No dependency on JavaScript-rendered docs.
- Use existing screenshots from `docs/screenshots/`.
- Documentation-only changes do not require the Rust test suite unless code examples or source files change.

---

### Task 1: Create Focused User Guides

**Files:**
- Create: `docs/GETTING_STARTED.md`
- Create: `docs/GETTING_STARTED.pt-BR.md`
- Create: `docs/FEATURES.md`
- Create: `docs/FEATURES.pt-BR.md`
- Create: `docs/AUTHENTICATION.md`
- Create: `docs/AUTHENTICATION.pt-BR.md`
- Create: `docs/KEYMAP.md`
- Create: `docs/KEYMAP.pt-BR.md`

**Interfaces:**
- Consumes: verified facts from `README.md`, `README.pt-BR.md`, `CHANGELOG.md`, and source search.
- Produces: focused guides linked by the READMEs.

- [ ] **Step 1: Create getting started guides**

Write install, requirements, first run, first search/play, optional sign-in, and troubleshooting pointers in English and PT-BR.

- [ ] **Step 2: Create feature showcase guides**

Write grouped feature documentation covering search with albums, playback, Home/recent, lyrics, visualizer/art, queue/radio, themes, account/library, and cache/prefetch in English and PT-BR.

- [ ] **Step 3: Create authentication guides**

Write anonymous mode, `g` sign-in, cookie path precedence, session expiry, anti-bot workaround, and privacy notes in English and PT-BR.

- [ ] **Step 4: Create keymap guides**

Write shortcut tables for navigation, search, playback, queue/account, appearance, and general actions in English and PT-BR.

### Task 2: Rewrite README Showcase Pair

**Files:**
- Modify: `README.md`
- Modify: `README.pt-BR.md`

**Interfaces:**
- Consumes: guide files from Task 1.
- Produces: showcase README pair with quick install, visual screenshots, docs hub, concise feature cards, contributor links, legal/license.

- [ ] **Step 1: Replace `README.md` with showcase structure**

Use a centered hero, badges, animation, screenshot grid, "Why ytmtui?", "What it feels like", quick install, docs hub, concise feature summary, contributor section, legal/license.

- [ ] **Step 2: Replace `README.pt-BR.md` with complete PT-BR counterpart**

Mirror the English structure naturally in Portuguese and link the PT-BR guide files.

### Task 3: Refresh Deep Docs

**Files:**
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/TROUBLESHOOTING.md`
- Modify: `docs/TROUBLESHOOTING.pt-BR.md`

**Interfaces:**
- Consumes: current source facts and new guide links.
- Produces: architecture and troubleshooting docs aligned with current behavior.

- [ ] **Step 1: Refresh architecture**

Correct search to four sub-searches, document in-app sign-in through `app/authentication.rs`, document recent history, clean mixed English/PT-BR prose, and keep technical flow diagrams readable.

- [ ] **Step 2: Refresh troubleshooting English**

Keep existing coverage, improve scanability, add in-app sign-in guidance where appropriate, and link to authentication/getting started guides.

- [ ] **Step 3: Refresh troubleshooting PT-BR**

Mirror the English troubleshooting structure in natural PT-BR.

### Task 4: Verify Documentation Refactor

**Files:**
- Verify all modified and new Markdown files.

**Interfaces:**
- Consumes: completed docs.
- Produces: verified documentation-only diff.

- [ ] **Step 1: Run whitespace verification**

Run: `git -C /home/itthiago/Projetos/ytmtui diff --check`

Expected: no output and exit 0.

- [ ] **Step 2: Search stale claims**

Run: `rg -n "three sub-searches|três sub-buscas|songs, artists, and playlists|músicas, artistas e playlists|restart ytmtui|reinicie o ytmtui" /home/itthiago/Projetos/ytmtui/README.md /home/itthiago/Projetos/ytmtui/README.pt-BR.md /home/itthiago/Projetos/ytmtui/docs/GETTING_STARTED.md /home/itthiago/Projetos/ytmtui/docs/GETTING_STARTED.pt-BR.md /home/itthiago/Projetos/ytmtui/docs/FEATURES.md /home/itthiago/Projetos/ytmtui/docs/FEATURES.pt-BR.md /home/itthiago/Projetos/ytmtui/docs/AUTHENTICATION.md /home/itthiago/Projetos/ytmtui/docs/AUTHENTICATION.pt-BR.md /home/itthiago/Projetos/ytmtui/docs/KEYMAP.md /home/itthiago/Projetos/ytmtui/docs/KEYMAP.pt-BR.md /home/itthiago/Projetos/ytmtui/docs/TROUBLESHOOTING.md /home/itthiago/Projetos/ytmtui/docs/TROUBLESHOOTING.pt-BR.md /home/itthiago/Projetos/ytmtui/docs/ARCHITECTURE.md`

Expected: no stale claim that excludes albums or incorrectly implies restart is the only sign-in path. Any remaining matches must be intentional historical/context text.

- [ ] **Step 3: Verify documentation links point to files**

Run: `for f in README.md README.pt-BR.md docs/GETTING_STARTED.md docs/GETTING_STARTED.pt-BR.md docs/FEATURES.md docs/FEATURES.pt-BR.md docs/AUTHENTICATION.md docs/AUTHENTICATION.pt-BR.md docs/KEYMAP.md docs/KEYMAP.pt-BR.md docs/TROUBLESHOOTING.md docs/TROUBLESHOOTING.pt-BR.md docs/ARCHITECTURE.md; do test -f "/home/itthiago/Projetos/ytmtui/$f" || exit 1; done`

Expected: exit 0.

- [ ] **Step 4: Review final diff**

Run: `git -C /home/itthiago/Projetos/ytmtui diff --stat`

Expected: Markdown-only changes.

- [ ] **Step 5: Skip Rust tests**

Expected: no Rust source, examples, Cargo files, or workflows changed.
