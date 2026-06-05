# Tauri Rebuild Requirements

This document is the only retained Tauri-specific design state after resetting the previous mock.
The next Tauri implementation should be built from scratch against these requirements, not by
copying the deleted implementation.

## Goal

Build a resident macOS project launcher that feels reliable under fast keyboard use:

- Press the global shortcut.
- Type immediately.
- Move selection if needed.
- Press Enter.
- Open the selected project in Zed.

No key should be lost, repeated, or applied to the wrong selection.

## Required Behavior

- Global shortcut: `Ctrl+M`.
- Pressing `Ctrl+M` while the palette is visible hides it.
- The app must be single-instance. A second launch must not leave another hotkey owner running.
- Palette show should use a pre-created hidden window where practical.
- The show path must avoid disk I/O.
- `Esc` hides the palette.
- Losing focus or clicking outside the palette hides it.
- `Enter` dispatches the selected project to `zed` and then hides the palette.
- `Cmd+,` opens settings.

## Keyboard Input

- The search field should be display-focused, not normal IME text input.
- Query updates should come from physical key handling, so Japanese IME state does not show candidates or duplicate letters.
- Required keys:
  - ASCII letters `a-z`
  - digits `0-9`
  - space
  - `-`
  - Backspace/Delete
  - `Ctrl+N` / ArrowDown for next
  - `Ctrl+P` / ArrowUp for previous
  - Enter
  - Escape
- Search updates and selection movement must be synchronous from the user perspective. If rendering lags, the buffered query and selected index must remain correct.

## Project Data

- Source of truth: `~/.project-manager.json`.
- Required project fields:
  - `id`
  - `name`
  - `path`
  - `openPaths`
  - `aliases`
  - `tags`
  - `language`
  - `lastOpenedAt`
- Load project data on startup and keep a memory index.
- Settings saves should update the in-memory index without requiring app restart.
- Existing paths should be validated before adding through settings.

## Search

- Normalize query to lowercase.
- Split whitespace into tokens.
- Exact alias hit must return a single result and be the fastest path.
- Other matching should cover:
  - project id
  - name prefix
  - name substring
  - tag
  - path
  - fuzzy subsequence fallback
- Return at most 50 visible results.
- After each search, select the first visible result.
- Selection movement must operate on the currently visible results.
- Enter must dispatch the currently selected visible result, not a stale index.

## Debug Actions

- Add one synthetic debug card:
  - name: `Switch to AppKitBench`
  - alias: `-`
  - action: quit TauriBench and launch `/Applications/AppKitBench.app`
- The debug card should not be saved into `~/.project-manager.json`.

## Settings

- Provide a simple settings window for project management:
  - list projects
  - add project
  - remove project
  - edit name/path/openPaths/aliases/tags/language
  - browse for an existing folder or workspace file
  - save/cancel
- Settings are functional, not decorative.

## Metrics

- Log JSONL to `~/Library/Logs/ProjectLauncherBench/`.
- Required events:
  - `app_ready`
  - `hotkey_received`
  - `palette_rendered`
  - `search_completed`
  - `selection_moved`
  - `open_requested`
  - `open_dispatched`
  - `debug_switch_requested`
  - `debug_switch_dispatched`
- Required metrics:
  - hotkey to rendered
  - search duration
  - input to result
  - selection move duration
  - open dispatch duration

## Verification

- Add browser-level keyboard tests with mocked Tauri IPC before shipping.
- Required test scenarios:
  - `kan -> Ctrl+N -> Ctrl+N -> Enter`
  - `kan -> ArrowDown -> Ctrl+P -> Enter`
  - `- -> Enter`
  - `Esc`
  - `Ctrl+M` toggle
- Also verify the installed macOS app with real or synthetic key events and inspect JSONL logs.

## Non-Goals For The Rebuild

- Do not port code from the deleted implementation.
- Do not add animations that can make rows invisible or selected state ambiguous.
- Do not keep multiple Tauri processes alive.
- Do not add broad plugin systems or Raycast-like extensions yet.
- Do not optimize cold start before the warm shortcut path is correct.
