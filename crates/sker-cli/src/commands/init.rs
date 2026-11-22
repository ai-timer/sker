use super::find_repo;
use anyhow::{Result, bail};
use sker_schema::{AgentConfig, find_agent_preset};
use std::{fs, path::Path};

const START: &str = "<!-- sker:managed:start -->";
const END: &str = "<!-- sker:managed:end -->";

pub fn run(path: &Path, explicit_agents: &[String], git_hooks: bool) -> Result<()> {
    let repo = find_repo(path)?;

    // Detect project languages
    let mut languages = Vec::new();
    for (file, name) in [
        ("go.mod", "go"),
        ("package.json", "typescript"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("Cargo.toml", "rust"),
    ] {
        if repo.join(file).exists() && !languages.iter().any(|x| x == name) {
            languages.push(name.to_string());
        }
    }

    // Determine enabled agents
    let mut enabled_agents: Vec<String> = Vec::new();
    if !explicit_agents.is_empty() {
        for a in explicit_agents {
            for sub in a.split(',') {
                let trimmed = sub.trim().to_lowercase();
                if trimmed.is_empty() || enabled_agents.contains(&trimmed) {
                    continue;
                }
                if find_agent_preset(&trimmed).is_none() {
                    bail!(
                        "unknown agent preset '{trimmed}'. Run 'sker agent list' for presets, or use 'sker agent add custom --name <n> --rules-file <path>' for custom integrations"
                    );
                }
                enabled_agents.push(trimmed);
            }
        }
    } else {
        // Restrained detection: only enable if host directory/file ALREADY exists in repository
        if repo.join(".claude").exists() || repo.join("CLAUDE.md").exists() {
            enabled_agents.push("claude".into());
        }
        if repo.join(".codex").exists() {
            enabled_agents.push("codex".into());
        }
        if repo.join(".agent").exists() || repo.join(".agents").exists() {
            enabled_agents.push("antigravity".into());
        }
        if repo.join(".cursor").exists() || repo.join(".cursorrules").exists() {
            enabled_agents.push("cursor".into());
        }
        if repo.join(".codebuddy").exists() {
            enabled_agents.push("codebuddy".into());
        }
        if repo.join(".windsurf").exists() || repo.join(".codeium").exists() {
            enabled_agents.push("windsurf".into());
        }
        if repo.join(".opencode").exists() {
            enabled_agents.push("opencode".into());
        }

        // Print friendly hints if detected in $HOME but not present in repository
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        if let Some(ref h) = home {
            if !enabled_agents.iter().any(|a| a == "claude") && h.join(".claude").exists() {
                println!(
                    "hint: Claude detected in $HOME; run 'sker agent add claude' to enable in this repo"
                );
            }
            if !enabled_agents.iter().any(|a| a == "codex") && h.join(".codex").exists() {
                println!(
                    "hint: Codex detected in $HOME; run 'sker agent add codex' to enable in this repo"
                );
            }
            if !enabled_agents.iter().any(|a| a == "antigravity")
                && h.join(".gemini/antigravity").exists()
            {
                println!(
                    "hint: Antigravity detected in $HOME; run 'sker agent add antigravity' to enable in this repo"
                );
            }
            if !enabled_agents.iter().any(|a| a == "cursor") && h.join(".cursor").exists() {
                println!(
                    "hint: Cursor detected in $HOME; run 'sker agent add cursor' to enable in this repo"
                );
            }
            if !enabled_agents.iter().any(|a| a == "codebuddy") && h.join(".codebuddy").exists() {
                println!(
                    "hint: CodeBuddy detected in $HOME; run 'sker agent add codebuddy' to enable in this repo"
                );
            }
        }

        // Universal fallback if no repo-specific hosts detected
        if enabled_agents.is_empty() {
            enabled_agents.push("universal".into());
        }
    }

    for agent in &enabled_agents {
        println!("agent enabled: {agent}");
    }

    // Write or update .sker.yaml
    let cfg = repo.join(".sker.yaml");
    if !cfg.exists() {
        let config = serde_json::json!({
            "schema_version": "sker.config/v1",
            "languages": languages,
            "agents": enabled_agents,
            "rules": []
        });
        fs::write(&cfg, serde_yaml_ng::to_string(&config)?)?;
    } else {
        // Idempotently ensure agents list exists
        let content = fs::read_to_string(&cfg)?;
        if let Ok(mut val) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content)
            && let Some(map) = val.as_mapping_mut()
            && !map.contains_key(serde_yaml_ng::Value::String("agents".into()))
            && !map.contains_key(serde_yaml_ng::Value::String("integrations".into()))
            && !map.contains_key(serde_yaml_ng::Value::String("hosts".into()))
        {
            let seq = serde_yaml_ng::to_value(&enabled_agents)?;
            map.insert(serde_yaml_ng::Value::String("agents".into()), seq);
            fs::write(&cfg, serde_yaml_ng::to_string(&val)?)?;
        }
    }

    // Ensure .sker/rules directory exists
    fs::create_dir_all(repo.join(".sker/rules"))?;

    // Universal AGENTS.md guide (always created or updated with managed block)
    let body = format!(
        "{START}\n1. Read the matched rule and its scope.\n2. Apply the rule, then run its positive and negative fixtures.\nBefore changing code: run `sker consult` for the relevant paths.\nAfter changes: run `sker check` and resolve deterministic BLOCK findings.\n{END}\n"
    );
    let agents_md = repo.join("AGENTS.md");
    let prior_agents = fs::read_to_string(&agents_md).unwrap_or_default();
    let updated_agents = replace_block(&prior_agents, &body);
    if updated_agents != prior_agents {
        fs::write(agents_md, updated_agents)?;
    }

    // Configure integrations for each enabled agent. The implicit `universal`
    // fallback only gets the AGENTS.md block above — never host-specific files.
    for agent_str in &enabled_agents {
        if agent_str == "universal" && explicit_agents.is_empty() {
            continue;
        }
        let agent_cfg = AgentConfig::Preset(agent_str.clone());
        install_agent_integration(&repo, &agent_cfg)?;
    }

    // Idempotently append Sker rebuildable caches to .gitignore
    update_gitignore(&repo)?;

    // Automatically perform initial compilation to create bundle.rkyv and projections
    super::compile::run(&repo, None, None, false)?;

    // Optional lightweight git pre-commit hook
    if git_hooks {
        install_git_hooks(&repo)?;
    }

    println!(
        "initialized: languages={}, agents={}",
        languages.join(","),
        enabled_agents.join(",")
    );

    Ok(())
}

