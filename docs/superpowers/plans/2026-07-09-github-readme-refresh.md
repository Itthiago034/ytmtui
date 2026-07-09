# GitHub README Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refresh the GitHub profile and `ytmtui` READMEs with an English-first, PT-BR-aware, elegant, lightly animated presentation that stays factually accurate.

**Architecture:** This is a documentation-only change. The project README edits stay inside `Itthiago034/ytmtui`; the profile README edits happen in a separate local checkout of `Itthiago034/Itthiago034`. No Rust source, workflows, or assets are changed.

**Tech Stack:** GitHub Markdown/HTML, shields.io badges, `readme-typing-svg`, existing PNG screenshots from `docs/screenshots/`.

## Global Constraints

- English is the primary language.
- PT-BR remains clearly available through `README.pt-BR.md` and one concise profile sentence.
- Do not add unverified features.
- Do not claim album search unless the project documentation/source supports it.
- Do not claim one-key login; describe optional cookie-based sign-in instead.
- Do not add JavaScript or private/local-only assets.
- Use existing screenshots from `docs/screenshots/`.
- README-only edits do not require the Rust test suite.

---

### Task 1: Prepare Profile Checkout

**Files:**
- Create or update local checkout: `/home/itthiago/Projetos/Itthiago034/README.md`
- Read-only reference: `/home/itthiago/Projetos/ytmtui/README.md`
- Read-only reference: `/home/itthiago/Projetos/ytmtui/README.pt-BR.md`
- Read-only reference: `/home/itthiago/Projetos/ytmtui/docs/ARCHITECTURE.md`

**Interfaces:**
- Consumes: public GitHub repository `https://github.com/Itthiago034/Itthiago034.git`
- Produces: editable local profile README at `/home/itthiago/Projetos/Itthiago034/README.md`

- [ ] **Step 1: Check whether the profile repo is already present**

Run: `test -d /home/itthiago/Projetos/Itthiago034/.git && git -C /home/itthiago/Projetos/Itthiago034 remote -v || true`

Expected: either a remote for `Itthiago034/Itthiago034` or no output.

- [ ] **Step 2: Clone the profile repo if missing**

Run: `git clone https://github.com/Itthiago034/Itthiago034.git /home/itthiago/Projetos/Itthiago034`

Expected: local checkout created. If the directory already exists, skip this command.

- [ ] **Step 3: Confirm clean state**

Run: `git -C /home/itthiago/Projetos/Itthiago034 status --short`

Expected: no output before editing.

### Task 2: Refresh Profile README

**Files:**
- Modify: `/home/itthiago/Projetos/Itthiago034/README.md`

**Interfaces:**
- Consumes: accurate `ytmtui` feature claims from project docs.
- Produces: English-first GitHub profile README with subtle animation, one PT-BR line, accurate featured project copy, screenshot, and compact tech badges.

- [ ] **Step 1: Replace the current profile README**

Use this content:

```markdown
<h1 align="center">Hi, I'm Thiago</h1>

<p align="center">
  <a href="https://git.io/typing-svg">
    <img src="https://readme-typing-svg.demolab.com?font=Fira+Code&weight=500&size=18&duration=2600&pause=900&color=FF2D46&center=true&vCenter=true&width=720&lines=Rust+%2B+terminal+tools;Embedded+systems+and+low-level+programming;Linux-first+workflows+with+a+practical+edge" alt="Typing SVG" />
  </a>
</p>

<p align="center">
  Developer exploring <strong>embedded systems</strong>, <strong>low-level programming</strong>,
  <strong>Linux</strong>, and polished tools for the terminal.
</p>

<p align="center">
  <em>Também documento e mantenho projetos em PT-BR quando isso ajuda a comunidade.</em>
</p>

---

## Featured Project

### [ytmtui](https://github.com/Itthiago034/ytmtui)

<p>
  <a href="https://github.com/Itthiago034/ytmtui/releases">
    <img src="https://img.shields.io/github/v/release/Itthiago034/ytmtui?include_prereleases&sort=semver&label=release&color=ff2d46" alt="ytmtui release" />
  </a>
  <a href="https://github.com/Itthiago034/ytmtui/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/Itthiago034/ytmtui/ci.yml?label=CI" alt="ytmtui CI" />
  </a>
  <img src="https://img.shields.io/badge/Rust-Ratatui-f97316?logo=rust&logoColor=white" alt="Rust + Ratatui" />
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license" />
</p>

`ytmtui` is a YouTube Music terminal client written in Rust with Ratatui. It focuses on fast keyboard navigation, clean terminal UI, and playback that works from a developer's shell.

- Search songs, artists, and playlists without requiring sign-in.
- Stream playback through `yt-dlp`, `ffmpeg`, and `rodio`.
- Use optional cookie-based sign-in for account name, private playlists, library sync, and likes.
- Follow synced lyrics, real-time spectrum visualization, album art, themes, queue, radio, and autoplay.
- Read it in [English](https://github.com/Itthiago034/ytmtui) or [PT-BR](https://github.com/Itthiago034/ytmtui/blob/master/README.pt-BR.md).

<p align="center">
  <a href="https://github.com/Itthiago034/ytmtui">
    <img src="https://raw.githubusercontent.com/Itthiago034/ytmtui/master/docs/screenshots/home.png" alt="ytmtui home screen" width="760" />
  </a>
</p>

---

## Toolbox

<p>
  <img src="https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/Linux-FCC624?logo=linux&logoColor=black" alt="Linux" />
  <img src="https://img.shields.io/badge/Terminal-111827?logo=gnometerminal&logoColor=white" alt="Terminal" />
  <img src="https://img.shields.io/badge/Embedded-0f766e" alt="Embedded systems" />
  <img src="https://img.shields.io/badge/Python-3776AB?logo=python&logoColor=white" alt="Python" />
</p>

---

<p align="center">
  <em>Older experiments may be archived; active work is kept visible first.</em>
</p>
```

