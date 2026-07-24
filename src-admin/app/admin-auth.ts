import { env } from "cloudflare:workers";
import type { ChatGPTUser } from "./chatgpt-auth";
import { getChatGPTUser } from "./chatgpt-auth";

export type WorkspaceRole = "viewer" | "reviewer" | "admin";
export type AdminAccess = {
  user: ChatGPTUser;
  allowed: boolean;
  role: WorkspaceRole | null;
};

const roleRank: Record<WorkspaceRole, number> = { viewer: 1, reviewer: 2, admin: 3 };

export async function getAdminAccess(
  workspaceId = process.env.DEFAULT_WORKSPACE_ID ?? "local",
): Promise<AdminAccess> {
  const user = await getChatGPTUser() ?? {
    displayName: "Cloudflare Access required",
    email: "not-authenticated",
    fullName: null,
  };
  const allowedEmails = (process.env.TRANSLATION_ADMIN_EMAILS ?? "")
    .split(",")
    .map((email) => email.trim().toLowerCase())
    .filter(Boolean);
  const localWildcard = process.env.ALLOW_LOCAL_ADMIN_WILDCARD === "true"
    && allowedEmails.includes("*");
  if (user.email === "not-authenticated") return { user, allowed: false, role: null };

  const email = user.email.trim().toLowerCase();
  if (localWildcard || allowedEmails.includes(email)) {
    return { user, allowed: true, role: "admin" };
  }

  let role: WorkspaceRole | null = null;
  try {
    const member = await env.DB.prepare(
      "SELECT role FROM workspace_members WHERE workspace_id=?1 AND lower(email)=?2 LIMIT 1",
    ).bind(workspaceId, email).first<{ role: WorkspaceRole }>();
    role = member?.role ?? null;
  } catch (error) {
    console.error(JSON.stringify({
      message: "workspace authorization query failed",
      error: error instanceof Error ? error.message : String(error),
    }));
  }
  return { user, allowed: role !== null, role };
}

export function hasRole(actual: WorkspaceRole | null, required: WorkspaceRole): boolean {
  return actual !== null && roleRank[actual] >= roleRank[required];
}
