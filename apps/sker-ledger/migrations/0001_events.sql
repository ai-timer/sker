CREATE TABLE IF NOT EXISTS governance_events (
  event_id UUID PRIMARY KEY,
  occurred_at TIMESTAMPTZ NOT NULL,
  event_type TEXT NOT NULL CHECK (event_type IN ('consult','search','check','attest','proposal','ci_gate','replay','compile')),
  rule_id VARCHAR(128),
  bundle_sha256 CHAR(64) NOT NULL CHECK (bundle_sha256 ~ '^[0-9a-f]{64}$'),
  track TEXT CHECK (track IS NULL OR track IN ('deterministic','semantic')),
  state TEXT CHECK (state IS NULL OR state IN ('comply','not_triggered_in_logic','exempt','escalate')),
  payload JSONB NOT NULL,
  received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS governance_events_occurred_idx ON governance_events(occurred_at);
CREATE INDEX IF NOT EXISTS governance_events_rule_time_idx ON governance_events(rule_id, occurred_at) WHERE rule_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS governance_events_type_time_idx ON governance_events(event_type, occurred_at);
CREATE TABLE IF NOT EXISTS rule_lifecycles (
  rule_id VARCHAR(128) PRIMARY KEY,
  activated_at TIMESTAMPTZ,
  cooldown_hours INTEGER CHECK (cooldown_hours IS NULL OR cooldown_hours >= 0),
  hotfix_expires_at TIMESTAMPTZ,
  review_by TIMESTAMPTZ,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CHECK (hotfix_expires_at IS NULL OR cooldown_hours IS NULL)
);
ALTER TABLE rule_lifecycles ADD COLUMN IF NOT EXISTS review_by TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS rule_lifecycles_expiry_idx ON rule_lifecycles(hotfix_expires_at) WHERE hotfix_expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS rule_lifecycles_review_idx ON rule_lifecycles(review_by) WHERE review_by IS NOT NULL;
