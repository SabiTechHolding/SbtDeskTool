CREATE TABLE IF NOT EXISTS translation_records (
  workspace_id TEXT NOT NULL,
  entity_type TEXT NOT NULL CHECK (entity_type IN ('dictionary', 'translation_memory')),
  entity_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
  payload TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL,
  updated_by_device TEXT NOT NULL,
  PRIMARY KEY (workspace_id, entity_type, entity_id)
);

CREATE TABLE IF NOT EXISTS sync_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id TEXT NOT NULL,
  outbox_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id TEXT NOT NULL,
  operation TEXT NOT NULL,
  payload TEXT NOT NULL,
  version INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE (workspace_id, outbox_id)
);

CREATE INDEX IF NOT EXISTS idx_sync_events_pull
  ON sync_events (workspace_id, id);

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
  action TEXT NOT NULL,
  entity_type TEXT,
  entity_id TEXT,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_workspace_time
  ON audit_log (workspace_id, created_at DESC);
