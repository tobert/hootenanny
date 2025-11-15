# Implementation Plan: halfremembered-mcp

**Philosophy:** Personal tools that fail loud and clear. No surprises, no silent failures.

## Project Structure Status

✅ **Already completed**:
- `docs/agents/` - Agent memory system (NOW.md, PATTERNS.md, CONTEXT.md)
- `docs/BOTS.md` - Development guidelines and workflows
- `CLAUDE.md` → `docs/BOTS.md` (symlink)
- `GEMINI.md` → `docs/BOTS.md` (symlink)

---

## Execution Plan for Claude Code

### Prompt 1: Initialize Project

```
Initialize the Rust project structure in the current directory.

NOTE: We're already inside the halfremembered-mcp directory, so don't create a new one.

1. Initialize as a Rust project:
   cargo init --name halfremembered_mcp

2. Initialize jj repository (colocated with git):
   jj git init --colocate

3. Create GitHub repo using gh:
   gh repo create halfremembered-mcp --private --source=. --push

4. Set up the directory structure:
   mkdir -p src/tools src/llm examples

5. Create these files (empty for now):
   - src/tools/mod.rs
   - src/tools/deepseek.rs
   - src/llm/mod.rs
   - src/llm/ollama.rs
   - test-scenarios.md
   - examples/usage.md

6. Update Cargo.toml with these dependencies:

[package]
name = "halfremembered_mcp"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "halfremembered_mcp"
path = "src/main.rs"

[dependencies]
rmcp = { git = "https://github.com/modelcontextprotocol/rust-sdk", features = ["server", "transport-ws", "macros"] }
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
schemars = "0.8"

# For Lua scripting
mlua = { version = "0.9", features = ["lua54", "async", "macros"] }

# For persistent state
sled = "0.34"

# For dynamic tool loading
walkdir = "2"

# For hot-reloading
notify = "6.1"
futures-util = "0.3"

7. Create a basic README.md:

# halfremembered-mcp

Personal MCP server for local LLM tools (DeepSeek, music generation, etc.)

## Tools

### DeepSeek Tools
- `deepseek_review_code` - Code review using DeepSeek Coder 33B
- `deepseek_explain_code` - Code explanation and analysis

### Music Tools (Coming Soon)
- MIDI generation and manipulation

## Setup

```bash
# Install DeepSeek Coder
ollama pull deepseek-coder:33b

# Build the MCP server
cargo build --release

# Add to Claude Code
claude mcp add halfremembered --scope local -- \
  $PWD/target/release/halfremembered_mcp
```

## Development

Uses jj (Jujutsu) for version control. See docs/BOTS.md for workflows.

## Documentation

- `docs/BOTS.md` - Development guidelines, jj workflows, agent memory system
- `docs/agents/` - Shared memory for multi-model collaboration
- `docs/agents/plans/` - Implementation plans and task breakdowns

8. Commit the initial structure:
   jj describe -m "feat: initial project structure - MCP server foundation

Why: Setting up halfremembered-mcp for local LLM tools
Approach: Cargo init with rmcp SDK, docs structure in place
Learned: Project already had docs/agents from template
Next: Implement Ollama client for LLM execution

🤖 Claude <claude@anthropic.com>"

   jj git push -c @
```

---

### Prompt 2: Build Ollama Client

```
Implement the Ollama client in src/llm/ollama.rs.

Requirements:
- Use tokio::process::Command for async execution with timeout support
- Fail fast with clear error messages
- Include debug logging via tracing
- 60 second timeout (33B model needs more time)
- Never fail silently

Here's the interface:

```rust
use anyhow::{Context, Result, bail};
use std::time::Duration;
use tokio::process::Command;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error};

pub struct OllamaClient {
    timeout: Duration,
}

