pub fn parse_window(window:Option<&str>)->Option<i64>{match window.unwrap_or("7d"){"7d"=>Some(7),"30d"=>Some(30),"90d"=>Some(90),_=>None}}

pub fn noise_rate(not_triggered:u64,human_override:u64,total:u64)->Option<f64>{if total==0{None}else{Some((not_triggered+human_override) as f64/total as f64)}}
pub fn noise_status(rate:Option<f64>)->bool{rate.is_some_and(|r|r>0.70)}

#[cfg(test)]mod tests{use super::*;#[test]fn snr_threshold_and_zero_samples(){assert_eq!(noise_rate(0,0,0),None);assert!(!noise_status(Some(0.70)));assert!(noise_status(Some(0.71)));}}
