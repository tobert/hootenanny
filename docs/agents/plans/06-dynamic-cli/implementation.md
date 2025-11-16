# Dynamic CLI - Technical Implementation

## 🏗️ Module Structure

```
crates/hrcli/src/
├── main.rs                # Entry point with dynamic/static fallback
├── discovery/
│   ├── mod.rs             # Discovery module interface
│   ├── cache.rs           # Schema caching with TTL
│   ├── schema.rs          # Extended tool schema types
│   └── client.rs          # MCP client for discovery
├── builder/
│   ├── mod.rs             # CLI building interface
│   ├── command.rs         # Command generation from schemas
│   ├── parameters.rs      # Parameter type mapping
│   └── help.rs            # Help text generation
├── execution/
│   ├── mod.rs             # Execution engine interface
│   ├── transformer.rs     # CLI args → MCP requests
│   ├── formatter.rs       # Response formatting
│   └── errors.rs          # Error handling and messages
└── shell/
    ├── completions.rs     # Shell completion generation
    └── env.rs             # Environment variable handling
```

## 📦 Core Components

### 1. Discovery Module

```rust
// crates/hrcli/src/discovery/schema.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Extended tool schema with musical context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Tool name (e.g., "play", "fork_branch")
    pub name: String,

    /// Brief description for --help
    pub description: String,

    /// Extended help for humans
    pub human_context: String,

    /// Extended help for AI agents
    pub ai_context: String,

    /// When to use this tool
    pub usage_context: String,

    /// Emotional implications
    pub emotional_hint: String,

    /// Parameters with rich metadata
    pub parameters: Vec<Parameter>,

    /// Example invocations
    pub examples: Vec<Example>,

    /// Related tools
    pub see_also: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub description: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default: Option<serde_json::Value>,
    pub env_var: Option<String>,
    pub musical_meaning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParamType {
    /// Simple string
    String {
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },

    /// Numeric value with range
    Number {
        min: Option<f64>,
        max: Option<f64>,
    },

    /// Musical note or chord
    Musical {
        kind: MusicalKind,
    },

    /// Emotional vector (special handling)
    EmotionalVector,

    /// Complex JSON object
    Object {
        schema: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MusicalKind {
    Note,       // Single note
    Chord,      // Chord symbol
    Scale,      // Scale pattern
    Rhythm,     // Rhythm pattern
}
```

### 2. Discovery Client

```rust
// crates/hrcli/src/discovery/client.rs

use anyhow::{Context, Result};
use std::time::Duration;

pub struct DiscoveryClient {
    server_url: String,
    timeout: Duration,
}

impl DiscoveryClient {
    pub async fn discover_tools(&self) -> Result<Vec<ToolSchema>> {
        // Connect with timeout
        let client = McpClient::connect(&self.server_url)
            .timeout(self.timeout)
            .await
            .context("Cannot connect to musical consciousness")?;

        // Initialize MCP session
        client.initialize().await?;

        // Get base tool list
        let tools = client.list_tools().await?;

        // Try to get extended schemas (if server supports it)
        let extended = if client.supports_extended_schemas().await? {
            client.list_tools_extended().await?
        } else {
            // Fallback: construct from basic info
            self.construct_schemas_from_basic(tools)?
        };

        Ok(extended)
    }

    fn construct_schemas_from_basic(&self, tools: Vec<BasicTool>) -> Result<Vec<ToolSchema>> {
        tools.into_iter()
            .map(|tool| {
                // Infer musical context from tool name and parameters
                let schema = ToolSchema {
                    name: tool.name.clone(),
                    description: tool.description,
                    human_context: self.infer_human_context(&tool),
                    ai_context: self.infer_ai_context(&tool),
                    usage_context: self.infer_usage(&tool),
                    emotional_hint: self.infer_emotional(&tool),
                    parameters: self.map_parameters(tool.parameters),
                    examples: self.generate_examples(&tool),
                    see_also: self.find_related(&tool.name),
                };
                Ok(schema)
            })
            .collect()
    }
}
```

### 3. Cache System

```rust
// crates/hrcli/src/discovery/cache.rs

use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use anyhow::{Context, Result};

pub struct SchemaCache {
    cache_dir: PathBuf,
    ttl: Duration,
}

impl SchemaCache {
    pub fn new() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .context("Cannot find cache directory")?
            .join("hrcli");

        std::fs::create_dir_all(&cache_dir)?;

        Ok(Self {
            cache_dir,
            ttl: Duration::from_secs(300), // 5 minutes default
        })
    }

    pub fn load(&self) -> Result<Option<CachedSchemas>> {
        let cache_file = self.cache_dir.join("tools.json");

        if !cache_file.exists() {
            return Ok(None);
        }

        let metadata = std::fs::metadata(&cache_file)?;
        let age = SystemTime::now().duration_since(metadata.modified()?)?;

        if age > self.ttl {
            // Cache expired
            return Ok(None);
        }

        let content = std::fs::read_to_string(&cache_file)?;
        let schemas: CachedSchemas = serde_json::from_str(&content)?;

        Ok(Some(schemas))
    }

    pub fn save(&self, schemas: &[ToolSchema]) -> Result<()> {
        let cache_file = self.cache_dir.join("tools.json");

        let cached = CachedSchemas {
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: SystemTime::now(),
            server_url: self.last_server_url.clone(),
            schemas: schemas.to_vec(),
        };

        let content = serde_json::to_string_pretty(&cached)?;
        std::fs::write(&cache_file, content)?;

        Ok(())
    }
}
```