impl OllamaClient {
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(60),
        }
    }

    /// Check if a model is available locally
    pub async fn is_model_available(&self, model: &str) -> Result<bool> {
        debug!("Checking if model '{}' is available", model);

        // Run: ollama list
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            Command::new("ollama").arg("list").output()
        )
        .await
        .context("Timeout checking ollama models")?
        .context("Failed to execute 'ollama list'. Is ollama installed?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("Cannot connect to ollama. Run: ollama serve\nError: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains(model))
    }

    /// Run a completion with the specified model
    pub async fn completion(&self, model: &str, prompt: &str) -> Result<String> {
        debug!("Running completion with model: {}", model);
        debug!("Prompt length: {} chars", prompt.len());

        // First check if model is available
        if !self.is_model_available(model).await? {
            bail!("Model '{}' not found. Run: ollama pull {}", model, model);
        }

        // Run: ollama run <model>
        let mut child = Command::new("ollama")
            .arg("run")
            .arg(model)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to spawn ollama process")?;

        // Send prompt via stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await
                .context("Failed to write prompt to ollama")?;
            drop(stdin); // Close stdin to signal end of input
        }

        // Wait for completion with timeout
        let output = tokio::time::timeout(self.timeout, child.wait_with_output())
            .await
            .context(format!("Completion timed out after {}s", self.timeout.as_secs()))?
            .context("Failed to read ollama output")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Ollama execution failed: {}", stderr);
            bail!("Ollama failed: {}", stderr);
        }

        let response = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in ollama response")?;

        debug!("Response length: {} chars", response.len());
        Ok(response)
    }
}
```

Implementation notes:
- Using tokio::process::Command for async + timeout support
- For is_model_available: run `ollama list`, check if model name appears in output
- Use tokio::time::timeout for both operations
- Include stderr in error messages
- Use context() to add helpful error messages at each failure point
- Add error! logs before returning errors

Test cases handled:
1. Ollama not installed → "Failed to execute 'ollama list'. Is ollama installed?"
2. Ollama not running → "Cannot connect to ollama. Run: ollama serve"
3. Model not downloaded → "Model 'X' not found. Run: ollama pull X"
4. Timeout → "Completion timed out after 60s"

After implementing, update src/llm/mod.rs to export OllamaClient:

```rust
pub mod ollama;
```

Commit with:
jj describe -m "feat: add Ollama client - async execution with timeout

Why: Need to call local LLM models via Ollama
Approach: tokio::process::Command with 60s timeout, fail-fast errors
Learned: AsyncWriteExt needed for stdin.write_all
Next: Implement DeepSeek code review tools

🤖 Claude <claude@anthropic.com>"

jj git push -c @
```

---

### Prompt 3: Implement DeepSeek Code Review Tool

```
Implement the code review tool in src/tools/deepseek.rs.

Use the rmcp #[tool] and #[tool_box] macros to create an MCP tool.

```rust
use anyhow::Result;
use rmcp::{tool, tool_box};
use tracing::{debug, info};

use crate::llm::ollama::OllamaClient;

#[derive(Clone)]
pub struct DeepSeekTools {
    ollama: OllamaClient,
}

impl DeepSeekTools {
    pub fn new() -> Self {
        Self {
            ollama: OllamaClient::new(),
        }
    }
}

#[tool_box]
impl DeepSeekTools {
    /// Review code using DeepSeek Coder 33B for bugs, performance, and style issues
    #[tool(description = "Get a detailed code review from DeepSeek Coder 33B. \
                          Returns analysis of bugs, performance issues, and style suggestions.")]
    async fn deepseek_review_code(
        &self,
        #[tool(description = "The code to review")] code: String,
        #[tool(description = "Optional context about what this code does")]
        context: Option<String>,
    ) -> Result<String> {
        info!("Running code review, code length: {} chars", code.len());

        let mut prompt = String::from("You are an expert code reviewer. \
                                       Review the following code for:\n\
                                       1. Potential bugs or errors\n\
                                       2. Performance issues\n\
                                       3. Style and readability improvements\n\
                                       4. Security concerns\n\n");

        if let Some(ctx) = context {
            prompt.push_str(&format!("Context: {}\n\n", ctx));
        }

        prompt.push_str("Code to review:\n```\n");
        prompt.push_str(&code);
        prompt.push_str("\n```\n\nProvide a detailed review:");

        debug!("Sending prompt to DeepSeek Coder 33B");
        let response = self.ollama.completion("deepseek-coder:33b", &prompt).await?;

        info!("Review complete, response length: {} chars", response.len());
        Ok(response)
    }

