PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS workspaces (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_members (
  workspace_id TEXT NOT NULL,
  email TEXT NOT NULL,
  role TEXT NOT NULL CHECK(role IN ('viewer', 'reviewer', 'admin')),
  created_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, email)
);

CREATE TABLE IF NOT EXISTS translation_records (
  workspace_id TEXT NOT NULL,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('dictionary', 'translation_memory')),
  entity_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('upsert', 'delete')),
  payload TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL,
  updated_by_device TEXT NOT NULL,
  deleted_at TEXT,
  PRIMARY KEY (workspace_id, entity_type, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_translation_records_workspace_updated
  ON translation_records(workspace_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS sync_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id TEXT NOT NULL,
  outbox_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('dictionary', 'translation_memory')),
  entity_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('upsert', 'delete')),
  payload TEXT NOT NULL,
  version INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(workspace_id, outbox_id)
);

CREATE INDEX IF NOT EXISTS idx_sync_events_pull
  ON sync_events(workspace_id, id);

CREATE TABLE IF NOT EXISTS sync_devices (
  workspace_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  last_cursor INTEGER NOT NULL DEFAULT 0,
  last_seen_at TEXT NOT NULL,
  PRIMARY KEY (workspace_id, device_id)
);

CREATE TABLE IF NOT EXISTS audit_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id TEXT NOT NULL,
  actor_device_id TEXT NOT NULL,
  actor TEXT,
  action TEXT NOT NULL,
  entity_type TEXT,
  entity_id TEXT,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_workspace_time
  ON audit_log(workspace_id, created_at DESC);

CREATE TABLE IF NOT EXISTS review_decisions (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  entity_type TEXT NOT NULL CHECK(entity_type IN ('dictionary', 'translation_memory')),
  entity_id TEXT NOT NULL,
  decision TEXT NOT NULL CHECK(decision IN ('approved', 'rejected', 'conflict')),
  reviewer_email TEXT NOT NULL,
  note TEXT,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_review_workspace_created
  ON review_decisions(workspace_id, created_at DESC);
