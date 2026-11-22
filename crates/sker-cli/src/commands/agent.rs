use super::find_repo;
use anyhow::{Result, bail};
use sker_schema::{AGENT_PRESETS, AgentConfig, CustomAgentConfig, find_agent_preset};
use std::{fs, path::Path};

pub fn list(path: &Path) -> Result<()> {
    let repo = find_repo(path)?;
    let configured_agents = get_configured_agents(&repo);

    println!("Agents / Integrations for {}:", repo.display());
    for preset in AGENT_PRESETS {
        let is_enabled = configured_agents
            .iter()
            .any(|a| a.name().eq_ignore_ascii_case(preset.name));
        let status = if is_enabled {
            "[enabled] "
        } else {
            "[disabled]"
        };
        println!("  {:12} {} {}", preset.name, status, preset.display_name);
    }

    for agent in &configured_agents {
        if let AgentConfig::Custom(c) = agent {
            println!(
                "  {:12} [enabled] Custom (rules: {}, mcp: {})",
                c.name,
                c.rules_file,
                c.mcp_file.as_deref().unwrap_or("none")
            );
        }
    }
    Ok(())
}

pub fn add(
    path: &Path,
    name: &str,
    custom_name: Option<&str>,
    rules_file: Option<&str>,
    mcp_file: Option<&str>,
) -> Result<()> {
    let repo = find_repo(path)?;
    let mut config = read_or_create_config(&repo)?;

    let new_agent = if name.eq_ignore_ascii_case("custom") {
        let actual_name = custom_name.ok_or_else(|| {
            anyhow::anyhow!("--name is required when adding custom agent integration")
        })?;
        let rf = rules_file.ok_or_else(|| {
            anyhow::anyhow!("--rules-file is required when adding custom agent integration")
        })?;
        AgentConfig::Custom(CustomAgentConfig {
            name: actual_name.to_string(),
            rules_file: rf.to_string(),
            mcp_file: mcp_file.map(str::to_string),
        })
    } else if let Some(preset) = find_agent_preset(name) {
        AgentConfig::Preset(preset.name.to_string())
    } else {
        bail!(
            "unknown agent preset '{}'. Run 'sker agent list' to see available presets or use 'custom'",
            name
        );
    };

    let agent_name = new_agent.name().to_string();
    let already_exists = config
        .iter()
        .any(|a| a.name().eq_ignore_ascii_case(&agent_name));

    if !already_exists {
        config.push(new_agent.clone());
        save_agents_to_config(&repo, &config)?;
    }

    // Configure managed block and MCP file
    crate::commands::init::install_agent_integration(&repo, &new_agent)?;

    // Regenerate projections
    super::compile::run(&repo, None, None, false)?;

    println!("added agent integration: {agent_name}");
    Ok(())
}

pub fn remove(path: &Path, name: &str) -> Result<()> {
    let repo = find_repo(path)?;
    let mut config = read_or_create_config(&repo)?;
    let initial_len = config.len();
    config.retain(|a| !a.name().eq_ignore_ascii_case(name));

    if config.len() == initial_len {
        println!("agent integration '{name}' not found in configuration");
        return Ok(());
    }

    save_agents_to_config(&repo, &config)?;
    println!("removed agent integration: {name}");
    Ok(())
}

pub fn mcp_config() {
    println!(
        "{{\n  \"mcpServers\": {{\n    \"sker\": {{ \"command\": \"sker\", \"args\": [\"mcp\"] }}\n  }}\n}}"
    );
}

pub fn get_configured_agents(repo: &Path) -> Vec<AgentConfig> {
    let cfg_path = repo.join(".sker.yaml");
    if !cfg_path.exists() {
        return vec![AgentConfig::Preset("universal".into())];
    }
    let Ok(bytes) = fs::read(&cfg_path) else {
        return vec![AgentConfig::Preset("universal".into())];
    };
    let Ok(val) = serde_yaml_ng::from_slice::<serde_yaml_ng::Value>(&bytes) else {
        return vec![AgentConfig::Preset("universal".into())];
    };
    let agents_seq = val
        .get("agents")
        .or_else(|| val.get("integrations"))
        .or_else(|| val.get("hosts"))
        .and_then(|v| v.as_sequence());

    let Some(seq) = agents_seq else {
        return vec![AgentConfig::Preset("universal".into())];
    };

    let mut out = Vec::new();
    for item in seq {
        if let Ok(agent) = serde_yaml_ng::from_value::<AgentConfig>(item.clone()) {
            out.push(agent);
        }
    }
    if out.is_empty() {
        out.push(AgentConfig::Preset("universal".into()));
    }
    out
}

fn read_or_create_config(repo: &Path) -> Result<Vec<AgentConfig>> {
    Ok(get_configured_agents(repo))
}

fn save_agents_to_config(repo: &Path, agents: &[AgentConfig]) -> Result<()> {
    let cfg_path = repo.join(".sker.yaml");
    let mut val: serde_yaml_ng::Value = if cfg_path.exists() {
        let bytes = fs::read(&cfg_path)?;
        serde_yaml_ng::from_slice(&bytes)
            .unwrap_or_else(|_| serde_yaml_ng::to_value(serde_json::json!({})).unwrap())
    } else {
        serde_yaml_ng::to_value(serde_json::json!({
            "schema_version": "sker.config/v1",
            "languages": [],
            "rules": []
        }))?
    };

    if let Some(map) = val.as_mapping_mut() {
        let agents_val = serde_yaml_ng::to_value(agents)?;
        map.insert(serde_yaml_ng::Value::String("agents".into()), agents_val);
        // Remove legacy hosts key if present to migrate
        map.remove(serde_yaml_ng::Value::String("hosts".into()));
    }

    fs::write(&cfg_path, serde_yaml_ng::to_string(&val)?)?;
    Ok(())
}