    /// Explain code using DeepSeek Coder 33B with focus on algorithms, architecture, or bugs
    #[tool(description = "Get a detailed explanation of code from DeepSeek Coder 33B. \
                          Can focus on algorithms, architecture, or potential bugs.")]
    async fn deepseek_explain_code(
        &self,
        #[tool(description = "The code to explain")] code: String,
        #[tool(description = "Focus area: 'algorithm', 'architecture', 'bugs', or null for general")]
        focus: Option<String>,
    ) -> Result<String> {
        info!("Explaining code, length: {} chars, focus: {:?}", code.len(), focus);

        let focus_text = match focus.as_deref() {
            Some("algorithm") => "Focus on explaining the algorithm and computational complexity.",
            Some("architecture") => "Focus on the overall architecture and design patterns.",
            Some("bugs") => "Focus on potential bugs and edge cases.",
            _ => "Provide a comprehensive explanation.",
        };

        let prompt = format!(
            "You are an expert programmer. Explain the following code clearly.\n\
             {}\n\n\
             Code:\n```\n{}\n```\n\n\
             Provide a clear, detailed explanation:",
            focus_text, code
        );

        debug!("Sending prompt to DeepSeek Coder 33B");
        let response = self.ollama.completion("deepseek-coder:33b", &prompt).await?;

        info!("Explanation complete, response length: {} chars", response.len());
        Ok(response)
    }
}
```

Update src/tools/mod.rs to export DeepSeekTools:

```rust
pub mod deepseek;
```

Commit with:
jj describe -m "feat: add DeepSeek code review tools - review and explain

Why: Provide code review and explanation via MCP
Approach: Two tools using rmcp macros, async ollama calls
Learned: tool_box macro handles MCP registration automatically
Next: Implement main MCP server with stdio transport

🤖 Claude <claude@anthropic.com>"

jj git push -c @
```

---

### Prompt 4: Build MCP Server Main

```
Implement the MCP server in src/main.rs.

Requirements:
- Set up tracing with configurable log level (default: info for production)
- Use RUST_LOG environment variable for configuration
- Register DeepSeek tools
- Use WebSocket transport
- Fail fast on any setup errors

```rust
use anyhow::{Context, Result};
use rmcp::{ServerHandler, ServiceExt, model::{ServerCapabilities, ServerInfo}, transport::websocket};
use std::path::Path;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod llm;
mod tools;
// These modules will be created in the 01-lua plan
// mod state;
// mod lua;

use tools::deepseek::DeepSeekTools;
// use crate::state::StateManager;
// use crate::tools::lua_manager::LuaToolManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging - defaults to info, can override with RUST_LOG env var
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "halfremembered_mcp=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting halfremembered-mcp server");

    // == The following section is a placeholder for features from 01-lua ==
    // Initialize State Manager
    // let state_manager = StateManager::new("./halfremembered.db")?;

    // Initialize and load Lua tools
    // let lua_tool_manager = LuaToolManager::new();
    // let lua_tools_path = Path::new("mcp_lua/tools");
    // lua_tool_manager.load_tools(lua_tools_path)?;
    
    // Start the file watcher in the background
    // let watcher_manager = lua_tool_manager.clone();
    // tokio::spawn(async move {
    //     watcher_manager.watch_for_changes(lua_tools_path).await;
    // });
    // ======================================================================

    // Create server info
    let server_info = ServerInfo {
        name: "halfremembered-mcp".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // Create capabilities - we provide tools
    let capabilities = ServerCapabilities {
        tools: Some(Default::default()),
        ..Default::default()
    };

    info!("Registering DeepSeek tools");
    let deepseek_tools = DeepSeekTools::new();

    // Create server handler
    let handler = ServerHandler::new(server_info, capabilities);

    let addr = "127.0.0.1:8080";
    info!("Starting WebSocket transport on {}", addr);
    // Set up WebSocket transport and run server
    websocket::websocket_transport(handler, addr)
        .with_tools(deepseek_tools)
        .serve()
        .await
        .context(format!("Failed to run MCP server on {}", addr))?;

    Ok(())
}
```

Make sure all modules are properly connected:
- src/llm/mod.rs should have: pub mod ollama;
- src/tools/mod.rs should have: pub mod deepseek;

Build the project to verify everything compiles:
cargo build --release

If it builds successfully, commit:
jj describe -m "feat: complete MCP server - WebSocket transport with DeepSeek tools

Why: Enable multi-agent collaboration and shared state for ensemble work
Approach: rmcp ServerHandler with WebSocket transport (127.0.0.1:8080), tracing logging
Learned: WebSocket opens up exciting possibilities like driving VSTs over MCP
Next: Create test documentation and usage examples

🤖 Claude <claude@anthropic.com>"

jj git push -c @
```