- [ ] **Step 2: Verify profile claims**

Run: `rg -n "albums|one-key|login with one|Login with one|Search songs, artists, and playlists|cookie-based" /home/itthiago/Projetos/Itthiago034/README.md`

Expected: no misleading `albums` or `one-key` claims; accurate search and cookie wording present.

- [ ] **Step 3: Review profile diff**

Run: `git -C /home/itthiago/Projetos/Itthiago034 diff -- README.md`

Expected: only profile README copy/layout changes.

### Task 3: Refresh `ytmtui` English README Header

**Files:**
- Modify: `/home/itthiago/Projetos/ytmtui/README.md`

**Interfaces:**
- Consumes: existing complete English README content after the opening section.
- Produces: more polished first viewport while keeping install, features, requirements, architecture, and troubleshooting content intact.

- [ ] **Step 1: Replace only the opening block before `## Table of Contents`**

Use a centered title, badges, language switch, typing SVG, accurate tagline, and preserve the existing terminal preview. The replacement starts at the top of `README.md` and ends immediately before `## Table of Contents`.

- [ ] **Step 2: Keep accurate project positioning**

The opening copy must say:

```markdown
**ytmtui** is a terminal client (TUI) for **YouTube Music**, written in **Rust** with **[Ratatui](https://ratatui.rs)**. It brings search, playback, queue management, synced lyrics, themes, album art, and a real-time audio visualizer into a keyboard-first terminal interface.
```

- [ ] **Step 3: Verify no misleading claims**

Run: `rg -n "album search|one-key|one key|albums in one list|login with" /home/itthiago/Projetos/ytmtui/README.md`

Expected: no output.

### Task 4: Refresh `ytmtui` PT-BR README Header

**Files:**
- Modify: `/home/itthiago/Projetos/ytmtui/README.pt-BR.md`

**Interfaces:**
- Consumes: English header structure from Task 3.
- Produces: Portuguese counterpart with the same factual positioning and a clear link back to English.

- [ ] **Step 1: Replace only the opening block before `## Sumário`**

Use the same structure as the English README, translated naturally to PT-BR.

- [ ] **Step 2: Keep accurate project positioning**

The opening copy must say:

```markdown
**ytmtui** é um cliente de terminal (TUI) para o **YouTube Music**, escrito em **Rust** com **[Ratatui](https://ratatui.rs)**. Ele leva busca, reprodução, gerenciamento de fila, letras sincronizadas, temas, capa do álbum e um visualizador de áudio em tempo real para uma interface de terminal focada no teclado.
```

- [ ] **Step 3: Verify no misleading claims**

Run: `rg -n "busca de álbuns|um clique|uma tecla|login com uma tecla|álbuns em uma lista" /home/itthiago/Projetos/ytmtui/README.pt-BR.md`

Expected: no output.

### Task 5: Final Documentation Verification

**Files:**
- Verify: `/home/itthiago/Projetos/Itthiago034/README.md`
- Verify: `/home/itthiago/Projetos/ytmtui/README.md`
- Verify: `/home/itthiago/Projetos/ytmtui/README.pt-BR.md`

**Interfaces:**
- Consumes: edited README files.
- Produces: verified documentation-only changes ready for user review.

- [ ] **Step 1: Check repository statuses**

Run: `git -C /home/itthiago/Projetos/ytmtui status --short`

Expected: modified README files and this plan only, unless already committed.

Run: `git -C /home/itthiago/Projetos/Itthiago034 status --short`

Expected: modified `README.md` only.

- [ ] **Step 2: Check Markdown references**

Run: `rg -n "README.pt-BR.md|docs/screenshots/home.png|readme-typing-svg|Itthiago034/ytmtui" /home/itthiago/Projetos/ytmtui/README.md /home/itthiago/Projetos/ytmtui/README.pt-BR.md /home/itthiago/Projetos/Itthiago034/README.md`

Expected: language links, screenshot references, typing SVG, and repository links present.

- [ ] **Step 3: Review diffs**

Run: `git -C /home/itthiago/Projetos/ytmtui diff -- README.md README.pt-BR.md docs/superpowers/plans/2026-07-09-github-readme-refresh.md`

Expected: documentation-only changes.

Run: `git -C /home/itthiago/Projetos/Itthiago034 diff -- README.md`

Expected: profile README-only changes.

- [ ] **Step 4: Do not run Rust tests**

Expected: no Rust source or examples changed, so `cargo test` is unnecessary for this task.