### 4. CLI Builder

```rust
// crates/hrcli/src/builder/command.rs

use clap::{Command, Arg, ArgAction, value_parser};
use crate::discovery::schema::{ToolSchema, Parameter, ParamType};

pub struct DynamicCliBuilder {
    schemas: Vec<ToolSchema>,
}

impl DynamicCliBuilder {
    pub fn build(self) -> Command {
        let mut app = Command::new("hrcli")
            .version(env!("CARGO_PKG_VERSION"))
            .author("Amy Tobey & AI Ensemble")
            .about("Dynamic Musical CLI - Bridge between thought and sound")
            .long_about(PHILOSOPHICAL_HELP)
            .arg(
                Arg::new("server")
                    .long("server")
                    .short('s')
                    .global(true)
                    .env("HRCLI_SERVER")
                    .default_value("http://127.0.0.1:8080")
                    .help("The musical consciousness to connect with")
            )
            .arg(
                Arg::new("offline")
                    .long("offline")
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help("Use cached schemas (no server connection)")
            )
            .arg(
                Arg::new("agent")
                    .long("agent-id")
                    .global(true)
                    .env("HRCLI_AGENT")
                    .default_value_os(std::env::var_os("USER"))
                    .help("Your identity in the musical conversation")
            );

        // Add each discovered tool as a subcommand
        for schema in self.schemas {
            app = app.subcommand(self.build_tool_command(schema));
        }

        // Add meta commands
        app = app
            .subcommand(self.build_cache_command())
            .subcommand(self.build_completions_command())
            .subcommand(self.build_repl_command());

        app
    }

    fn build_tool_command(&self, schema: ToolSchema) -> Command {
        let mut cmd = Command::new(&schema.name)
            .about(&schema.description)
            .long_about(self.format_long_help(&schema));

        // Add parameters as arguments
        for param in schema.parameters {
            cmd = self.add_parameter(cmd, param);
        }

        // Add examples to help
        if !schema.examples.is_empty() {
            let examples = schema.examples.iter()
                .map(|e| format!("  $ hrcli {} {}", schema.name, e.invocation))
                .collect::<Vec<_>>()
                .join("\n");

            cmd = cmd.after_help(format!("\nEXAMPLES:\n{}", examples));
        }

        cmd
    }

    fn add_parameter(&self, mut cmd: Command, param: Parameter) -> Command {
        match param.param_type {
            ParamType::EmotionalVector => {
                // Special handling: expand to three arguments
                cmd = cmd
                    .arg(Arg::new("valence")
                        .long("valence")
                        .value_parser(value_parser!(f32))
                        .allow_hyphen_values(true)
                        .default_value("0.0")
                        .env(param.env_var.as_ref().map(|e| format!("{}_VALENCE", e)))
                        .help("Joy-sorrow axis: -1.0 (melancholy) to 1.0 (euphoric)"))
                    .arg(Arg::new("arousal")
                        .long("arousal")
                        .value_parser(value_parser!(f32))
                        .default_value("0.5")
                        .env(param.env_var.as_ref().map(|e| format!("{}_AROUSAL", e)))
                        .help("Energy level: 0.0 (meditative) to 1.0 (ecstatic)"))
                    .arg(Arg::new("agency")
                        .long("agency")
                        .value_parser(value_parser!(f32))
                        .allow_hyphen_values(true)
                        .default_value("0.0")
                        .env(param.env_var.as_ref().map(|e| format!("{}_AGENCY", e)))
                        .help("Initiative: -1.0 (following) to 1.0 (leading)"));
            },

            ParamType::String { pattern } => {
                let mut arg = Arg::new(&param.name)
                    .long(&param.name.to_kebab_case())
                    .help(&param.description)
                    .value_parser(value_parser!(String))
                    .required(param.required);

                if let Some(env) = param.env_var {
                    arg = arg.env(env);
                }

                if let Some(default) = param.default {
                    arg = arg.default_value(default.as_str().unwrap_or(""));
                }

                cmd = cmd.arg(arg);
            },

            ParamType::Number { min, max } => {
                let mut arg = Arg::new(&param.name)
                    .long(&param.name.to_kebab_case())
                    .help(&param.description)
                    .value_parser(value_parser!(f64))
                    .allow_hyphen_values(true)
                    .required(param.required);

                if let Some(env) = param.env_var {
                    arg = arg.env(env);
                }

                // Add range validation
                if let (Some(min), Some(max)) = (min, max) {
                    arg = arg.value_parser(
                        value_parser!(f64)
                            .range(min..=max)
                    );
                }

                cmd = cmd.arg(arg);
            },

            ParamType::Musical { kind } => {
                let help = format!("{} ({})",
                    param.description,
                    match kind {
                        MusicalKind::Note => "e.g., C, D#, Eb",
                        MusicalKind::Chord => "e.g., Cmaj7, Dm7, G7",
                        MusicalKind::Scale => "e.g., C-major, A-minor",
                        MusicalKind::Rhythm => "e.g., 4/4, shuffle",
                    }
                );

                cmd = cmd.arg(
                    Arg::new(&param.name)
                        .long(&param.name.to_kebab_case())
                        .help(&help)
                        .value_parser(value_parser!(String))
                        .required(param.required)
                );
            },

            ParamType::Object { .. } => {
                // Complex types need JSON
                cmd = cmd.arg(
                    Arg::new(&param.name)
                        .long(&param.name.to_kebab_case())
                        .help(&format!("{} (JSON object)", param.description))
                        .value_parser(value_parser!(String))
                        .required(param.required)
                );
            },
        }

        cmd
    }

    fn format_long_help(&self, schema: &ToolSchema) -> String {
        format!(
            r#"{}

WHEN TO USE:
{}

FOR HUMANS:
{}

FOR AI AGENTS:
{}

EMOTIONAL CONTEXT:
{}

SEE ALSO:
  {}"#,
            schema.description,
            schema.usage_context,
            schema.human_context,
            schema.ai_context,
            schema.emotional_hint,
            schema.see_also.join(", ")
        )
    }
}

const PHILOSOPHICAL_HELP: &str = r#"
hrcli - The Musical Conversation Interface

This CLI bridges the gap between intention and sound, allowing both
humans and language models to participate in musical conversations.
Each command represents a musical action in the conversation tree,
where abstract emotions become concrete sounds.

FOR LANGUAGE MODELS:
  You can use this CLI to express musical ideas by mapping your
  intentions to the three-dimensional emotional space (valence,
  arousal, agency). The server will realize your abstract desires
  as concrete musical events.

FOR HUMANS:
  Write shell scripts that compose music, create generative pieces,
  or interact with AI musicians in real-time.

The conversation tree preserves all musical utterances, allowing
for branching explorations and merged conclusions.

ENVIRONMENT VARIABLES:
  HRCLI_SERVER         MCP server URL (default: http://127.0.0.1:8080)
  HRCLI_AGENT          Your agent ID (default: $USER)
  HRCLI_CACHE_TTL      Cache time-to-live in seconds (default: 300)
  HRCLI_DEFAULT_*      Default values for emotional parameters

EXAMPLES:
  # List available tools
  hrcli list-tools

  # Play a note with emotion
  hrcli play --what C --how softly --valence 0.5 --arousal 0.3

  # Fork the conversation
  hrcli fork_branch --name "jazz-exploration" --reason "Trying seventh chords"

  # Generate shell completions
  hrcli completions bash > ~/.local/share/bash-completion/completions/hrcli
"#;
```