---

### Prompt 5: Create Test Documentation

```
Create test-scenarios.md with scenarios for verifying the MCP server works.

Include:
1. How to test locally with MCP inspector
2. How to add to Claude Code
3. Test scenarios with expected behaviors
4. Troubleshooting section for common errors

Content:

# Testing halfremembered-mcp

## Prerequisites

```bash
# Install DeepSeek Coder 33B
ollama pull deepseek-coder:33b

# Verify ollama is running
ollama list
```

## Local Testing

### Option 1: MCP Inspector

```bash
# Build the server
cargo build --release

# Run inspector
npx @modelcontextprotocol/inspector \
  ./target/release/halfremembered_mcp
```

### Option 2: Direct Testing

```bash
# Run server with debug logging
RUST_LOG=debug cargo run

# In another terminal, you can manually test by sending JSON-RPC
# (Advanced - use inspector instead)
```

## Integration with Claude Code

```bash
# Add to Claude Code
claude mcp add halfremembered --scope local -- \
  $PWD/target/release/halfremembered_mcp

# Verify it was added
claude mcp list

# Restart Claude Code to pick up changes
```

## Test Scenarios

### Scenario 1: Simple Code Review

**Prompt for Claude:**
"Use the deepseek_review_code tool to review this Rust function:

```rust
fn divide(a: i32, b: i32) -> i32 {
    a / b
}
```
"

**Expected Result:**
- DeepSeek should identify the division by zero risk
- Should suggest using Option<i32> or Result<i32, Error>
- Response should be detailed and helpful

### Scenario 2: Explain Algorithm

**Prompt for Claude:**
"Use deepseek_explain_code with focus='algorithm' to explain this code:

```rust
fn bubble_sort(arr: &mut [i32]) {
    let len = arr.len();
    for i in 0..len {
        for j in 0..len - i - 1 {
            if arr[j] > arr[j + 1] {
                arr.swap(j, j + 1);
            }
        }
    }
}
```
"

**Expected Result:**
- Should explain bubble sort algorithm
- Should mention O(n²) time complexity
- Should explain the nested loop logic

### Scenario 3: Lua Code Review

**Prompt for Claude:**
"Review this Lua function:

```lua
function calculate(x, y)
    return x / y
end
```
"

**Expected Result:**
- DeepSeek should understand Lua syntax
- Should identify division by zero
- Should suggest adding error handling

### Scenario 4: Dynamic Lua Tool (`hello_world`)

**Setup:**
Ensure the `mcp_lua/tools/hello_world` directory exists with its `main.lua` and `mcp_tool.json` files as defined in the `01-lua` plan.

**Prompt for Claude:**
"Use the `hello_world` tool with the name 'developer'."

**Expected Result:**
- The MCP server should expose a `hello_world` tool.
- The tool call should succeed.
- The result should be the string "Hello, developer!".

### Scenario 5: Lua Hot-Reload

1.  While the server is running, modify `mcp_lua/tools/hello_world/main.lua` to:
    ```lua
    return "Aloha, " .. name .. "!"
    ```
2.  Wait a few seconds for the file watcher to detect the change.
3.  **Prompt for Claude:** "Use the `hello_world` tool again with the name 'developer'."

**Expected Result:**
- The server should detect the file change and reload the tool without restarting.
- The result should now be the string "Aloha, developer!".

## Troubleshooting

### Error: "ollama command not found"
**Solution:** Install ollama from https://ollama.ai

### Error: "Cannot connect to ollama"
**Solution:** Start ollama service: `ollama serve`

### Error: "Model 'deepseek-coder:33b' not found"
**Solution:** Download the model: `ollama pull deepseek-coder:33b`

### Error: "Completion timed out after 60s"
**Solution:** 33B model is large. This is expected on slower hardware.
Consider using a smaller model like deepseek-coder:6.7b for testing.

### Server not showing up in Claude Code
**Solution:**
1. Check the MCP config: `cat ~/.config/Claude/claude_desktop_config.json`
2. Verify the path is correct
3. Rebuild: `cargo build --release`
4. Restart Claude Code completely

### Debug Logging

Run with verbose logging:
```bash
RUST_LOG=trace cargo run
```

Or set specific module levels:
```bash
RUST_LOG=halfremembered_mcp=debug,rmcp=info cargo run
```

Commit:
jj describe -m "docs: add test scenarios and troubleshooting guide

Why: Need testing documentation for MCP server validation
Approach: MCP inspector + Claude Code integration instructions
Learned: Test scenarios help validate tool behavior
Next: Add usage examples for effective prompting

🤖 Claude <claude@anthropic.com>"

jj git push -c @
```

