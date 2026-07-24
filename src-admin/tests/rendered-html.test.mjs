import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function source(name) {
  return readFile(new URL(name, root), "utf8");
}

test("admin console exposes the operational sections", async () => {
  const page = await source("app/page.tsx");

  assert.match(page, /Translation operations/);
  assert.match(page, /Review queue/);
  assert.match(page, /Members & roles/);
  assert.match(page, /Recent activity/);
  assert.doesNotMatch(page, /Your site is taking shape|Building your site/);
});

test("admin mutations remain server-authorized and audited", async () => {
  const [route, controls, auth] = await Promise.all([
    source("app/api/admin/route.ts"),
    source("app/AdminControls.tsx"),
    source("app/admin-auth.ts"),
  ]);

  assert.match(route, /getAdminAccess\(body\.workspaceId\)/);
  assert.match(route, /hasRole\(access\.role,"reviewer"\)/);
  assert.match(route, /hasRole\(access\.role,"admin"\)/);
  assert.match(route, /INSERT INTO audit_log/);
  assert.match(controls, /action:"review"/);
  assert.match(controls, /action:"upsert_member"/);
  assert.match(controls, /action:"remove_member"/);
  assert.match(controls, /action:"create_device_token"/);
  assert.match(controls, /action:"revoke_device_token"/);
  assert.match(route, /token_hash/);
  assert.match(route, /sha256Hex/);
  assert.match(auth, /workspace_members/);
  assert.doesNotMatch(route, /TRANSLATION_ADMIN_EMAILS|SYNC_API_TOKEN/);
});

test("Cloudflare Access identity is accepted only after JWT verification", async () => {
  const [auth, config] = await Promise.all([
    source("app/chatgpt-auth.ts"),
    source("vite.config.ts"),
  ]);

  assert.match(auth, /requestHeaders\.get\(ACCESS_JWT_HEADER\)/);
  assert.match(auth, /createRemoteJWKSet/);
  assert.match(auth, /jwtVerify\(token, keySet/);
  assert.match(auth, /issuer:\s*teamDomain/);
  assert.match(auth, /audience:\s*policyAudience/);
  assert.match(auth, /typeof payload\.email === "string"/);
  assert.doesNotMatch(auth, /cf-access-authenticated-user-email/);
  assert.match(config, /TEAM_DOMAIN:\s*ACCESS_TEAM_DOMAIN/);
  assert.match(config, /POLICY_AUD:\s*ACCESS_POLICY_AUD/);
});
