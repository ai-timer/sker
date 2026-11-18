CREATE MATERIALIZED VIEW IF NOT EXISTS ledger_daily_metrics AS
SELECT date_trunc('day', occurred_at) AS metric_day,
       count(*) FILTER (WHERE event_type='consult') AS consult_events,
       count(*) FILTER (WHERE event_type='consult' AND COALESCE((payload->'counters'->>'match_count')::numeric,0)>0) AS consult_cited,
       count(*) FILTER (WHERE event_type='check' AND payload->'labels'->>'outcome' IN ('warn','block')) AS warnings,
       count(*) FILTER (WHERE event_type='check' AND payload->'labels'->>'outcome'='pass') AS successful_checks,
       count(*) FILTER (WHERE event_type='ci_gate') AS ci_events,
       count(*) FILTER (WHERE event_type='ci_gate' AND payload->'labels'->>'outcome'='pass') AS ci_passes,
       count(*) FILTER (WHERE event_type='ci_gate' AND payload->'labels'->>'outcome'='block') AS merge_escapes,
       sum(COALESCE((payload->'counters'->>'changed_line_count')::numeric,0)) AS changed_lines,
       sum(COALESCE((payload->'counters'->>'duration_ms')::numeric,0)) AS duration_ms
FROM governance_events GROUP BY 1 WITH NO DATA;
CREATE UNIQUE INDEX IF NOT EXISTS ledger_daily_metrics_day_idx ON ledger_daily_metrics(metric_day);
