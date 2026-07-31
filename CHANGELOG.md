# SbtDeskTool changelog

## v1.26.7.31 — Updater Reliability Fixes

- Fixed automatic updates by publishing permanent tagged GitHub download URLs instead of temporary draft-release URLs.
- Stopped retrying alternate network strategies after a server responds with an HTTP error.
- Fixed Notes so sequential saves replace edited content instead of incorrectly merging older lines.
- Added shared syntax auto-detection to Diff, Translate Source/Translated, and Notes editors, with Markdown fallback for regular text and line numbers in Source.

## v1.26.7.30 — Multi-window Notes Sync and Notepad++ Shortcuts

- Fixed data loss when editing Notes across multiple windows by automatically merging latest content without overwriting changes.
- Fixed Compact mode Always-on-Top feature so it takes effect immediately upon startup.
- Updated Notepad++ keyboard shortcut presets for the text editor (full screen, code folding, line duplication, column selection, tab switching, etc.).
- Added icons to Translate tab toolbar controls (Providers, Excel, Dictionary, Memory) and enabled responsive collapse to icon buttons when narrowing window width.

## v1.26.7.27 — Translation Platform and Enterprise Administration

- Fixed automatic updates on managed networks by publishing public GitHub Release download URLs instead of GitHub Assets API URLs.
- Added configurable Google, Gemini, OpenAI, Claude, DeepL and Local AI providers with secure credential storage, connection tests, fallback ordering, batching, concurrency, retry and timeout policies.
- Added Excel workbook translation with sheet, column and range exclusions, duplicate-text reuse, resumable checkpoints, per-cell error reporting, detailed logs and safe output files.
- Added Dictionary and Translation Memory management with in-sentence terminology protection, CSV import/export, review states, cache TTL controls and optional automatic TM saving.
- Added enterprise push/pull synchronization through Cloudflare Workers and D1 with per-device credentials, workspace isolation, cursor handling, retries, tombstones and optimistic conflict detection.
- Added the Cloudflare Access-protected Web Admin for members, roles, Dictionary/TM review, device credentials, conflicts and audit history.
- Standardized local and release artifacts under `target`, added versioned Windows portable/installer downloads, and fixed universal macOS packaging for the credential helpers.
- Windows Authenticode remains disabled until a trusted code-signing certificate is purchased; updater artifacts continue to use Tauri signatures.

## v1.26.7.23 - Improvement and Bug Fixes

- Refreshed the app icon for clearer visibility across Windows.
- Improved controls and layouts for compact and narrow windows.
- Improved language selection and swapping in Translate Compact mode.
- Made the Notes list more compact and added drag-and-drop ordering.
- Added familiar keyboard shortcuts and simpler editor right-click menus.
- Made startup and window resizing smoother.
- Improved update reliability, including on managed company networks.
- Simplified downloads and improved packages for Windows, macOS and Linux.

## v1.26.7.21 — Initial release

- Compare and edit text side by side with two diff algorithms, word-level details, whitespace controls and optional copy/revert actions.
- Translate text automatically with language detection, language swap, network fallbacks and reuse of unchanged lines.
- Create local Markdown notes with filtering, preview, line numbers, auto-save and Compact-mode quick selection.
- Search individual text areas, or search both text areas together in Diff and Translate, with case, whole-word and regular-expression options.
- Keep independent wrap, zoom, cursor and status state for Diff, Translate and Notes.
- Customize the workspace with light/dark themes, Compact mode, always-on-top, window effects, tray controls and drag-and-drop.
- Receive signed in-app updates and native packages for Windows, macOS and Linux.