### 5. Execution Engine

```rust
// crates/hrcli/src/execution/transformer.rs

use clap::ArgMatches;
use serde_json::{json, Value};
use anyhow::Result;

pub struct ArgumentTransformer {
    schema: ToolSchema,
}

impl ArgumentTransformer {
    /// Transform CLI arguments to MCP request parameters
    pub fn transform(&self, matches: &ArgMatches) -> Result<Value> {
        let mut params = json!({});

        for param in &self.schema.parameters {
            match param.param_type {
                ParamType::EmotionalVector => {
                    // Collect three separate arguments into one object
                    if matches.contains_id("valence") {
                        params[&param.name] = json!({
                            "valence": matches.get_one::<f32>("valence").copied().unwrap_or(0.0),
                            "arousal": matches.get_one::<f32>("arousal").copied().unwrap_or(0.5),
                            "agency": matches.get_one::<f32>("agency").copied().unwrap_or(0.0),
                        });
                    }
                },

                _ => {
                    // Standard parameter handling
                    if let Some(value) = matches.get_one::<String>(&param.name) {
                        params[&param.name] = match param.param_type {
                            ParamType::Number { .. } => {
                                json!(value.parse::<f64>()?)
                            },
                            ParamType::Object { .. } => {
                                serde_json::from_str(value)?
                            },
                            _ => json!(value),
                        };
                    } else if param.required {
                        anyhow::bail!("Missing required parameter: {}", param.name);
                    }
                }
            }
        }

        // Add global agent ID if present
        if let Some(agent) = matches.get_one::<String>("agent") {
            params["agent_id"] = json!(agent);
        }

        Ok(params)
    }
}
```

