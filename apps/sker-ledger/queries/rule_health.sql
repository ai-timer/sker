WITH hits AS (
 SELECT rule_id, count(*) FILTER (WHERE occurred_at >= now()-interval '7 days')::bigint weekly_samples,
 count(*) FILTER (WHERE occurred_at >= now()-interval '7 days' AND state IN ('not_triggered_in_logic','exempt'))::float8 noise_events,
 max(occurred_at) last_hit FROM governance_events WHERE rule_id IS NOT NULL GROUP BY rule_id
), all_rules AS (
 SELECT rule_id FROM rule_lifecycles UNION SELECT DISTINCT rule_id FROM governance_events WHERE rule_id IS NOT NULL
), notices AS (
 SELECT a.rule_id,'suspected_dead'::text status,COALESCE(h.weekly_samples,0)::bigint sample_count,
 CASE WHEN COALESCE(h.weekly_samples,0)=0 THEN NULL ELSE h.noise_events/h.weekly_samples END noise_rate,
 h.last_hit last_hit_at,NULL::timestamptz expires_at,now() first_detected_at,
 'No rule hit for 180 days; review this rule.'::text message,'review'::text action
 FROM all_rules a LEFT JOIN hits h USING(rule_id) WHERE COALESCE(h.last_hit,'-infinity'::timestamptz)<now()-interval '180 days'
 UNION ALL
 SELECT h.rule_id,'noisy',h.weekly_samples,h.noise_events/NULLIF(h.weekly_samples,0),h.last_hit,NULL,now(),
 'Weekly signal-to-noise rate exceeds 70%; review trigger quality.','review'
 FROM hits h WHERE h.weekly_samples>0 AND h.noise_events/h.weekly_samples>0.70
 UNION ALL
 SELECT rule_id,CASE WHEN COALESCE(hotfix_expires_at,activated_at+make_interval(hours=>cooldown_hours))<=now() THEN 'lifecycle_expired' ELSE 'lifecycle_expiring' END,0,NULL,NULL,
 COALESCE(hotfix_expires_at,activated_at+make_interval(hours=>cooldown_hours)),now(),
 CASE WHEN COALESCE(hotfix_expires_at,activated_at+make_interval(hours=>cooldown_hours))<=now() THEN 'Lifecycle deadline has passed.' ELSE 'Rule lifecycle deadline is approaching.' END,
 CASE WHEN COALESCE(hotfix_expires_at,activated_at+make_interval(hours=>cooldown_hours))<=now() THEN 'renew' ELSE 'review' END
 FROM rule_lifecycles WHERE (hotfix_expires_at IS NOT NULL AND hotfix_expires_at<=now()+interval '7 days')
 OR (cooldown_hours IS NOT NULL AND activated_at+make_interval(hours=>cooldown_hours)<=now()+interval '7 days')
)
SELECT rule_id,status,sample_count,noise_rate,last_hit_at,expires_at,first_detected_at,message,action FROM notices;
