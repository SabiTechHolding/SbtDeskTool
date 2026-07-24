# Cloudflare translation infrastructure

## Deployed resources

- D1: `sbt-desk-translation` (APAC)
- Sync API Worker: `sbt-desk-translation-api`
  - `https://sbt-desk-translation-api.thangngo-it195.workers.dev`
- Private Web Admin Worker: `sbt-desk-translation-admin`
  - `https://sbt-desk-translation-admin.thangngo-it195.workers.dev`

The desktop uses the Sync API URL by default. Each bearer token is scoped to
one workspace and device, has a `reader` or `editor` role, and is stored in the
operating-system credential store. D1 stores only its SHA-256 hash.

## Free-tier architecture

```text
SbtDesk desktop
  -> HTTPS Worker sync API
  -> D1 translation database

Administrator
  -> Cloudflare Access authentication
  -> Web Admin Worker + static assets
  -> D1 binding
```

No desktop client connects directly to D1. Provider API keys are never sent to
Cloudflare.

## Wrangler workflow

Run from `src-admin/sync-api`:

```powershell
wrangler login
wrangler d1 migrations apply sbt-desk-translation --remote
wrangler types worker-configuration.d.ts
wrangler deploy --dry-run
wrangler deploy --minify --keep-vars
```

`SYNC_API_TOKEN` is retained only as a disabled rollback secret.
`ALLOW_LEGACY_SYNC_TOKEN` must remain `false` in production. Issue and revoke
active credentials through Web Admin; never copy a token into source control.

Build and deploy the Web Admin from `src-admin`. The vinext build writes the
Worker deployment configuration to `dist/server/wrangler.json`:

```powershell
npm ci
npm run build
npx wrangler deploy --config dist/server/wrangler.json --minify --keep-vars
```

Keep `--keep-vars` so a generated config cannot erase production variables or
secrets. Cloudflare Access remains the outer authentication layer.

## Required security configuration

- `sync_device_tokens` contains revocable per-device credentials. Raw tokens
  are shown once and are never stored in D1 or logs.
- `SYNC_API_TOKEN` is a rotated, disabled rollback secret; it is accepted only
  if `ALLOW_LEGACY_SYNC_TOKEN` is explicitly changed to `true`.
- `TRANSLATION_ADMIN_EMAILS` is the optional application-level administrator
  allowlist. Keep Cloudflare Access restricted while it is empty; populate it
  before granting dashboard access to other users.
- The app validates the authenticated identity supplied by Cloudflare Access.
  Requests without that identity render an access-required page and do not query or
  expose workspace data.

## Verification

```powershell
wrangler d1 migrations list sbt-desk-translation --remote
wrangler d1 execute sbt-desk-translation --remote --command `
  "SELECT workspace_id,device_id,last_cursor FROM sync_devices"
wrangler tail sbt-desk-translation-api --status error
```

The production smoke test completed with the credential scoped to `nambv-pc`
in workspace `sabitech`. Reader push, wrong-device access and stale-version
conflict behavior were also verified locally before deployment.
