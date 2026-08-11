# SbtDeskTool changelog

## v1.26.8.11 — Folder Diff Tree View and Persistent UI State

- Added persistent Folder Diff left and right folder selections.
- Persisted the selected Folder Compare mode across app restarts.
- Persisted Folder Diff filter-group and List/Tree view-group choices across app restarts.
- Added expandable and collapsible folder rows in Tree view with per-level indentation guides.

## v1.26.8.7 — Folder Comparison and Developer Tooling

- Added folder comparison engine with recursive file difference computation and configurable result filtering (all / changes / different / left-only / right-only).
- Added Folder Diff panel UI with left/right folder path selection, swap, clear, and file list showing status and sizes.
- Added `--folder-diff-left` and `--folder-diff-right` CLI arguments for quick folder diff launches.
- Added persistent folder diff configuration stored per folder pair.
- Added two-panel preview for image, PDF, audio, and video files via the asset protocol, with Text Diff / Preview toggle.
- Added text/binary-aware file preview with 4 MB head truncation, hexdump for binary content, and multi-encoding fallback (UTF-8, BOM, Windows-1252, Shift-JIS, EUC-JP, GBK, Big5).
- Files larger than 4 MB open in a two-panel preview showing per-side file sizes; switching to Text Diff loads the truncated text/binary content.

## v1.26.8.5 — Translation Provider Reliability and Agent CLI

- Fixed provider connection tests to use the current unsaved enablement, endpoint, model, and API key values.
- Preserved stored API keys when Save or Test is used with a blank API key field.
- Added reusable Agent CLI profiles for Codex, Claude Code, Gemini CLI, Kiro, OpenCode, GitHub Copilot CLI, Qwen, and custom non-interactive AI commands.
- Added support for creating multiple Agent CLI providers with configurable executables, arguments, prompt or stdin input, stdout output, batching, fallback, retries, and timeout handling.
- Prevented Agent CLI processes from opening visible console windows on Windows.
- Added removable built-in AI providers and persisted deleted providers across app restarts, while keeping Google Translate as the protected system provider.
- Added multiple custom OpenAI-compatible providers with configurable names, models, endpoints, optional API keys, fallback support, and secure per-provider credential storage.
- Moved Add Custom Provider and Add Agent CLI actions to the bottom of the provider sidebar.
- Fixed Translate status to report the actual source, such as Google Translate, Dictionary, Translation Memory, or cache, instead of labeling every fresh translation as AI.

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
