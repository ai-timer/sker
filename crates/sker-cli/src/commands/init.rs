use super::find_repo;
use anyhow::Result;
use std::{fs, path::Path};
const START: &str = "<!-- sker:managed:start -->";
const END: &str = "<!-- sker:managed:end -->";
pub fn run(path: &Path) -> Result<()> {
    let repo = find_repo(path)?;
    let mut languages = Vec::new();
    for (file, name) in [
        ("go.mod", "go"),
        ("package.json", "typescript"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("Cargo.toml", "rust"),
    ] {
        if repo.join(file).exists() && !languages.iter().any(|x| x == name) {
            languages.push(name.to_string())
        }
    }
    let hosts = host_list(&repo);
    for host in &hosts {
        println!("host detected: {host}")
    }
    let config = serde_json::json!({"schema_version":"sker.config/v1","languages":languages,"hosts":hosts,"rules":[]});
    let cfg = repo.join(".sker.yaml");
    if !cfg.exists() {
        fs::write(&cfg, serde_yaml_ng::to_string(&config)?)?;
    }
    install_mcp_config(&repo)?;
    if hosts.contains(&"claude") {
        install_claude_hook(&repo)?;
    }
    if hosts.contains(&"codex") {
        install_codex_mcp(&repo)?;
    }
    fs::create_dir_all(repo.join(".sker/rules"))?;
    let body = format!(
        "{START}\nBefore changing code: run `sker consult` for the relevant paths.\nAfter changes: run `sker check` and resolve deterministic BLOCK findings.\n{END}\n"
    );
    for file in ["AGENTS.md", "CLAUDE.md"] {
        let p = repo.join(file);
        let prior = fs::read_to_string(&p).unwrap_or_default();
        let updated = replace_block(&prior, &body);
        if updated != prior {
            fs::write(p, updated)?;
        }
    }
    let hosts = host_list(&repo);
    println!(
        "initialized: languages={}, hosts={}",
        languages.join(","),
        hosts.join(",")
    );
    Ok(())
}
fn host_list(repo: &Path) -> Vec<&'static str> {
    let mut v = Vec::new();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if repo.join(".claude").exists()
        || repo.join("CLAUDE.md").exists()
        || home.as_ref().is_some_and(|h| h.join(".claude").exists())
    {
        v.push("claude")
    }
    if repo.join(".codex").exists()
        || repo.join("AGENTS.md").exists()
        || home.as_ref().is_some_and(|h| h.join(".codex").exists())
    {
        v.push("codex")
    }
    v
}
fn install_mcp_config(repo: &Path) -> Result<()> {
    let path = repo.join(".mcp.json");
    let mut config: serde_json::Value = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        serde_json::json!({})
    };
    let root = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(".mcp.json must contain a JSON object"))?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(".mcp.json mcpServers must be an object"))?;
    servers.insert(
        "sker".into(),
        serde_json::json!({"command":"sker","args":["mcp"]}),
    );
    fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}
fn install_claude_hook(repo: &Path) -> Result<()> {
    let dir = repo.join(".claude");
    fs::create_dir_all(&dir)?;
    let path = dir.join("settings.json");
    let mut config: serde_json::Value = if path.exists() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        serde_json::json!({})
    };
    let root = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Claude settings.json must contain a JSON object"))?;
    let hooks = root.entry("hooks").or_insert_with(|| serde_json::json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Claude hooks setting must be an object"))?;
    hooks.insert(
        "PreToolUse".into(),
        serde_json::json!([{"matcher":"","hooks":[{"type":"command","command":"sker hook"}]}]),
    );
    fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}
fn install_codex_mcp(repo: &Path) -> Result<()> {
    let dir = repo.join(".codex");
    fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let old = fs::read_to_string(&path).unwrap_or_default();
    let start = "# sker:managed:start";
    let end = "# sker:managed:end";
    let block = format!("{start}\n[mcp_servers.sker]\ncommand = \"sker\"\nargs = [\"mcp\"]\n{end}");
    let value = if let (Some(a), Some(b)) = (old.find(start), old.find(end)) {
        format!("{}{}{}", &old[..a], block, &old[b + end.len()..])
    } else {
        format!(
            "{}{}{}",
            old,
            if old.is_empty() || old.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            block
        )
    };
    fs::write(path, value)?;
    Ok(())
}
fn replace_block(old: &str, block: &str) -> String {
    if let (Some(a), Some(b)) = (old.find(START), old.find(END)) {
        let mut s = String::new();
        s.push_str(&old[..a]);
        s.push_str(block.trim_end());
        s.push_str(&old[b + END.len()..]);
        s
    } else {
        format!(
            "{}{}{}",
            old,
            if old.is_empty() || old.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            block
        )
    }
}
