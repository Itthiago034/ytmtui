# Safe Authentication and Doctor Command Design

**Date:** 2026-07-12

**Status:** Approved direction, pending written-spec review

**Target:** the next ytmtui release after `v0.2.0`

## Context

ytmtui already imports browser cookies without asking for a password, resolves
cookie paths deterministically, writes the default cookie file atomically, and
models anonymous, authenticated, invalid, and expired sessions explicitly.
Firefox is attempted before Chromium-family browsers.

The remaining reliability gap is that sign-in commits the first technically
valid cookie export before it knows which Google/YouTube account the export
represents. If Firefox extraction fails, the app can fall back to Brave without
showing the failed attempt clearly. In a browser with multiple Google accounts,
the client also always sends `X-Goog-AuthUser: 0`, so a valid cookie jar can
still resolve to a different account than the one the user intended.

Troubleshooting currently requires source inspection and ad hoc shell commands.
There is no safe, repeatable command that checks dependencies, browser
detection, cookie configuration, file permissions, API connectivity, and the
account recognized by YouTube Music.

## Goals

This work must:

1. preserve the browser order `Firefox -> Brave -> Chrome -> Chromium -> Edge
   -> Vivaldi -> Opera`;
2. use a later browser only after the earlier browser fails to export or
   validate a usable YouTube Music session;
3. identify the browser, profile, account name, and Google account index before
   replacing the active cookie file;
4. let the user choose among multiple accounts found in one cookie jar;
5. require confirmation before activating a different account;
6. preserve the previous authenticated session when preparation, validation,
   confirmation, or activation fails;
7. persist the selected browser/profile and account index for subsequent
   refreshes;
8. add a non-interactive `ytmtui doctor` command with actionable, sanitized
   diagnostics;
9. keep authentication provider-specific while the application and UI consume
   provider-neutral previews and results; and
10. add tests around the behavior before reorganizing the existing code.

## Non-goals

- Password, OAuth device-code, or embedded-webview login.
- Uploading diagnostics, cookies, logs, or account data anywhere.
- Automatically changing Google account state inside a browser.
- Removing Brave or other supported browsers as fallbacks.
- A general plugin diagnostics framework before a second production provider
  needs one.
- A full `app.rs` modularization; this slice only extracts authentication state
  and messages that it directly touches.
- Machine-readable doctor output in the first version.

## Chosen Approach

Authentication becomes a two-phase provider operation:

1. **Prepare:** discover browsers in priority order, export to a provider-owned
   temporary file, validate the cookies against YouTube Music, enumerate the
   available account identities, and return a provider-neutral preview.
2. **Activate:** after explicit user confirmation, atomically install the
   prepared credentials, switch the provider client, persist the selected
   account index and browser source, and remove temporary state.

Cancellation discards the prepared credentials and leaves the current provider
client and cookie file untouched.

This is preferred over committing first and offering an undo because the old
session remains valid throughout preparation. It is also preferred over making
Firefox mandatory because the approved product behavior keeps Brave and the
other browsers as useful fallbacks.

## Architecture

### Provider-neutral authentication contract

The existing one-shot `MusicProvider::sign_in` contract will be replaced by
three blocking operations, all run through `spawn_blocking`:

```text
prepare_sign_in(progress) -> SignInPreview
activate_sign_in(preview_id, account_index) -> SignInSummary
cancel_sign_in(preview_id)
```

`SignInPreview` contains display-safe data only:

```text
preview_id
method                 # firefox, brave, ...
profile_label          # concise display label, not cookie contents
accounts[]
  index                # X-Goog-AuthUser value
  name
  handle, when present
current_account_name   # optional comparison hint
```

The provider retains the temporary credential path in private pending state,
keyed by `preview_id`. Neither the UI nor the generic application state receives
cookie contents, authorization hashes, or an unrestricted temporary path.
Only one prepared sign-in may exist at a time. Starting another preparation
cancels and cleans up the previous one.

Providers that support one-step authentication may return a preview with one
account. The generic UI still confirms it through the same contract.

### YouTube Music sign-in service