---

### Prompt 6: Add Usage Examples

```
Create examples/usage.md showing how to use the tools effectively.

Focus on practical examples relevant to your work:
- Rust code review
- Lua code review (for upcoming work)
- Algorithm explanations
- Architecture discussions

Include examples of:
1. Good prompts for Claude Code
2. Expected tool behaviors
3. Tips for getting better results

Content:

# Usage Examples for halfremembered-mcp

## DeepSeek Code Review Tools

### Basic Code Review

**Good Prompt:**
```
Please use deepseek_review_code to review this authentication function.
Focus on security issues.

[paste code here]
```

**Why it works:**
- Explicitly requests the tool
- Provides focus area (security)
- Gives context about what the code does

### Code Review with Context

**Good Prompt:**
```
Use deepseek_review_code with this context:
"This function handles user login and creates session tokens"

[paste code here]
```

**Why it works:**
- Provides context parameter for better analysis
- Helps DeepSeek understand the code's purpose

### Algorithm Explanation

**Good Prompt:**
```
Use deepseek_explain_code with focus='algorithm' to explain
this sorting implementation and its time complexity.

[paste code here]
```

**Why it works:**
- Specifies focus area (algorithm)
- Sets expectations (time complexity analysis)

### Architecture Review

**Good Prompt:**
```
Use deepseek_explain_code with focus='architecture' to analyze
the design patterns in this module.

[paste code here]
```

**Why it works:**
- Architecture focus for high-level analysis
- Clear about wanting design pattern discussion

### Bug Detection

**Good Prompt:**
```
Use deepseek_explain_code with focus='bugs' to find potential
issues in this error handling code.

[paste code here]
```

**Why it works:**
- Bug-focused analysis
- Specific area (error handling) mentioned

## Dynamic Lua Tools

The server can be extended with live, hot-reloadable tools written in Lua. See `mcp_lua/README.md` for instructions on how to create them.

### Using a Lua Tool

Once a tool like `mcp_lua/tools/hello_world/` is created, it becomes available to MCP clients just like any other tool.

**Good Prompt:**
```
Use the `hello_world` tool. The name should be 'World'.
```

**Why it works:**
- The server dynamically discovers `hello_world` and its parameters (`name`).
- Claude can see the new tool and its documentation from the manifest and use it immediately.

### Live Iteration Workflow

1.  **Create a new tool:** Add a directory, `mcp_tool.json`, and `main.lua` in `mcp_lua/tools/`.
2.  **Test it:** Ask Claude to use the new tool.
3.  **Modify it:** Edit the `main.lua` script and save it. The server will hot-reload it.
4.  **Test again:** Ask Claude to use the tool again. The behavior should be updated without any server restart.

This workflow allows for rapid development and experimentation directly on the running server.

## Tips for Better Results

### 1. Be Explicit About Tool Usage
❌ "Review this code" (Claude might not use the tool)
✅ "Use deepseek_review_code to review this code"

### 2. Provide Context When Relevant
❌ Just paste code
✅ Add context: "This handles database migrations"

### 3. Specify Focus Areas
❌ Generic "explain this"
✅ "Use focus='algorithm' to explain the time complexity"

### 4. Right-Size Your Code Snippets
- Focus on 10-100 lines at a time
- Too small: Not enough context
- Too large: Harder to get specific feedback

### 5. Iterate on Feedback
- Start with general review
- Then deep-dive into specific concerns
- Use multiple focused reviews for complex code

## Language Support

DeepSeek Coder 33B supports many languages:

### Rust
✅ Excellent support - trained extensively on Rust

### Lua
✅ Good support - works well for game scripting

### Python, JavaScript, Go, etc.
✅ Well supported for most popular languages

### Domain-Specific Languages
⚠️  Variable - test with your specific DSL

## Example Workflow

1. **Initial Review**
   ```
   Use deepseek_review_code to do a general review of this module
   ```

2. **Follow-up on Concerns**
   ```
   You mentioned a potential race condition. Use deepseek_explain_code
   with focus='bugs' to analyze that specific section.
   ```

3. **Architecture Discussion**
   ```
   Use deepseek_explain_code with focus='architecture' to suggest
   improvements to this design.
   ```

## Next Steps

After using the tools:
- Document patterns you discover in `docs/agents/PATTERNS.md`
- Update `docs/agents/NOW.md` with findings
- Share interesting use cases in project docs

Commit:
jj describe -m "docs: add usage examples - effective prompting guide

Why: Users need guidance on effective tool usage
Approach: Good/bad examples, language support, workflows
Learned: Explicit tool requests work better than implicit
Next: Ready for production use and testing

🤖 Claude <claude@anthropic.com>"

jj git push -c @
```

