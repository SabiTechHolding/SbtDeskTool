# Translation platform plan

## Objective

Provide one translation policy for the Translate tab and Excel-file workflow:

```text
Dictionary -> Translation Memory -> provider -> fallback
```

The desktop client is local-first. It must work offline when a suitable local
translation exists, and must never overwrite an input Excel file.

## Delivery phases

1. **Foundation (implemented)**
   - Preserve the current Google Translate behaviour as the default provider.
   - Store exact-match Translation Memory locally in SQLite.
   - Use the same manager for text now and Excel later.
   - Include sync-ready record fields: UUID-like id, workspace id, version,
     timestamps and soft-delete support.
2. **Provider registry and settings (implemented; live credentials still required for provider smoke tests)**
   - Register Google Translate, Gemini, OpenAI, Claude, DeepL and Local AI.
   - Add a Translation Providers settings screen for enablement, model,
     endpoint, test connection and fallback order.
   - Store provider secrets in the OS credential store; configuration contains
     no API keys.
3. **Excel workflow (implemented)**
   - Translate `.xlsx` to a new output file with progress, cancellation and a
     per-cell error report.
   - Skip formulas, numbers, identifiers and configured sheets/columns by
     default. Excluded ranges accept workbook notation such as `A1:D20`,
     `Sheet2!F:F` and `'Sales Data'!2:5`.
   - Deduplicate source strings, use provider-specific concurrency, continue
     after cell failures, write a detailed log and open the output location.
   - Google batches up to 20 unique texts per Excel job, further bounded by
     request character size. Marker validation falls back to individual
     translation when a batch cannot be separated safely.
4. **Dictionary and TM management (implemented)**
   - Implemented: Dictionary CRUD, TM search/delete, CSV import/export,
     provider provenance, soft deletes and a local sync outbox.
   - Dictionary terms inside longer sentences are protected during provider
     translation. TM suggestions support approve/reject review in the desktop
     UI, and review status is included in CSV and synchronization payloads.
   - The server schema and Web Admin include review decisions; explicit
     personal/shared scopes in the desktop client remain future work.
5. **Enterprise synchronization (implemented and production verified)**
   - Desktop outbox/cursor sync to a company API over HTTPS.
   - Cloudflare D1 behind the Worker API; the desktop never accesses the
     database directly. Hyperdrive/PostgreSQL remains a future option if the
     enterprise deployment outgrows the D1 tenancy model.
   - Server-side optimistic conflict detection uses each outbox item's base
     version. Stale writes become review conflicts and return the canonical
     record instead of silently overwriting it.
   - Every desktop uses a revocable token scoped to one workspace, device and
     `reader`/`editor` role. D1 stores only a SHA-256 token hash.
   - Web administration for workspaces, members, roles, Dictionary/TM review,
     device credentials, conflicts and audit history is implemented and
     deployed as a Cloudflare Worker protected by Cloudflare Access.

## Current verification

- Rust formatting and Clippy with warnings denied.
- Rust unit tests: 32 passed; four network/credential tests remain opt-in.
- The live Google Translate smoke test passed on 2026-07-24.
- A live two-item Google batch smoke test passed on 2026-07-24.
- Provider credentials persist through the native Windows Credential Manager
  backend. The previously supplied Gemini key is considered exposed and must
  be replaced before the next live Gemini test.
- The production Enterprise Sync migration and Worker deployment completed on
  2026-07-24. A per-device credential for `nambv-pc` passed the live sync smoke
  test after the disabled legacy shared secret was rotated.
- Svelte/Vite production build completed successfully.
- Windows release build and NSIS packaging completed successfully on
  2026-07-24. The packaged application is explicitly pinned to the
  `sbt-desk-tool` binary rather than the credential helper binary.
- Enterprise sync contract tests cover HTTPS validation, camel-case API
  response parsing, device/workspace isolation, reader/editor RBAC and stale
  write conflict handling.
- Live provider tests require real credentials/endpoints and are available from
  the Provider Settings screen.

## Remaining work requiring external inputs or a release window

- Live-test OpenAI, Claude and DeepL after their API keys are supplied.
- Run fidelity tests with representative business workbooks containing styles,
  merged cells, charts, images and pivots; no such workbook is stored in this
  repository.
- Authenticode-sign the Windows executable and installer before distributing
  them outside an internal test group.
- `translate.toml` is intentionally not added. `settings.json` remains the
  single non-secret desktop configuration format, while provider and sync
  secrets remain in the operating-system credential store.
- The Web Admin device-credential UI is deployed at
  `https://sbt-desk-translation-admin.thangngo-it195.workers.dev` and remains
  protected by Cloudflare Access.
- The `src-admin` module includes the responsive operations dashboard, review
  queue, sync health, audit views and D1-backed admin actions.
- Add remote integration tests to CI once protected Cloudflare test credentials
  are available.

## Data ownership and conflict rules

- Dictionary records are approved business terminology. Only authorized users
  can alter shared records.
- AI-generated TM is initially a suggestion; it must not silently become a
  shared Dictionary entry.
- Dictionary conflicts require review. TM may hold several candidates, with
  approved entries ranked above suggestions.
- API keys are device/user secrets and are never sent through enterprise sync.
