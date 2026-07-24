import { env } from "cloudflare:workers";
import { getAdminAccess, hasRole, type WorkspaceRole } from "../../admin-auth";

type ReviewAction = { action:"review"; workspaceId:string; entityType:"dictionary"|"translation_memory"; entityId:string; decision:"approved"|"rejected"|"conflict"; note?:string };
type UpsertMemberAction = { action:"upsert_member"; workspaceId:string; email:string; role:WorkspaceRole };
type RemoveMemberAction = { action:"remove_member"; workspaceId:string; email:string };
type CreateDeviceTokenAction = { action:"create_device_token"; workspaceId:string; deviceId:string; role:"reader"|"editor"; label?:string; expiresAt?:string|null };
type RevokeDeviceTokenAction = { action:"revoke_device_token"; workspaceId:string; tokenId:string };
type AdminAction = ReviewAction | UpsertMemberAction | RemoveMemberAction | CreateDeviceTokenAction | RevokeDeviceTokenAction;

const MAX_BODY_BYTES = 16 * 1024;
const emailPattern = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
const json = (body:Record<string,unknown>,status=200) => Response.json(body,{status,headers:{"cache-control":"no-store"}});
const validWorkspaceId = (value:unknown): value is string => typeof value === "string" && /^[a-zA-Z0-9_-]{1,80}$/.test(value);
const validDeviceId = (value:unknown): value is string => typeof value === "string" && /^[a-zA-Z0-9_.:-]{1,120}$/.test(value);

function base64Url(bytes:Uint8Array):string {
  let binary="";
  for(const byte of bytes) binary+=String.fromCharCode(byte);
  return btoa(binary).replaceAll("+","-").replaceAll("/","_").replace(/=+$/,"");
}

async function sha256Hex(value:string):Promise<string> {
  const digest=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest),(byte)=>byte.toString(16).padStart(2,"0")).join("");
}

async function parseAction(request:Request): Promise<AdminAction|null> {
  if (!request.headers.get("content-type")?.toLowerCase().startsWith("application/json")) return null;
  if (Number(request.headers.get("content-length") ?? 0) > MAX_BODY_BYTES) return null;
  if (!request.body) return null;
  const reader = request.body.getReader();
  const chunks:Uint8Array[]=[];
  let size=0;
  while (true) {
    const {done,value}=await reader.read();
    if (done) break;
    size+=value.byteLength;
    if (size>MAX_BODY_BYTES) { await reader.cancel(); return null; }
    chunks.push(value);
  }
  const bytes=new Uint8Array(size);
  let offset=0;
  for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
  const text=new TextDecoder().decode(bytes);
  try {
    const value:unknown = JSON.parse(text);
    return value && typeof value === "object" ? value as AdminAction : null;
  } catch { return null; }
}