### 6. Response Formatter

```rust
// crates/hrcli/src/execution/formatter.rs

use colored::*;
use serde_json::Value;

pub struct ResponseFormatter;

impl ResponseFormatter {
    pub fn format(&self, tool_name: &str, response: Value) -> String {
        match tool_name {
            "play" => self.format_play_response(response),
            "fork_branch" => self.format_fork_response(response),
            "evaluate_branch" => self.format_evaluation(response),
            _ => self.format_generic(response),
        }
    }

    fn format_play_response(&self, response: Value) -> String {
        let node_id = response["node_id"].as_u64().unwrap_or(0);
        let what = response["what"].as_str().unwrap_or("?");
        let how = response["how"].as_str().unwrap_or("?");
        let valence = response["emotion"]["valence"].as_f64().unwrap_or(0.0);
        let arousal = response["emotion"]["arousal"].as_f64().unwrap_or(0.5);
        let agency = response["emotion"]["agency"].as_f64().unwrap_or(0.0);

        let emotion_color = self.emotion_to_color(valence, arousal);

        format!(
            "\n{} {}\n{}\n  {} #{} on branch '{}'\n  {} {} ({})\n  {} valence={:.2}, arousal={:.2}, agency={:.2}\n\n  {}:\n    {}\n\n  {}:\n    • {}\n    • {}\n    • {}\n{}\n",
            "🎵".bright_cyan(),
            "Musical Event Created".bright_white().bold(),
            "━".repeat(50).bright_black(),
            "Node:".bright_black(),
            node_id.to_string().bright_yellow(),
            response["branch"].as_str().unwrap_or("main").bright_green(),
            "Content:".bright_black(),
            what.color(emotion_color).bold(),
            how.italic(),
            "Emotion:".bright_black(),
            valence, arousal, agency,
            "Musical Interpretation".bright_black(),
            response["interpretation"].as_str().unwrap_or("...").white(),
            "Suggested Responses".bright_black(),
            response["suggestions"][0].as_str().unwrap_or("...").bright_black(),
            response["suggestions"][1].as_str().unwrap_or("...").bright_black(),
            response["suggestions"][2].as_str().unwrap_or("...").bright_black(),
            "━".repeat(50).bright_black(),
        )
    }

    fn emotion_to_color(&self, valence: f64, arousal: f64) -> Color {
        match (valence > 0.0, arousal > 0.5) {
            (true, true) => Color::BrightYellow,   // Happy + energetic
            (true, false) => Color::Green,          // Happy + calm
            (false, true) => Color::BrightRed,      // Sad + energetic
            (false, false) => Color::Blue,          // Sad + calm
        }
    }
}
```

### 7. Shell Completion Generator

```rust
// crates/hrcli/src/shell/completions.rs

use clap_complete::{generate, Shell};
use std::io;

pub fn generate_completions(app: &mut Command, shell: Shell) {
    generate(shell, app, "hrcli", &mut io::stdout());
}
```

## 🚀 Performance Optimizations

### Lazy Discovery
- Only connect to server when needed
- Cache schemas aggressively
- Timeout quickly (2s) and fall back to cache

### Parallel Tool Discovery
```rust
// Discover tool details in parallel
let tools = futures::future::join_all(
    basic_tools.iter().map(|tool| {
        client.get_tool_details(&tool.name)
    })
).await?;
```

### Smart Caching
- Cache per server URL
- Version-aware (invalidate on CLI version change)
- Background refresh when cache is stale but usable

## 🧪 Testing Strategy

### Unit Tests
- Parameter transformation
- Cache TTL logic
- Help text generation
- Error formatting

### Integration Tests
- Full discovery flow
- Cache persistence
- Shell completion generation
- Offline mode

### End-to-End Tests
```bash
#!/bin/bash
# test_dynamic_cli.sh

# Test discovery
hrcli --server http://test-server:8080 list-tools

# Test caching
hrcli cache refresh
hrcli --offline list-tools

# Test all discovered tools
for tool in $(hrcli list-tools --json | jq -r '.tools[].name'); do
    hrcli $tool --help
done

# Test shell script
./examples/blues_jam.sh
```

## 🎯 Success Metrics

- **Discovery time**: <100ms with cache, <2s without
- **Cache hit rate**: >90% in normal usage
- **Help text quality**: Clear for both audiences
- **Shell completion**: Works in bash, zsh, fish
- **Error messages**: Actionable for both humans and AI

---

This implementation creates a truly dynamic CLI that adapts to whatever the MCP server provides, while maintaining excellent UX for both human and AI users.