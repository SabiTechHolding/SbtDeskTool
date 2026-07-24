CREATE TABLE IF NOT EXISTS sync_device_tokens (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL,
  device_id TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  role TEXT NOT NULL CHECK (role IN ('reader', 'editor')),
  label TEXT,
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  expires_at TEXT,
  revoked_at TEXT,
  last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_device_tokens_workspace_device
  ON sync_device_tokens(workspace_id, device_id);

CREATE INDEX IF NOT EXISTS idx_sync_device_tokens_active
  ON sync_device_tokens(workspace_id, revoked_at, expires_at);