`src/ytmusic/signin.rs` remains responsible for browser discovery and cookie
export. Its responsibilities are extended to:

- represent browser candidates as typed values rather than display strings;
- preserve the approved priority order, including explicit XDG Firefox profile
  paths;
- export into a restrictive temporary file without replacing
  `~/.config/ytmtui/cookies.txt`;
- construct a temporary authenticated client;
- query `account/account_menu` with bounded `X-Goog-AuthUser` indices;
- return only non-empty, distinct account identities;
- treat a technically valid cookie jar with no identifiable account as a failed
  browser attempt; and
- retain a concise reason for every failed browser candidate.

Account enumeration is bounded to indices `0..=9` and stops after two
consecutive empty or unauthorized account responses following the last valid
identity. A transport failure stops enumeration for that browser and is
reported as validation failure rather than interpreted as “no more accounts.”

If Firefox exports and validates at least one account, no Chromium browser is
tried automatically. Brave is attempted only when Firefox extraction or
validation fails. A user cancellation or rejection is not a technical failure
and therefore does not trigger an automatic fallback.

### Account-aware client

`YtMusicClient` will carry the selected account index alongside its parsed
authentication data. Authenticated requests will use that value for
`X-Goog-AuthUser` instead of the current hard-coded `0`.

Configuration gains a backward-compatible optional authentication preference:

```json
{
  "authentication": {
    "browser": "firefox",
    "profile": "default-release",
    "auth_user": 0
  }
}
```

Existing configurations without this object continue to use account index `0`
and normal browser detection. The profile value is a stable display/lookup
identifier where possible; absolute paths are accepted for XDG Firefox but are
never shown in ordinary status messages.

### Application and UI flow

Pressing `g` starts preparation and displays each attempt explicitly:

```text
Trying Firefox profile default-release...
Firefox failed: <sanitized reason>; trying Brave...
```

On success, `Msg::SignInPrepared` opens a focused modal. A single-account
preview asks:

```text
Connect as Thiago Santos using Firefox?
Enter confirm  Esc cancel
```

For multiple accounts, the modal presents a keyboard-navigable list. The user
selects one with arrows or `j`/`k`, confirms with `Enter`, or cancels with
`Esc`. Rendering the modal performs no I/O.

Activation runs in a background task. Until it succeeds, the old session
remains active. A successful result closes the modal, updates account-only data,
reloads Home and Library, and persists the browser/profile/account preference.
Failure closes the busy state, preserves the old session, and provides one
concrete recovery action.

Authentication-specific application state and messages move into
`src/app/authentication.rs`; unrelated queue, playback, and rendering behavior
remain unchanged.

## Atomic Activation and Cleanup

Prepared cookies use mode `0600`. Activation performs the following sequence:

1. validate that the pending preview still exists and matches the requested
   account index;
2. validate the selected account once more using the prepared cookies;
3. create a restrictive backup of an existing default cookie file;
4. atomically rename the prepared file into the default cookie path;
5. persist the browser, profile, account index, and credential path;
6. publish the already validated account-aware client; and
7. remove the backup after successful activation, or retain it only when needed
   for recovery from a partial filesystem failure.

If a step before the atomic rename fails, production credentials are untouched.
If persistence fails after the rename, the backup is restored before returning
an error and the provider continues using the old in-memory client. Publishing
the new client is the final non-cleanup step and cannot fail. Temporary files
are removed on cancellation, replacement, normal process shutdown, and
best-effort startup cleanup of stale ytmtui-owned files.

## `ytmtui doctor`

`main.rs` will dispatch `ytmtui doctor` before raw mode, the alternate screen,
audio initialization, or MPRIS registration. The command is read-only and does
not replace or refresh browser cookies.

The initial doctor report includes:

```text
Runtime
  [ok] yt-dlp <version>
  [ok] ffmpeg <version>
  [warn] deno not found (optional)

Authentication
  [ok] configured cookie file exists, mode 0600
  [ok] cookie file has required YouTube authentication fields
  [ok] YouTube Music recognizes account "Thiago Santos" (auth user 0)
  [ok] preferred browser: Firefox / default-release
  [ok] fallback browser detected: Brave / Default

Connectivity
  [ok] music.youtube.com reachable
  [ok] authenticated account endpoint returned successfully

Summary: 8 passed, 1 warning, 0 failed
```

