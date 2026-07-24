import vinext from "vinext";
import { defineConfig } from "vite";
import hostingConfig from "./.openai/hosting.json";
import { sites } from "./build/sites-vite-plugin";

const TRANSLATION_DATABASE_ID = "7b33a7c6-9827-4a70-a99b-dffd59855500";
const ACCESS_TEAM_DOMAIN = "https://thangngo-it195.cloudflareaccess.com";
const ACCESS_POLICY_AUD =
  "bfad5bcac10faf0ef5256cb28e111120164c68669c2c8b555034fec06b717052";

const { d1, r2 } = hostingConfig;

// macOS Seatbelt blocks FSEvents, so Codex previews need polling for HMR.
const isCodexSeatbeltSandbox = process.env.CODEX_SANDBOX === "seatbelt";
const localAdminEmails = process.env.TRANSLATION_ADMIN_EMAILS ?? "";
const allowLocalAdminWildcard = process.env.ALLOW_LOCAL_ADMIN_WILDCARD === "true";

const localBindingConfig = {
  name: "sbt-desk-translation-admin",
  main: "./worker/index.ts",
  compatibility_date: "2026-07-24",
  compatibility_flags: ["nodejs_compat"],
  vars: {
    DEFAULT_WORKSPACE_ID: "sabitech",
    TEAM_DOMAIN: ACCESS_TEAM_DOMAIN,
    POLICY_AUD: ACCESS_POLICY_AUD,
    ...(localAdminEmails ? { TRANSLATION_ADMIN_EMAILS: localAdminEmails } : {}),
    ALLOW_LOCAL_ADMIN_WILDCARD: allowLocalAdminWildcard ? "true" : "false",
  },
  d1_databases: d1
    ? [
        {
          binding: d1,
          database_name: "sbt-desk-translation",
          database_id: TRANSLATION_DATABASE_ID,
        },
      ]
    : [],
  r2_buckets: r2
    ? [
        {
          binding: r2,
          bucket_name: "site-creator-r2",
        },
      ]
    : [],
};

export default defineConfig(async () => {
  // Keep Wrangler and Miniflare state project-local. These are non-secret tool
  // settings; application environment belongs in ignored `.env*` files.
  process.env.WRANGLER_WRITE_LOGS ??= "false";
  process.env.WRANGLER_LOG_PATH ??= ".wrangler/logs";
  process.env.MINIFLARE_REGISTRY_PATH ??= ".wrangler/registry";

  // Wrangler snapshots its log path while the Cloudflare plugin is imported.
  const { cloudflare } = await import("@cloudflare/vite-plugin");

  return {
    server: isCodexSeatbeltSandbox
      ? { watch: { useFsEvents: false, usePolling: true } }
      : undefined,
    plugins: [
      vinext(),
      sites(),
      cloudflare({
        viteEnvironment: { name: "rsc", childEnvironments: ["ssr"] },
        config: localBindingConfig,
      }),
    ],
  };
});
