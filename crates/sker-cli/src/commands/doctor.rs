use super::{bundle, find_repo};
use anyhow::Result;
use std::{fs, path::Path};
pub fn run(path: &Path) -> Result<()> {
    let repo = find_repo(path)?;
    let mut failed = false;
    let b = bundle(&repo);
    report(
        "bundle hash",
        b.as_ref().map(|_| ()).map_err(|e| e.to_string()),
        &mut failed,
    );
    let max = 300usize;
    for file in ["AGENTS.md", "CLAUDE.md"] {
        let p = repo.join(file);
        if p.exists() {
            let text = fs::read_to_string(&p)?;
            let block = managed_block(&text);
            let ok = block.is_some_and(|s| s.split_whitespace().count() <= max);
            report(
                &format!("{file} managed block <= {max} tokens"),
                if ok {
                    Ok(())
                } else {
                    Err("managed block exceeds token limit or is malformed".into())
                },
                &mut failed,
            );
        }
    }
    let settings = repo.join(".mcp.json");
    let codex = repo.join(".codex/config.toml");
    let hook = repo.join(".claude/settings.json");
    report(
        "MCP configuration",
        if settings.exists() || codex.exists() {
            Ok(())
        } else {
            Err("no MCP host config; run `sker init` and register `sker mcp`".into())
        },
        &mut failed,
    );
    if repo.join(".claude").exists()
        || repo.join("CLAUDE.md").exists()
        || std::env::var_os("HOME")
            .is_some_and(|h| std::path::PathBuf::from(h).join(".claude").exists())
    {
        report(
            "Hook configuration",
            if hook.exists() {
                Ok(())
            } else {
                Err("no Claude Hook config; run `sker init` and configure PreToolUse".into())
            },
            &mut failed,
        );
    } else {
        println!("SKIP Hook configuration: no Claude host detected")
    }
    if let Ok(url) = std::env::var("SKER_LEDGER_URL") {
        let endpoint = url.trim_end_matches('/');
        let ok = std::net::TcpStream::connect_timeout(
            &format_addr(endpoint),
            std::time::Duration::from_millis(300),
        )
        .is_ok();
        report(
            "optional Ledger",
            if ok {
                Ok(())
            } else {
                Err(format!("Ledger unavailable at {endpoint}"))
            },
            &mut failed,
        )
    } else {
        println!("SKIP optional Ledger: SKER_LEDGER_URL not configured")
    };
    if failed {
        anyhow::bail!("doctor found failed checks; see repair guidance above")
    };
    Ok(())
}
fn report(label: &str, result: Result<(), String>, failed: &mut bool) {
    match result {
        Ok(()) => println!("PASS {label}"),
        Err(e) => {
            *failed = true;
            println!("FAIL {label}: {e}")
        }
    }
}
fn managed_block(s: &str) -> Option<&str> {
    let a = s.find("<!-- sker:managed:start -->")?;
    let b = s.find("<!-- sker:managed:end -->")?;
    if b < a {
        return None;
    }
    Some(&s[a..b + "<!-- sker:managed:end -->".len()])
}
fn format_addr(url: &str) -> std::net::SocketAddr {
    let without = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let default = if url.starts_with("https://") { 443 } else { 80 };
    let addr = if without
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        .is_some()
    {
        without.to_string()
    } else {
        format!("{without}:{default}")
    };
    addr.parse()
        .unwrap_or_else(|_| "127.0.0.1:9".parse().unwrap())
}