pub fn install_agent_integration(repo: &Path, agent: &AgentConfig) -> Result<()> {
    let name = agent.name().to_lowercase();
    let rules_rel = agent.rules_file();
    let body = format!(
        "{START}\n1. Read the matched rule and its scope.\n2. Apply the rule, then run its positive and negative fixtures.\nBefore changing code: run `sker consult` for the relevant paths.\nAfter changes: run `sker check` and resolve deterministic BLOCK findings.\n{END}\n"
    );

    // 1. Rules file managed block
    let rules_path = repo.join(&rules_rel);
    if let Some(parent) = rules_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let prior = fs::read_to_string(&rules_path).unwrap_or_default();
    let updated = replace_block(&prior, &body);
    if updated != prior {
        fs::write(rules_path, updated)?;
    }

    // 2. MCP configuration file
    if let Some(mcp_rel) = agent.mcp_file() {
        let mcp_path = repo.join(&mcp_rel);
        if let Some(parent) = mcp_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if mcp_rel.ends_with(".toml") {
            install_codex_mcp(&mcp_path)?;
        } else if mcp_rel == ".claude/settings.json" {
            install_claude_settings(&mcp_path)?;
        } else {
            install_json_mcp(&mcp_path)?;
        }
    }

    // Additional agent-specific behaviors
    if name == "antigravity" {
        install_antigravity_home_config()?;
    }

    Ok(())
}

