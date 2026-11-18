use serde_json::{Value,json};
use sqlx::PgPool;
use crate::metrics::Window;

pub async fn overview(pool:&PgPool,w:Window)->Result<Value,sqlx::Error>{
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY ledger_daily_metrics").execute(pool).await?;
    let row=sqlx::query_scalar::<_,Value>(include_str!("../queries/overview.sql"))
        .bind(w.from).bind(w.to).fetch_one(pool).await?;
    let number=|key:&str|row[key].as_f64().unwrap_or(0.0);
    let sample=row["samples"].as_i64().unwrap_or(0);
    let freshness=match row["last_received_at"].as_str().and_then(|s|sqlx::types::chrono::DateTime::parse_from_rfc3339(s).ok()).map(|t|t.with_timezone(&sqlx::types::chrono::Utc)) {
        None=>"unavailable",
        Some(t) if (sqlx::types::chrono::Utc::now()-t).num_hours()<=1=>"fresh",
        Some(t) if (sqlx::types::chrono::Utc::now()-t).num_hours()<=24=>"delayed",
        Some(_)=>"stale",
    };
    let vals=[("consult_citation_rate",number("consult_n"),number("consult_d"),sample,"ratio"),("warning_precision",number("warning_n"),number("warning_d"),sample,"ratio"),("local_coverage",number("coverage_n"),number("coverage_d"),sample,"ratio"),("merge_escape_rate",number("escape_n"),number("escape_d"),sample,"ratio"),("rework_cost_ratio",number("rework_n"),number("rework_d"),sample,"ratio"),("declaration_mismatch_rate",number("mismatch_n"),number("mismatch_d"),sample,"ratio")];
    let mut metrics:Vec<Value>=vals.into_iter().map(|(key,n,d,s,u)|json!({"key":key,"value":if d==0.0 {Value::Null}else{json!(n/d)},"numerator":n,"denominator":d,"sample_count":s,"unit":u,"freshness":freshness,"distribution":[]})).collect();
    let dist=json!([{"bucket":"0_30_days","rule_count":row["age_0_30"].as_i64().unwrap_or(0)},{"bucket":"31_90_days","rule_count":row["age_31_90"].as_i64().unwrap_or(0)},{"bucket":"91_180_days","rule_count":row["age_91_180"].as_i64().unwrap_or(0)},{"bucket":"over_180_days","rule_count":row["age_over_180"].as_i64().unwrap_or(0)},{"bucket":"unknown","rule_count":row["age_unknown"].as_i64().unwrap_or(0)}]);
    let age_total=dist.as_array().unwrap().iter().map(|v|v["rule_count"].as_i64().unwrap_or(0)).sum::<i64>();
    metrics.push(json!({"key":"rule_age_distribution","value":null,"numerator":0,"denominator":age_total,"sample_count":age_total,"unit":"distribution","freshness":freshness,"distribution":dist}));
    Ok(json!({"window":{"from":w.from.to_rfc3339(),"to":w.to.to_rfc3339(),"label":w.label},"generated_at":sqlx::types::chrono::Utc::now().to_rfc3339(),"metrics":metrics}))
}

pub async fn rule_health(pool:&PgPool,days:i64,statuses:Option<Vec<String>>)->Result<Value,sqlx::Error>{
    let rows=sqlx::query_as::<_,(String,String,i64,Option<f64>,Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,Option<sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>>,sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,String,String)>(include_str!("../queries/rule_health.sql"))
        .bind(days).fetch_all(pool).await?;
    let notices:Vec<Value>=rows.into_iter().filter(|r|statuses.as_ref().is_none_or(|s|s.contains(&r.1))).map(|r|json!({"rule_id":r.0,"status":r.1,"sample_count":r.2,"noise_rate":r.3,"last_hit_at":r.4.map(|d|d.to_rfc3339()),"expires_at":r.5.map(|d|d.to_rfc3339()),"first_detected_at":r.6.to_rfc3339(),"message":r.7,"action":r.8})).collect();
    let to=sqlx::types::chrono::Utc::now();Ok(json!({"window":{"from":(to-std::time::Duration::from_secs(days as u64*86400)).to_rfc3339(),"to":to.to_rfc3339(),"label":format!("{days}d")},"generated_at":to.to_rfc3339(),"notices":notices}))
}