Checks produce structured internal results with severity `ok`, `warning`, or
`failure`, a short title, a sanitized detail, and an optional recovery hint.
The renderer prints those results as plain terminal text. Exit status is `0`
when all required checks pass and `1` when any required check fails; optional
dependency warnings do not fail the command.

The command may query YouTube Music using the already configured cookie file to
identify the active account. It does not export cookies from every browser or
change the active account. If there is no configured session, browser detection
is still reported and the authenticated check becomes an actionable warning.

Doctor output must never include:

- cookie values or complete cookie headers;
- SAPISID hashes or authorization headers;
- full contents of config or cookie files;
- raw API responses;
- unrestricted temporary paths; or
- command stderr that may contain sensitive arguments without sanitization.

## Error Semantics

Errors are typed at the sign-in/diagnostic boundary and formatted only at the
UI or CLI edge. Required categories are:

- browser/profile not found;
- cookie database unreadable or locked;
- browser export process failed;
- no YouTube session in the exported jar;
- no identifiable account;
- network unavailable or timed out;
- session rejected or expired;
- pending preview expired or cancelled;
- atomic credential installation failed; and
- configuration persistence failed.

Fallback occurs only for failures through account validation. Activation and
configuration failures do not try another browser because the user already
confirmed a specific identity.

## Testing Strategy

Implementation follows test-driven development. New tests cover:

### Unit tests

- Firefox remains first and Chromium browsers preserve their fallback order.
- A successful Firefox preparation does not invoke Brave.
- A failed Firefox preparation records its sanitized reason before Brave runs.
- Cancelling or rejecting a valid Firefox preview does not invoke Brave.
- Preparation never changes the production cookie file.
- Activation replaces credentials atomically and rollback preserves the old
  file on failure.
- Multiple distinct account indices are discovered and selectable.
- The selected account index is used in authenticated request headers.
- Legacy config deserializes with account index `0`.
- Doctor severities produce the documented exit status.
- Diagnostic formatting redacts representative cookie, SAPISID, authorization,
  home-path, and raw-stderr fixtures.

External processes and network calls are injected behind narrow test seams so
tests exercise ordering, state transitions, and file behavior without reading
the developer's browsers or contacting YouTube.

### Provider-boundary integration tests

- prepare -> confirm -> activate updates generic authentication state;
- prepare -> cancel preserves the existing authenticated provider;
- an activation error leaves account-only data and the old session intact; and
- a provider with no sign-in capability never exposes the modal.

### Manual Linux validation

- Firefox and Brave signed into different accounts: Firefox is presented first
  and Brave is not touched after Firefox succeeds.
- Firefox extraction failure: the reason is shown and Brave is tried.
- Firefox with multiple accounts: each account is listed and the selected one
  persists after restart.
- Cancellation: current account and cookie file remain unchanged.
- Expired current session: a newly confirmed session restores Home and Library.
- `ytmtui doctor` in healthy, offline, missing-dependency, invalid-cookie, and
  anonymous environments.

Full gates before completion:

```text
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo build --release
bash scripts/test-refresh-cookies.sh
```

## Documentation

The authentication, getting-started, troubleshooting, architecture, keymap, and
changelog documents will explain:

- browser priority and the exact fallback rule;
- account preview and multi-account selection;
- what authentication preferences are stored;
- how cancellation and rollback protect the existing session;
- how to run and interpret `ytmtui doctor`; and
- which diagnostic details are safe to paste into an issue.

English and Portuguese user-facing documentation remain synchronized.

## Delivery Boundary

This slice is complete when account-aware two-phase sign-in and the doctor
command pass all automated gates and the Firefox/Brave manual scenarios. The
larger roadmap items—general `app.rs` modularization, progressive playback,
InnerTube contract monitoring, session restore, wider packaging, and complete
runtime localization—remain separate follow-up designs so this reliability
work can ship without a broad rewrite.