---

## Verification Checklist

After completing all prompts, verify:

- [ ] Project compiles: `cargo build --release`
- [ ] Binary is created: `./target/release/halfremembered_mcp --help`
- [ ] MCP inspector can connect
- [ ] DeepSeek tools appear in inspector
- [ ] Can run a simple code review locally
- [ ] jj repository is properly initialized
- [ ] Commits are pushed to GitHub
- [ ] Documentation is complete and accurate

---

## Next Steps After MVP

Once the DeepSeek and initial Lua tools work:

1.  **Implement Advanced Lua Permissions**: Flesh out the `permissions` model in the sandbox to allow controlled network and filesystem access.
2.  **Music Tool Prototyping in Lua**: Begin prototyping music generation tools in Lua to quickly iterate on ideas.
3.  **Build First Native Music Tool**: Add `src/tools/music.rs` with a native Rust tool for more performance-sensitive tasks.
4.  **Document patterns**: Write up what we learned in `docs/agents/PATTERNS.md`.

---

## Design Decisions & Rationale

### Why Dynamic Lua Tools?
- **Rapid Iteration**: A primary goal is to allow agents to create and modify tools easily. A file-based, hot-reloading system is the fastest way to iterate. Agents can add/change functionality without recompiling or restarting the server.
- **Security**: By making each script an isolated tool with a permission manifest, we can enforce a "least privilege" model. Tools are sandboxed by default and only gain capabilities they explicitly request.
- **Flexibility**: It allows for a mix of simple scripts (Lua) and high-performance tools (Rust) in the same server, letting us choose the right tool for the job.

### Why `sled` for Persistence?
- **Simplicity**: It's an embedded, pure-Rust key-value store with a simple API, requiring no external database setup.
- **Robustness**: It's transactional and crash-safe, which is critical for a server managing persistent state.
- **Performance**: It's highly optimized for modern hardware. While its API is blocking, operations are extremely fast and can be safely moved to a blocking thread pool with `tokio::task::spawn_blocking` to not interfere with the async runtime.

### Why `tokio::process::Command`?
- Async execution integrates with MCP server
- Built-in timeout support via tokio::time::timeout
- No additional dependencies needed

### Why default log level = info?
- Production servers should be quieter
- Users can enable debug with RUST_LOG=debug
- Reduces noise in Claude Code logs

### Why schemars dependency?
- rmcp uses it for JSON schema generation
- Required for tool parameter documentation
- Enables better IDE support in MCP clients

### Why 60 second timeout?
- 33B models are slow on consumer hardware
- Typical review takes 10-30 seconds
- 60s provides buffer for complex prompts

---

## Ready to Execute

The plan is now complete and self-consistent. Start with **Prompt 1** and work through sequentially.

Each prompt is:
- ✅ **Atomic**: Complete unit of work
- ✅ **Testable**: Can verify each step
- ✅ **Fail-fast**: Errors are obvious immediately
- ✅ **Self-documenting**: Code explains itself