export async function POST(request:Request): Promise<Response> {
  const body = await parseAction(request);
  if (!body || !validWorkspaceId(body.workspaceId)) return json({ok:false,error:"Invalid request"},400);
  const access = await getAdminAccess(body.workspaceId);
  if (!access.allowed) return json({ok:false,error:"Unauthorized"},401);
  const now = new Date().toISOString();

  if (body.action === "review") {
    if (!hasRole(access.role,"reviewer")) return json({ok:false,error:"Reviewer role required"},403);
    if (!(["dictionary","translation_memory"] as const).includes(body.entityType)
      || !(["approved","rejected","conflict"] as const).includes(body.decision)
      || typeof body.entityId !== "string" || body.entityId.length < 1 || body.entityId.length > 200
      || (body.note !== undefined && (typeof body.note !== "string" || body.note.length > 1000))) {
      return json({ok:false,error:"Invalid review decision"},400);
    }
    const record = await env.DB.prepare("SELECT 1 AS found FROM translation_records WHERE workspace_id=?1 AND entity_type=?2 AND entity_id=?3 AND operation<>'delete' LIMIT 1")
      .bind(body.workspaceId,body.entityType,body.entityId).first<{found:number}>();
    if (!record) return json({ok:false,error:"Translation record not found"},404);
    await env.DB.batch([
      env.DB.prepare("INSERT INTO review_decisions(id,workspace_id,entity_type,entity_id,decision,reviewer_email,note,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")
        .bind(crypto.randomUUID(),body.workspaceId,body.entityType,body.entityId,body.decision,access.user.email.toLowerCase(),body.note?.trim()||null,now),
      env.DB.prepare("INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at) VALUES(?1,'web-admin',?2,'review_decision',?3,?4,?5,?6)")
        .bind(body.workspaceId,access.user.email.toLowerCase(),body.entityType,body.entityId,JSON.stringify({decision:body.decision}),now),
    ]);
    return json({ok:true});
  }

  if (!hasRole(access.role,"admin")) return json({ok:false,error:"Admin role required"},403);

  if (body.action === "create_device_token") {
    const label=typeof body.label==="string"?body.label.trim():"";
    const expiresAt=body.expiresAt??null;
    if (!validDeviceId(body.deviceId)
      || !(["reader","editor"] as const).includes(body.role)
      || label.length>120
      || (expiresAt!==null && (typeof expiresAt!=="string" || Number.isNaN(Date.parse(expiresAt)) || Date.parse(expiresAt)<=Date.now()))) {
      return json({ok:false,error:"Invalid device credential"},400);
    }
    const random=new Uint8Array(32);
    crypto.getRandomValues(random);
    const token=`sbt_sync_${base64Url(random)}`;
    const tokenId=crypto.randomUUID();
    await env.DB.batch([
      env.DB.prepare("INSERT INTO sync_device_tokens(id,workspace_id,device_id,token_hash,role,label,created_by,created_at,expires_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)")
        .bind(tokenId,body.workspaceId,body.deviceId,await sha256Hex(token),body.role,label||null,access.user.email.toLowerCase(),now,expiresAt),
      env.DB.prepare("INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at) VALUES(?1,'web-admin',?2,'device_token_created','sync_device_token',?3,?4,?5)")
        .bind(body.workspaceId,access.user.email.toLowerCase(),tokenId,JSON.stringify({deviceId:body.deviceId,role:body.role,label:label||null,expiresAt}),now),
    ]);
    return json({ok:true,token,tokenId});
  }
  if (body.action === "revoke_device_token") {
    if (typeof body.tokenId!=="string" || !/^[0-9a-f-]{36}$/i.test(body.tokenId)) return json({ok:false,error:"Invalid token id"},400);
    const existing=await env.DB.prepare("SELECT device_id FROM sync_device_tokens WHERE id=?1 AND workspace_id=?2 AND revoked_at IS NULL")
      .bind(body.tokenId,body.workspaceId).first<{device_id:string}>();
    if (!existing) return json({ok:false,error:"Active device credential not found"},404);
    await env.DB.batch([
      env.DB.prepare("UPDATE sync_device_tokens SET revoked_at=?1 WHERE id=?2 AND workspace_id=?3 AND revoked_at IS NULL").bind(now,body.tokenId,body.workspaceId),
      env.DB.prepare("INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at) VALUES(?1,'web-admin',?2,'device_token_revoked','sync_device_token',?3,?4,?5)")
        .bind(body.workspaceId,access.user.email.toLowerCase(),body.tokenId,JSON.stringify({deviceId:existing.device_id}),now),
    ]);
    return json({ok:true});
  }

  if (body.action !== "upsert_member" && body.action !== "remove_member") return json({ok:false,error:"Unsupported action"},400);
  const email = typeof body.email === "string" ? body.email.trim().toLowerCase() : "";
  if (!emailPattern.test(email) || email.length > 254) return json({ok:false,error:"Invalid email"},400);

  if (body.action === "upsert_member") {
    if (!(["viewer","reviewer","admin"] as const).includes(body.role)) return json({ok:false,error:"Invalid role"},400);
    await env.DB.batch([
      env.DB.prepare("INSERT INTO workspace_members(workspace_id,email,role,created_at) VALUES(?1,?2,?3,?4) ON CONFLICT(workspace_id,email) DO UPDATE SET role=excluded.role")
        .bind(body.workspaceId,email,body.role,now),
      env.DB.prepare("INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at) VALUES(?1,'web-admin',?2,'member_upsert','workspace_member',?3,?4,?5)")
        .bind(body.workspaceId,access.user.email.toLowerCase(),email,JSON.stringify({role:body.role}),now),
    ]);
    return json({ok:true});
  }
  if (body.action === "remove_member") {
    if (email === access.user.email.toLowerCase()) return json({ok:false,error:"You cannot remove your own access"},409);
    await env.DB.batch([
      env.DB.prepare("DELETE FROM workspace_members WHERE workspace_id=?1 AND email=?2").bind(body.workspaceId,email),
      env.DB.prepare("INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at) VALUES(?1,'web-admin',?2,'member_removed','workspace_member',?3,'{}',?4)")
        .bind(body.workspaceId,access.user.email.toLowerCase(),email,now),
    ]);
    return json({ok:true});
  }
  return json({ok:false,error:"Unsupported action"},400);
}
