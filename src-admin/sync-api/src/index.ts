type JsonObject = Record<string, unknown>;

type IncomingChange = {
  id: string;
  entityType: "dictionary" | "translation_memory";
  entityId: string;
  operation: "upsert" | "delete";
  payload: JsonObject;
  createdAt: string;
  baseVersion?: number;
};

type SyncBody = {
  workspaceId: string;
  deviceId: string;
  cursor: string | null;
  changes: IncomingChange[];
};

type EventRow = {
  id: number;
  entity_type: string;
  entity_id: string;
  operation: string;
  payload: string;
  version: number;
  created_at: string;
};

type WorkerEnv = Env & {
  SYNC_API_TOKEN?: string;
  SYNC_WORKSPACE_ID: string;
  ALLOW_LEGACY_SYNC_TOKEN?: string;
};

type AuthContext = {
  workspaceId: string;
  deviceId: string | null;
  role: "reader" | "editor";
  tokenId: string | null;
  legacy: boolean;
};

type DeviceTokenRow = {
  id: string;
  workspace_id: string;
  device_id: string;
  role: "reader" | "editor";
};

type CurrentRecordRow = {
  operation: "upsert" | "delete";
  payload: string;
  version: number;
  updated_by_device: string;
};

const JSON_HEADERS = { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" };
const MAX_BODY_BYTES = 1024 * 1024;
const MAX_PUSH_CHANGES = 200;
const MAX_PULL_CHANGES = 500;

function json(data: unknown, status = 200): Response {
  return Response.json(data, { status, headers: JSON_HEADERS });
}

async function secureEqual(provided: string, expected: string): Promise<boolean> {
  const encoder = new TextEncoder();
  const [left, right] = await Promise.all([
    crypto.subtle.digest("SHA-256", encoder.encode(provided)),
    crypto.subtle.digest("SHA-256", encoder.encode(expected)),
  ]);
  const a = new Uint8Array(left);
  const b = new Uint8Array(right);
  let difference = 0;
  for (let index = 0; index < a.length; index += 1) difference |= a[index] ^ b[index];
  return difference === 0;
}

async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function authorize(request: Request, env: WorkerEnv): Promise<AuthContext | null> {
  const header = request.headers.get("authorization");
  if (!header?.startsWith("Bearer ")) return null;
  const token = header.slice(7).trim();
  if (!token || token.length > 256) return null;
  const row = await env.DB.prepare(
    `SELECT id,workspace_id,device_id,role FROM sync_device_tokens
     WHERE token_hash=?1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?2) LIMIT 1`,
  ).bind(await sha256Hex(token), new Date().toISOString()).first<DeviceTokenRow>();
  if (row) {
    await env.DB.prepare("UPDATE sync_device_tokens SET last_used_at=?1 WHERE id=?2")
      .bind(new Date().toISOString(), row.id).run();
    return { workspaceId: row.workspace_id, deviceId: row.device_id, role: row.role, tokenId: row.id, legacy: false };
  }
  if (env.ALLOW_LEGACY_SYNC_TOKEN === "true" && env.SYNC_API_TOKEN && await secureEqual(token, env.SYNC_API_TOKEN)) {
    return { workspaceId: env.SYNC_WORKSPACE_ID, deviceId: null, role: "editor", tokenId: null, legacy: true };
  }
  return null;
}

function nonEmpty(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.trim().length > 0 && value.length <= maxLength;
}

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isChange(value: unknown): value is IncomingChange {
  if (!isObject(value)) return false;
  return nonEmpty(value.id, 160)
    && (value.entityType === "dictionary" || value.entityType === "translation_memory")
    && nonEmpty(value.entityId, 160)
    && (value.operation === "upsert" || value.operation === "delete")
    && isObject(value.payload)
    && JSON.stringify(value.payload).length <= 50_000
    && nonEmpty(value.createdAt, 64)
    && (value.baseVersion === undefined || (Number.isSafeInteger(value.baseVersion) && Number(value.baseVersion) >= 0));
}

function parseBody(value: unknown): SyncBody | null {
  if (!isObject(value)
    || !nonEmpty(value.workspaceId, 120)
    || !nonEmpty(value.deviceId, 160)
    || !(value.cursor === null || value.cursor === undefined || typeof value.cursor === "string")
    || !Array.isArray(value.changes)
    || value.changes.length > MAX_PUSH_CHANGES
    || !value.changes.every(isChange)) return null;
  return {
    workspaceId: value.workspaceId,
    deviceId: value.deviceId,
    cursor: typeof value.cursor === "string" ? value.cursor : null,
    changes: value.changes,
  };
}

function cursorNumber(cursor: string | null): number {
  if (!cursor || !/^\d+$/.test(cursor)) return 0;
  const parsed = Number(cursor);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : 0;
}

async function readJsonBody(request: Request): Promise<unknown> {
  const contentLength = Number(request.headers.get("content-length") ?? "0");
  if (contentLength > MAX_BODY_BYTES) throw new Error("body_too_large");
  if (!request.body) throw new Error("invalid_json");
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY_BYTES) {
      await reader.cancel();
      throw new Error("body_too_large");
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try { return JSON.parse(new TextDecoder().decode(bytes)) as unknown; }
  catch { throw new Error("invalid_json"); }
}

async function synchronize(request: Request, env: WorkerEnv, auth: AuthContext): Promise<Response> {
  let raw: unknown;
  try { raw = await readJsonBody(request); }
  catch (error) {
    return error instanceof Error && error.message === "body_too_large"
      ? json({ error: "Request body is too large" }, 413)
      : json({ error: "Request body must be valid JSON" }, 400);
  }
  const body = parseBody(raw);
  if (!body) return json({ error: "Invalid sync request" }, 400);
  if (body.workspaceId !== auth.workspaceId || (auth.deviceId !== null && body.deviceId !== auth.deviceId)) {
    return json({ error: "Credential is not valid for this workspace and device" }, 403);
  }
  if (auth.role === "reader" && body.changes.length > 0) return json({ error: "Reader credentials cannot push changes" }, 403);
  const now = new Date().toISOString();
  const statements: D1PreparedStatement[] = [];
  const currentResults = body.changes.length
    ? await env.DB.batch(body.changes.map((change) => env.DB.prepare(
      "SELECT operation,payload,version,updated_by_device FROM translation_records WHERE workspace_id=?1 AND entity_type=?2 AND entity_id=?3",
    ).bind(body.workspaceId, change.entityType, change.entityId)))
    : [];
  const virtualRecords = new Map<string, CurrentRecordRow>();
  body.changes.forEach((change, index) => {
    const row = currentResults[index]?.results[0] as CurrentRecordRow | undefined;
    if (row) virtualRecords.set(`${change.entityType}:${change.entityId}`, row);
  });
  const conflictedEntities = new Set<string>();
  for (const change of body.changes) {
    const payload = JSON.stringify(change.payload);
    const entityKey = `${change.entityType}:${change.entityId}`;
    const current = virtualRecords.get(entityKey);
    const conflict = conflictedEntities.has(entityKey)
      || (change.baseVersion !== undefined && current !== undefined
        && current.version > change.baseVersion && current.updated_by_device !== body.deviceId);
    if (conflict && current) {
      conflictedEntities.add(entityKey);
      let canonicalPayload = current.payload;
      if (change.entityType === "translation_memory" && current.operation !== "delete") {
        const parsed = JSON.parse(current.payload) as JsonObject;
        canonicalPayload = JSON.stringify({ ...parsed, status: "conflict" });
      }
      statements.push(
        env.DB.prepare(
          `INSERT OR IGNORE INTO sync_events(workspace_id,outbox_id,device_id,entity_type,entity_id,operation,payload,version,created_at)
           VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)`,
        ).bind(body.workspaceId, change.id, `server-conflict:${body.deviceId}`, change.entityType, change.entityId, current.operation, canonicalPayload, current.version, now),
        env.DB.prepare(
          `INSERT INTO review_decisions(id,workspace_id,entity_type,entity_id,decision,reviewer_email,note,created_at)
           SELECT ?1,?2,?3,?4,'conflict','sync-server',?5,?6 FROM sync_events
           WHERE workspace_id=?2 AND outbox_id=?7 AND created_at=?6`,
        ).bind(crypto.randomUUID(), body.workspaceId, change.entityType, change.entityId, `Rejected stale base version ${change.baseVersion}; current version is ${current.version}`, now, change.id),
        env.DB.prepare(
          `INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at)
           SELECT ?1,?2,?2,'sync.conflict',?3,?4,?5,?6 FROM sync_events
           WHERE workspace_id=?1 AND outbox_id=?7 AND created_at=?6`,
        ).bind(body.workspaceId, body.deviceId, change.entityType, change.entityId, JSON.stringify({ outboxId: change.id, baseVersion: change.baseVersion, currentVersion: current.version }), now, change.id),
      );
      continue;
    }
    statements.push(
      env.DB.prepare(
        `INSERT INTO translation_records(workspace_id,entity_type,entity_id,operation,payload,version,updated_at,updated_by_device,deleted_at)
         SELECT ?1,?2,?3,?4,?5,1,?6,?7,?9
         WHERE NOT EXISTS (SELECT 1 FROM sync_events WHERE workspace_id=?1 AND outbox_id=?8)
         ON CONFLICT(workspace_id,entity_type,entity_id) DO UPDATE SET
           operation=excluded.operation,payload=excluded.payload,version=translation_records.version+1,
           updated_at=excluded.updated_at,updated_by_device=excluded.updated_by_device,deleted_at=excluded.deleted_at`,
      ).bind(body.workspaceId, change.entityType, change.entityId, change.operation, payload, now, body.deviceId, change.id, change.operation === "delete" ? now : null),
      env.DB.prepare(
        `INSERT OR IGNORE INTO sync_events(workspace_id,outbox_id,device_id,entity_type,entity_id,operation,payload,version,created_at)
         SELECT ?1,?2,?3,?4,?5,?6,?7,version,?8 FROM translation_records
         WHERE workspace_id=?1 AND entity_type=?4 AND entity_id=?5`,
      ).bind(body.workspaceId, change.id, body.deviceId, change.entityType, change.entityId, change.operation, payload, now),
      env.DB.prepare(
        `INSERT INTO audit_log(workspace_id,actor_device_id,actor,action,entity_type,entity_id,metadata,created_at)
         SELECT ?1,?2,?2,?3,?4,?5,?6,?7 FROM sync_events
         WHERE workspace_id=?1 AND outbox_id=?8 AND created_at=?7`,
      ).bind(body.workspaceId, body.deviceId, `sync.${change.operation}`, change.entityType, change.entityId, JSON.stringify({ outboxId: change.id, changes: 1 }), now, change.id),
    );
    virtualRecords.set(entityKey, {
      operation: change.operation,
      payload,
      version: current ? current.version + 1 : 1,
      updated_by_device: body.deviceId,
    });
  }
  statements.push(
    env.DB.prepare(
      `INSERT INTO sync_devices(workspace_id,device_id,last_cursor,last_seen_at) VALUES(?1,?2,?3,?4)
       ON CONFLICT(workspace_id,device_id) DO UPDATE SET last_cursor=excluded.last_cursor,last_seen_at=excluded.last_seen_at`,
    ).bind(body.workspaceId, body.deviceId, cursorNumber(body.cursor), now),
  );
  if (statements.length) await env.DB.batch(statements);

  const pulled = await env.DB.prepare(
    `SELECT id,entity_type,entity_id,operation,payload,version,created_at FROM sync_events
     WHERE workspace_id=?1 AND id>?2 AND device_id<>?3 ORDER BY id ASC LIMIT ?4`,
  ).bind(body.workspaceId, cursorNumber(body.cursor), body.deviceId, MAX_PULL_CHANGES).all<EventRow>();
  const latest = await env.DB.prepare(
    "SELECT COALESCE(MAX(id),?2) AS cursor FROM sync_events WHERE workspace_id=?1",
  ).bind(body.workspaceId, cursorNumber(body.cursor)).first<{ cursor: number }>();
  const changes = pulled.results.map((row) => ({
    entityType: row.entity_type,
    entityId: row.entity_id,
    operation: row.operation,
    payload: JSON.parse(row.payload) as unknown,
    version: row.version,
    updatedAt: row.created_at,
  }));
  const lastPulled = pulled.results[pulled.results.length - 1]?.id;
  return json({
    cursor: String(lastPulled ?? latest?.cursor ?? cursorNumber(body.cursor)),
    acknowledgedIds: body.changes.map((change) => change.id),
    changes,
  });
}

export default {
  async fetch(request: Request, env: WorkerEnv): Promise<Response> {
    const url = new URL(request.url);
    try {
      const auth = await authorize(request, env);
      if (!auth) return json({ error: "Unauthorized" }, 401);
      if (request.method === "GET" && url.pathname === "/api/v1/health") {
        const database = await env.DB.prepare("SELECT 1 AS ok").first<{ ok: number }>();
        return json({ status: database?.ok === 1 ? "ok" : "degraded", service: "sbt-desk-translation-api", workspaceId: auth.workspaceId, deviceId: auth.deviceId, role: auth.role });
      }
      if (request.method === "POST" && url.pathname === "/api/v1/translation/sync") {
        return await synchronize(request, env, auth);
      }
      return json({ error: "Not found" }, 404);
    } catch (error) {
      console.error(JSON.stringify({ message: "request failed", path: url.pathname, error: error instanceof Error ? error.message : String(error) }));
      return json({ error: "Internal server error" }, 500);
    }
  },
} satisfies ExportedHandler<WorkerEnv>;
