import { env } from "cloudflare:workers";

type RecordRow = { entity_type:string; entity_id:string; payload:string; updated_at:string };
type DeviceRow = { device_id:string; last_seen_at:string; last_cursor:number };
type AuditRow = { action:string; actor:string|null; metadata:string; created_at:string };
type DecisionRow = { entity_type:string; entity_id:string; decision:string };
type MemberRow = { email:string; role:"viewer"|"reviewer"|"admin"; created_at:string };
type DeviceTokenRow = { id:string; device_id:string; role:"reader"|"editor"; label:string|null; created_at:string; expires_at:string|null; revoked_at:string|null; last_used_at:string|null };

export type DashboardItem = { id:string; entityType:"dictionary"|"translation_memory"; source:string; translation:string; direction:string; sourceType:string; status:string };
export type DashboardDevice = { id:string; lastSeen:string; cursor:number };
export type DashboardActivity = { action:string; actor:string; detail:string; createdAt:string };
export type DashboardMember = { email:string; role:"viewer"|"reviewer"|"admin"; createdAt:string };
export type DashboardDeviceToken = { id:string; deviceId:string; role:"reader"|"editor"; label:string; createdAt:string; expiresAt:string|null; revokedAt:string|null; lastUsedAt:string|null; active:boolean };
export type DashboardData = {
  connected:boolean;
  dictionaryCount:number;
  memoryCount:number;
  reviewCount:number;
  deviceCount:number;
  reviewItems:DashboardItem[];
  devices:DashboardDevice[];
  activities:DashboardActivity[];
  members:DashboardMember[];
  deviceTokens:DashboardDeviceToken[];
};

const empty: DashboardData = { connected:false,dictionaryCount:0,memoryCount:0,reviewCount:0,deviceCount:0,reviewItems:[],devices:[],activities:[],members:[],deviceTokens:[] };

function count(row: unknown): number {
  return row && typeof row === "object" && "count" in row
    ? Number((row as { count:unknown }).count ?? 0)
    : 0;
}
function payload(value: string): Record<string,unknown> {
  try { const parsed: unknown = JSON.parse(value); return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string,unknown> : {}; }
  catch { return {}; }
}

export async function loadDashboardData(workspaceId: string): Promise<DashboardData> {
  try {
    const [dictionary,memory,review,deviceTotal,records,devices,audit,decisions,members,deviceTokens] = await env.DB.batch([
      env.DB.prepare("SELECT COUNT(*) AS count FROM translation_records WHERE workspace_id=?1 AND entity_type='dictionary' AND operation<>'delete'").bind(workspaceId),
      env.DB.prepare("SELECT COUNT(*) AS count FROM translation_records WHERE workspace_id=?1 AND entity_type='translation_memory' AND operation<>'delete'").bind(workspaceId),
      env.DB.prepare("SELECT COUNT(*) AS count FROM translation_records r WHERE r.workspace_id=?1 AND r.entity_type='translation_memory' AND r.operation<>'delete' AND NOT EXISTS (SELECT 1 FROM review_decisions d WHERE d.workspace_id=r.workspace_id AND d.entity_type=r.entity_type AND d.entity_id=r.entity_id AND d.decision IN ('approved','rejected') AND d.created_at=(SELECT MAX(d2.created_at) FROM review_decisions d2 WHERE d2.workspace_id=r.workspace_id AND d2.entity_type=r.entity_type AND d2.entity_id=r.entity_id))").bind(workspaceId),
      env.DB.prepare("SELECT COUNT(*) AS count FROM sync_devices WHERE workspace_id=?1").bind(workspaceId),
      env.DB.prepare("SELECT entity_type,entity_id,payload,updated_at FROM translation_records WHERE workspace_id=?1 AND operation<>'delete' ORDER BY updated_at DESC LIMIT 20").bind(workspaceId),
      env.DB.prepare("SELECT device_id,last_seen_at,last_cursor FROM sync_devices WHERE workspace_id=?1 ORDER BY last_seen_at DESC LIMIT 8").bind(workspaceId),
      env.DB.prepare("SELECT action,COALESCE(actor,actor_device_id) AS actor,metadata,created_at FROM audit_log WHERE workspace_id=?1 ORDER BY created_at DESC LIMIT 12").bind(workspaceId),
      env.DB.prepare("SELECT d.entity_type,d.entity_id,d.decision FROM review_decisions d WHERE d.workspace_id=?1 AND d.created_at=(SELECT MAX(d2.created_at) FROM review_decisions d2 WHERE d2.workspace_id=d.workspace_id AND d2.entity_type=d.entity_type AND d2.entity_id=d.entity_id)").bind(workspaceId),
      env.DB.prepare("SELECT email,role,created_at FROM workspace_members WHERE workspace_id=?1 ORDER BY CASE role WHEN 'admin' THEN 1 WHEN 'reviewer' THEN 2 ELSE 3 END,email").bind(workspaceId),
      env.DB.prepare("SELECT id,device_id,role,label,created_at,expires_at,revoked_at,last_used_at FROM sync_device_tokens WHERE workspace_id=?1 ORDER BY created_at DESC LIMIT 50").bind(workspaceId),
    ]);
    const latestDecision = new Map((decisions.results as DecisionRow[]).map((entry) => [`${entry.entity_type}:${entry.entity_id}`,entry.decision]));
    return {
      connected:true,
      dictionaryCount:count(dictionary.results[0]), memoryCount:count(memory.results[0]), reviewCount:count(review.results[0]), deviceCount:count(deviceTotal.results[0]),
      reviewItems:(records.results as RecordRow[]).map((record) => {
        const data = payload(record.payload);
        const sourceLang = String(data.sourceLang ?? data.source_lang ?? "?").toUpperCase();
        const targetLang = String(data.targetLang ?? data.target_lang ?? "?").toUpperCase();
        const decision = latestDecision.get(`${record.entity_type}:${record.entity_id}`);
        const status = decision ? decision[0].toUpperCase()+decision.slice(1) : record.entity_type === "dictionary" ? "Approved" : "Review";
        return { id:record.entity_id,entityType:record.entity_type as "dictionary"|"translation_memory",source:String(data.sourceText ?? data.source_text ?? data.source ?? ""),translation:String(data.translation ?? data.targetText ?? data.target_text ?? ""),direction:`${sourceLang} → ${targetLang}`,sourceType:record.entity_type === "dictionary" ? "Dictionary" : String(data.provider ?? "Memory"),status };
      }),
      devices:(devices.results as DeviceRow[]).map((device) => ({ id:device.device_id,lastSeen:device.last_seen_at,cursor:device.last_cursor })),
      activities:(audit.results as AuditRow[]).map((entry) => ({ action:entry.action,actor:entry.actor ?? "system",detail:String(payload(entry.metadata).decision ?? payload(entry.metadata).role ?? "Workspace event"),createdAt:entry.created_at })),
      members:(members.results as MemberRow[]).map((member) => ({ email:member.email,role:member.role,createdAt:member.created_at })),
      deviceTokens:(deviceTokens.results as DeviceTokenRow[]).map((token) => ({ id:token.id,deviceId:token.device_id,role:token.role,label:token.label??"",createdAt:token.created_at,expiresAt:token.expires_at,revokedAt:token.revoked_at,lastUsedAt:token.last_used_at,active:token.revoked_at===null&&(token.expires_at===null||Date.parse(token.expires_at)>Date.now()) })),
    };
  } catch (error) {
    console.error(JSON.stringify({ message:"dashboard query failed",error:error instanceof Error ? error.message : String(error) }));
    return empty;
  }
}
