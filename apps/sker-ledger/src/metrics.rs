use sqlx::types::chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Clone,Copy)]
pub struct Window { pub from: DateTime<Utc>, pub to: DateTime<Utc>, pub label: &'static str }

pub fn parse_window(label:Option<&str>,from:Option<&str>,to:Option<&str>)->Result<Window,&'static str>{
    let now=Utc::now();
    match label.unwrap_or("30d") {
        "7d"=>Ok(Window{from:now-Duration::from_secs(7*86400),to:now,label:"7d"}),
        "30d"=>Ok(Window{from:now-Duration::from_secs(30*86400),to:now,label:"30d"}),
        "90d"=>Ok(Window{from:now-Duration::from_secs(90*86400),to:now,label:"90d"}),
        "custom"=>{let f=from.ok_or("custom requires from and to")?.parse::<DateTime<Utc>>().map_err(|_|"from must be RFC3339")?;let t=to.ok_or("custom requires from and to")?.parse::<DateTime<Utc>>().map_err(|_|"to must be RFC3339")?;if f>=t||t>now+Duration::from_secs(60){return Err("custom window must have from < to and cannot be in the future")};Ok(Window{from:f,to:t,label:"custom"})},
        _=>Err("window must be 7d, 30d, 90d, or custom"),
    }
}

#[cfg(test)] mod tests {use super::*;#[test]fn rejects_incomplete_custom_window(){assert!(parse_window(Some("custom"),None,None).is_err());}#[test]fn accepts_standard_windows(){assert_eq!(parse_window(Some("7d"),None,None).unwrap().label,"7d");}}
