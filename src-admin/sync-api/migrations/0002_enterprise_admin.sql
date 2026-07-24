ALTER TABLE translation_records ADD COLUMN deleted_at TEXT;
ALTER TABLE audit_log ADD COLUMN actor TEXT;
UPDATE audit_log SET actor = actor_device_id WHERE actor IS NULL;

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
