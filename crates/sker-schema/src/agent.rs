use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentConfig {
    Preset(String),
    Custom(CustomAgentConfig),
}

impl AgentConfig {
    pub fn name(&self) -> &str {
        match self {
            AgentConfig::Preset(name) => name.as_str(),
            AgentConfig::Custom(c) => c.name.as_str(),
        }
    }

    pub fn rules_file(&self) -> String {
        match self {
            AgentConfig::Preset(name) => find_agent_preset(name)
                .map(|p| p.rules_file.to_string())
                .unwrap_or_else(|| "AGENTS.md".to_string()),
            AgentConfig::Custom(c) => c.rules_file.clone(),
        }
    }

    pub fn mcp_file(&self) -> Option<String> {
        match self {
            AgentConfig::Preset(name) => {
                find_agent_preset(name).and_then(|p| p.mcp_file.map(str::to_string))
            }
            AgentConfig::Custom(c) => c.mcp_file.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomAgentConfig {
    pub name: String,
    pub rules_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPreset {
    pub name: &'static str,
    pub display_name: &'static str,
    pub rules_file: &'static str,
    pub mcp_file: Option<&'static str>,
    pub detector_dirs: &'static [&'static str],
}

pub const AGENT_PRESETS: &[AgentPreset] = &[
    AgentPreset {
        name: "universal",
        display_name: "Universal (AGENTS.md + .mcp.json)",
        rules_file: "AGENTS.md",
        mcp_file: Some(".mcp.json"),
        detector_dirs: &["AGENTS.md"],
    },
    AgentPreset {
        name: "claude",
        display_name: "Claude Code / Claude Desktop",
        rules_file: "CLAUDE.md",
        mcp_file: Some(".claude/settings.json"),
        detector_dirs: &[".claude", "CLAUDE.md"],
    },
    AgentPreset {
        name: "codex",
        display_name: "Codex CLI",
        rules_file: "AGENTS.md",
        mcp_file: Some(".codex/config.toml"),
        detector_dirs: &[".codex"],
    },
    AgentPreset {
        name: "antigravity",
        display_name: "Google Antigravity",
        rules_file: ".agent/rules/sker.md",
        mcp_file: Some(".mcp.json"),
        detector_dirs: &[".agent", ".agents"],
    },
    AgentPreset {
        name: "cursor",
        display_name: "Cursor IDE",
        rules_file: ".cursorrules",
        mcp_file: Some(".cursor/mcp.json"),
        detector_dirs: &[".cursor", ".cursorrules"],
    },
    AgentPreset {
        name: "codebuddy",
        display_name: "Tencent CodeBuddy",
        rules_file: ".codebuddy/rules/sker.md",
        mcp_file: Some(".codebuddy/mcp.json"),
        detector_dirs: &[".codebuddy"],
    },
    AgentPreset {
        name: "windsurf",
        display_name: "Codeium Windsurf",
        rules_file: ".windsurfrules",
        mcp_file: Some(".codeium/windsurf/mcp_config.json"),
        detector_dirs: &[".windsurf", ".codeium", ".windsurfrules"],
    },
    AgentPreset {
        name: "opencode",
        display_name: "OpenCode Interpreter",
        rules_file: "AGENTS.md",
        mcp_file: Some("opencode.json"),
        detector_dirs: &[".opencode", "opencode.json"],
    },
];

pub fn find_agent_preset(name: &str) -> Option<&'static AgentPreset> {
    AGENT_PRESETS
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}