pub fn replace_block(old: &str, block: &str) -> String {
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

pub fn install_json_mcp(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut config: serde_json::Value = if path.exists() {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        serde_json::json!({})
    };
    let root = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} must contain a JSON object", path.display()))?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("mcpServers in {} must be an object", path.display()))?;
    servers.insert(
        "sker".into(),
        serde_json::json!({"command":"sker","args":["mcp"]}),
    );
    fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}

fn install_claude_settings(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut config: serde_json::Value = if path.exists() {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        serde_json::json!({})
    };
    let root = config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Claude settings must contain a JSON object"))?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(servers) = servers.as_object_mut() {
        servers.insert(
            "sker".into(),
            serde_json::json!({"command":"sker","args":["mcp"]}),
        );
    }
    let hooks = root.entry("hooks").or_insert_with(|| serde_json::json!({}));
    if let Some(hooks) = hooks.as_object_mut() {
        hooks.insert(
            "PreToolUse".into(),
            serde_json::json!([{"matcher":"","hooks":[{"type":"command","command":"sker hook"}]}]),
        );
    }
    fs::write(path, serde_json::to_vec_pretty(&config)?)?;
    Ok(())
}

fn install_codex_mcp(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let old = fs::read_to_string(path).unwrap_or_default();
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

fn install_antigravity_home_config() -> Result<()> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if let Some(home) = home {
        let user_cfg = home.join(".gemini/antigravity/mcp_config.json");
        if user_cfg.exists() {
            let content = fs::read(&user_cfg)?;
            if let Ok(mut json) = serde_json::from_slice::<serde_json::Value>(&content)
                && let Some(root) = json.as_object_mut()
            {
                let servers = root
                    .entry("mcpServers")
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(servers) = servers.as_object_mut() {
                    servers.insert(
                        "sker".into(),
                        serde_json::json!({"command": "sker", "args": ["mcp"]}),
                    );
                    fs::write(&user_cfg, serde_json::to_vec_pretty(&json)?)?;
                }
            }
        }
    }
    Ok(())
}

pub fn update_gitignore(repo: &Path) -> Result<()> {
    let gitignore = repo.join(".gitignore");
    let content = fs::read_to_string(&gitignore).unwrap_or_default();
    // `.sker/credential.json` is deliberately NOT ignored: Agent PRs must
    // commit it so CI can verify the attestation.
    let entries = [
        ".sker/bundle.*",
        ".sker/cache/",
        ".sker/tmp/",
        ".sker/check.json",
        ".sker/report.json",
        "*.sarif",
        "sker-report.json",
        "sker-check-report.json",
    ];
    let mut missing = Vec::new();
    for entry in entries {
        if !content.lines().any(|l| l.trim() == entry) {
            missing.push(entry);
        }
    }
    if !missing.is_empty() {
        let mut new_content = content;
        if !new_content.is_empty() && !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str("\n# Sker cache & reports\n");
        for entry in missing {
            new_content.push_str(entry);
            new_content.push('\n');
        }
        fs::write(&gitignore, new_content)?;
    }
    Ok(())
}

pub fn install_git_hooks(repo: &Path) -> Result<()> {
    let hooks_dir = repo.join(".git/hooks");
    fs::create_dir_all(&hooks_dir)?;
    let pre_commit = hooks_dir.join("pre-commit");
    let script = r#"#!/bin/sh
# Sker pre-commit guard
if command -v sker >/dev/null 2>&1; then
    sker check
fi
"#;
    fs::write(&pre_commit, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&pre_commit, fs::Permissions::from_mode(0o755));
    }
    println!("installed git hook: .git/hooks/pre-commit");
    Ok(())
}
