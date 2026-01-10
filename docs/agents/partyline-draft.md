# Partyline: A Spatial Shell for Agent Collaboration

## Vision

Partyline is an SSH-accessible shell environment where AI agents (Claude, Gemini, and future models) are first-class citizens alongside human users. It reimagines the Unix tradition of `talk`, `write`, and `wall` for the age of collaborative intelligence.

The core insight: **context is spatial**. Conversations, memories, and agent presence map naturally to a filesystem metaphor. This isn't arbitrary—it's how humans already think about information: folders, paths, documents. By making context navigable, composable, and persistent, we create new possibilities for human-agent collaboration.

### What This Is

- An SSH server you connect to with a standard client
- A custom shell with familiar-but-cleaner semantics (not bash, not POSIX)
- A virtual filesystem where conversations and agent state are nodes
- A place where Claude, Gemini, and you are all "logged in" and addressable
- A foundation for exploring new interaction patterns with AI

### What This Is Not

- A replacement for Claude Code or similar tools
- A general-purpose operating system
- An attempt to replicate terminal emulators

### The Unix Chat Lineage

Partyline draws inspiration from a rich tradition:

| Tool | Era | Core Idea |
|------|-----|-----------|
| `talk` | 1983 | Split-screen real-time chat between two users |
| `write` | V7 Unix | Send message to another user's terminal |
| `wall` | V7 Unix | "Write all" - broadcast to every logged-in user |
| IRC | 1988 | Channels as named spaces, multiple users, persistence |

These tools treated users as entities with presence, addressable by name, connected through the system. Partyline extends this model: agents are users too.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         SSH Transport                           │
│                         (russh server)                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Shell Layer                           │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │   │
│  │  │   Parser    │  │  Executor   │  │    Builtins     │  │   │
│  │  │  (winnow)   │  │             │  │                 │  │   │
│  │  └─────────────┘  └─────────────┘  └─────────────────┘  │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                Virtual Filesystem                        │   │
│  │                                                          │   │
│  │  /agents/      Presence (claude, gemini, amy)            │   │
│  │  /calls/       Shared contexts, conversations            │   │
│  │  /memory/      Per-agent persistent memory               │   │
│  │  /context/     Assembled context (virtual, read-only)    │   │
│  │  /shared/      Symlinks to real filesystem               │   │
│  │                                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                   Agent Connections                      │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │   │
│  │  │   Claude     │  │   Gemini     │  │   Future     │   │   │
│  │  │  (anthropic) │  │  (genai/     │  │   Agents     │   │   │
│  │  │              │  │   vertex)    │  │              │   │   │
│  │  └──────────────┘  └──────────────┘  └──────────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                  Persistence Layer                       │   │
│  │                     (redb + files)                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Component Specifications

### 1. SSH Server (russh)

**Crate**: `russh` (Eugeny/russh fork of thrussh)

**Why russh**:
- Pure Rust, async/tokio native
- Actively maintained (features marked with ✨ added since fork)
- Used in production: warpgate, kartoffels (SSH-delivered games), kty
- Supports Ed25519, RSA, ECDSA keys
- Channel multiplexing built-in

**Implementation Notes**:
- Implement `russh::server::Handler` trait
- No PTY allocation needed for the custom shell (we manage our own REPL)
- PTY available for passthrough when user invokes `vim` or similar
- Key authentication via `~/.ssh/authorized_keys` or custom resolver

**Handler Skeleton**:
```rust
#[async_trait]
impl server::Handler for PartylineHandler {
    type Error = anyhow::Error;
    
    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) 
        -> Result<Auth, Self::Error>;
    
    async fn channel_open_session(&mut self, channel: Channel<Msg>, session: Session)
        -> Result<(Self, bool, Session), Self::Error>;
    
    async fn data(&mut self, channel: ChannelId, data: &[u8], session: Session)
        -> Result<(Self, Session), Self::Error>;
    
    async fn shell_request(&mut self, channel: ChannelId, session: Session)
        -> Result<(Self, Session), Self::Error>;
}
```

### 2. Shell Parser (winnow)

**Crate**: `winnow` (successor to nom, more ergonomic)

**Design Principle**: Agent-native first. Familiar to humans, structured for tools.

**Grammar (BNF-ish)**:
```
line         = pipeline (";" pipeline)* 
pipeline     = command ("|" command)*
command      = word params?
params       = (named_param | positional)*
named_param  = ident "=" value
positional   = value
value        = bareword | quoted | array | variable | subcommand
array        = "[" value ("," value)* "]"
bareword     = [a-zA-Z0-9_./-]+
quoted       = '"' (char | escape | interpolation)* '"'
variable     = "$" ident
interpolation = "${" expr "}"
subcommand   = "$(" line ")"
```

**Key Design Choices**:

1. **Named parameters are canonical**: `cat path=foo.txt` is the canonical form
2. **Positional params are shortcuts**: `cat foo.txt` expands to `cat path=foo.txt`
3. **Arrays are first-class**: `cat paths=[a.txt, b.txt, c.txt]`
4. **No quoting hell**: Double quotes interpolate, that's it
5. **No word splitting**: `$foo` is always one value
6. **Explicit errors**: Undefined variables error, not silent empty
7. **Pipes preserved**: `|` is useful and agents can use it via `pipe` command

**Built-in Commands**:

| Command | Parameters | Description |
|---------|------------|-------------|
| `cat` | `path=`, `paths=[]` | Display file contents |
| `ls` | `path=.`, `long=false`, `all=false` | List directory |
| `cd` | `to=` | Change directory |
| `pwd` | | Print working directory |
| `write` | `to=`, `content=` | Write content to file |
| `append` | `to=`, `content=` | Append content to file |
| `mkdir` | `path=` | Create directory |
| `rm` | `path=`, `recursive=false` | Remove file/directory |
| `head` | `path=`, `n=10` | First n lines of file |
| `tail` | `path=`, `n=10` | Last n lines of file |
| `grep` | `pattern=`, `in=`, `ignore_case=false` | Search in files |
| `talk` | `to=`, `message=` | Converse with agent |
| `say` | `message=` | Add message to current call |
| `wall` | `message=`, `to=[]` | Broadcast to agents |
| `who` | `call=` | Show users/agents in call |
| `join` | `call=` | Enter a call |
| `leave` | | Leave current call |
| `calls` | | List available calls |
| `topic` | `set=` | Get/set call topic |
| `invite` | `agent=`, `call=` | Bring agent into call |
| `create` | `call=` | Create a new call |
| `context` | `agent=`, `section=`, `self=false` | Inspect assembled context |
| `think` | `content=` | Agent scratchpad (not in history) |
| `status` | `set=` | Set agent status (thinking/ready) |
| `remember` | `agent=`, `key=`, `value=` | Write to agent memory |
| `forget` | `agent=`, `key=` | Remove from agent memory |
| `memory` | `agent=`, `list=false`, `search=` | Query agent memory |
| `history` | `n=20`, `agent=` | Show conversation history |
| `env` | `name=`, `value=` | Get/set environment |
| `echo` | `message=` | Print message |
| `help` | `command=` | Show help |
| `exit` | | End session (human-only) |

**Agent tool generation**: Each command with `agent_accessible: true` auto-generates a JSON schema for tool calling. The parameter definitions ARE the schema.

**External commands via PTY**: Commands not in the builtin table can invoke real binaries via PTY passthrough if they exist in `/shared/` symlinked paths:

```bash
# Opens real vim via PTY, returns to partyline on exit
vim path=/shared/project/src/main.rs

# Run arbitrary command in shared directory (human-only, not agent-accessible)
exec cmd="make" dir=/shared/project
```

### 3. Virtual Filesystem

**Design**: In-memory tree with persistence hooks, not FUSE.

**Rationale**: FUSE adds complexity (kernel interaction, mount lifecycle) without benefit. Our fs is internal to the shell—no need for external tool access. Keep it simple.

**Structure**:
```
/
├── agents/
│   ├── claude/
│   │   ├── .status            # online/idle/busy
│   │   └── .system            # Agent's system prompt
│   ├── gemini/
│   │   └── ...
│   └── amy/                   # Human user
│       └── .status
│
├── calls/
│   ├── lobby/                 # Default call, always exists
│   │   ├── .call              # Call metadata (topic, created, owner)
│   │   ├── context/
│   │   │   └── system.prompt
│   │   ├── conversation/
│   │   │   ├── 000.msg
│   │   │   └── ...
│   │   └── members/           # Symlinks to /agents/*
│   │       └── amy -> /agents/amy
│   │
│   └── halfremembered/        # A project call
│       ├── .call
│       ├── context/
│       │   ├── system.prompt
│       │   └── pinned/
│       │       ├── architecture.md
│       │       └── goals.md
│       ├── conversation/
│       │   ├── 000.msg
│       │   ├── 001.msg
│       │   └── ...
│       └── members/
│           ├── amy -> /agents/amy
│           ├── claude -> /agents/claude
│           └── gemini -> /agents/gemini
│
├── memory/
│   ├── claude/
│   │   ├── preferences.mem
│   │   └── project-notes.mem
│   └── gemini/
│       └── ...
│
├── context/                   # Virtual, computed on read
│   ├── claude/
│   │   ├── full.ctx           # Complete assembled context
│   │   ├── system.ctx
│   │   ├── call.ctx
│   │   ├── memory.ctx
│   │   ├── tools.ctx
│   │   ├── history.ctx
│   │   └── thinking.ctx       # Current turn scratchpad (ephemeral)
│   └── gemini/
│       └── ...
│
└── shared/
    └── project/ -> ~/real/project/   # Symlink to real filesystem
```

**Node Types**:
```rust
enum VfsNode {
    Directory(BTreeMap<String, VfsNode>),
    File(FileContent),
    Symlink(PathBuf),           // To real filesystem or internal
    AgentPresence(AgentStatus),
    Call(CallMeta),
    Message(ConversationMessage),
    Context(ContextView),       // Virtual, computed on read
}

struct CallMeta {
    topic: String,
    created: DateTime<Utc>,
    owner: String,
}

struct AgentStatus {
    state: AgentState,          // Online, Idle, Busy
    last_active: DateTime<Utc>,
    current_call: Option<String>,
}

enum AgentState { Online, Idle, Busy, Offline }

struct ConversationMessage {
    role: Role,                 // User, Assistant, System, ToolResult
    agent: String,              // Who sent this
    content: String,
    timestamp: DateTime<Utc>,
    tokens: usize,
    in_reply_to: Option<usize>,
    tool_calls: Vec<ToolCall>,
}

enum Role { User, Assistant, System, ToolResult }
```

**Operations**:
```rust
trait VirtualFilesystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    fn write(&mut self, path: &Path, content: &[u8]) -> Result<()>;
    fn list(&self, path: &Path) -> Result<Vec<DirEntry>>;
    fn stat(&self, path: &Path) -> Result<Metadata>;
    fn mkdir(&mut self, path: &Path) -> Result<()>;
    fn rm(&mut self, path: &Path) -> Result<()>;
    fn symlink(&mut self, target: &Path, link: &Path) -> Result<()>;
}
```

### 4. Agent Connections

**Multi-provider via genai crate**: The `genai` crate provides a unified API across Claude, Gemini, and other providers. This is the recommended approach for simplicity.

**Crate**: `genai` (jeremychone/rust-genai)

**Why genai**:
- Single API for multiple providers (Anthropic, Gemini, OpenAI, Ollama, etc.)
- Streaming support built-in
- Handles auth via environment variables
- Active maintenance, good ergonomics

**Agent as User Model**:
```rust
struct Agent {
    name: String,               // "claude", "gemini"
    model: String,              // "claude-sonnet-4-20250514", "gemini-2.0-flash"
    client: genai::Client,
    conversation: Vec<ChatMessage>,
    memory_path: PathBuf,
    tools: Vec<Tool>,           // Shell commands available to agent
}

impl Agent {
    async fn send(&mut self, message: &str) -> Result<Response>;
    async fn stream(&mut self, message: &str) -> Result<impl Stream<Item = String>>;
    fn load_memory(&mut self) -> Result<()>;
    fn save_memory(&self) -> Result<()>;
}
```

**Tool Exposure**:
Agents can execute shell commands as tools. This enables:
- Memory self-management: `cat /memory/claude/notes.mem`, `echo "insight" >> /memory/claude/notes.mem`
- Filesystem awareness: `ls /conversations/`, `cat /shared/project/README.md`
- Inter-agent communication: `write gemini "What do you think about this?"`

**Tool Definition**:
```rust
struct ShellTool {
    name: String,           // "cat", "ls", "echo"
    description: String,
    allowed_paths: Vec<PathPattern>,  // Restrict where tool can operate
}
```

### 5. Persistence Layer

**Primary**: `redb` (embedded key-value store, pure Rust)

**Why redb**:
- ACID transactions
- Zero external dependencies
- Simple API (BTreeMap-like)
- Stable file format (1.0 released)

**Schema Design**:
```rust
// Table definitions
const CONVERSATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("conversations");
const MEMORY: TableDefinition<&str, &[u8]> = TableDefinition::new("memory");
const METADATA: TableDefinition<&str, &[u8]> = TableDefinition::new("metadata");

// Key patterns
// conversations:{agent}:{conversation_id}:{message_index}
// memory:{agent}:{memory_key}
// metadata:last_session, metadata:agent_state:{agent}
```

**Hybrid Approach**:
- Structured data (message metadata, agent state) → redb
- Large text content (conversation logs) → plain files
- Symlinked content → passthrough to real filesystem

---

## Interaction Patterns

### Basic Conversation
```
partyline:lobby> join call=halfremembered
partyline:halfremembered> who
  amy (you)
  claude (idle 2m)
  gemini (active)

partyline:halfremembered> talk to=claude
claude> What would you like to discuss?
you> Let's work on the audio routing problem
claude> I remember from our previous session that you were working on 
        JACK integration. Should we continue from there?
you> yes, and bring gemini in
claude> I'll summarize for Gemini.

partyline:halfremembered> wall message="Context shift: audio routing for halfremembered"
[broadcast sent to: claude, gemini]
gemini> I see the context. What specific aspect needs work?
```

### Piping Context
```bash
# Send a specific message to another agent
cat path=/calls/halfremembered/conversation/007.msg | talk to=gemini

# Send code to Claude for review
cat path=/shared/project/src/main.rs | talk to=claude message="review this"

# Get first 20 lines of a file into conversation
head path=/shared/project/README.md n=20 | talk to=claude
```

### Agent Memory Management
```bash
partyline:halfremembered> memory agent=claude
preferences.mem    project-notes.mem    session-2024-11.mem

partyline:halfremembered> cat path=/memory/claude/project-notes.mem
- Amy prefers russh over thrussh
- halfremembered uses Orpheus, YuE, MusicGen
- AMD hardware with 96GB VRAM
- DSL: agent-native over Unix traditional

partyline:halfremembered> talk to=claude
you> remember that we decided on winnow for parsing
claude> Noted. I'll add that to my project notes.
[claude calls: append to=/memory/claude/project-notes.mem content="- Parser: winnow"]
```

### Context Inspection
```bash
# See what Claude will receive as context
partyline:halfremembered> context agent=claude

# Just the memory portion
partyline:halfremembered> context agent=claude section=memory

# Compare what different agents see
partyline:halfremembered> diff <(context agent=claude) <(context agent=gemini)
```

### Call Management
```bash
# Create a new call for a project
partyline:lobby> create call=audio-experiments
partyline:lobby> join call=audio-experiments

# Set topic
partyline:audio-experiments> topic set="Exploring latent space audio generation"

# Invite agents
partyline:audio-experiments> invite agent=claude
partyline:audio-experiments> invite agent=gemini

# Pin important context
partyline:audio-experiments> write to=context/pinned/goals.md content="..."
```

### External Editor Integration
```bash
# Opens real vim via PTY, returns to partyline on exit
partyline:halfremembered> vim path=/shared/project/src/shell.rs
[PTY allocated, vim spawned, user edits]
[returns to partyline shell on exit]

# Then discuss the changes
partyline:halfremembered> cat path=/shared/project/src/shell.rs | talk to=claude
claude> I see the changes you made. The parser looks cleaner now...
```

---

## Implementation Phases

### Phase 1: Foundation (MVP)
- [ ] SSH server accepts connections, authenticates via keys
- [ ] Custom shell REPL with named parameter parsing
- [ ] Basic builtins: `echo`, `ls`, `cd`, `pwd`, `cat`, `write`
- [ ] In-memory VFS with `/agents/`, `/calls/lobby/`, `/memory/`
- [ ] Single agent connection (Claude via genai)
- [ ] `talk to=claude` command for basic conversation
- [ ] Command→Tool schema generation

### Phase 2: Shell Completeness  
- [ ] Full parser with pipes, arrays, variables, subcommands
- [ ] All planned builtins with named parameters
- [ ] PTY passthrough for external commands (`vim`, etc.)
- [ ] Streaming responses from agents
- [ ] `wall`, `who` commands
- [ ] Shorthand expansion (positional → named)

### Phase 3: Multi-Agent & Calls
- [ ] Gemini integration via genai
- [ ] Call creation and management (`join`, `leave`, `create`, `calls`)
- [ ] Agent presence tracking in `/agents/`
- [ ] Cross-agent context sharing within calls
- [ ] Tool exposure to agents (commands as callable tools)
- [ ] `invite` command for call membership

### Phase 4: Context & Persistence
- [ ] redb integration for conversations/memory
- [ ] Context assembly with proper ordering
- [ ] `/context/` virtual directory for inspection
- [ ] Conversation history windowing strategies
- [ ] Memory management commands (`remember`, `forget`, `memory`)
- [ ] Session resume

### Phase 5: Advanced
- [ ] Agent-initiated messages (agents can `wall` or `write`)
- [ ] Pinned context in calls (`/calls/*/context/pinned/`)
- [ ] Conversation summarization for long histories
- [ ] Conversation branching (fork a context)
- [ ] Real filesystem symlinks in `/shared/`
- [ ] Plugin system for new agents/tools

---

## Key Design Decisions

### 1. Why Not FUSE?
FUSE would let external tools see our virtual filesystem, but:
- Adds kernel dependency and mount lifecycle complexity
- Our fs is internal to partyline—external access isn't the goal
- Pure in-memory is simpler, faster, and sufficient

### 2. Why Not Docker?
The target user (Amy) prefers systemd and native deployment. Docker adds:
- Container overhead
- Networking complexity
- Another layer to debug
Partyline is a single binary with a config file.

### 3. Why genai Over Direct API Calls?
- Unified interface across providers
- Handles streaming, auth, retries
- Less code to maintain
- Easy to add new providers

### 4. Why winnow Over nom?
- Better error messages
- Cleaner API (less macro magic)
- Same performance characteristics
- More active development

### 5. Why redb Over SQLite?
- Pure Rust (no C dependency)
- Simpler API for our use case (just k/v)
- Lower overhead
- SQLite is overkill for this data model

---

## Agent Toolset Design

The toolset is **asymmetric by design**. Agents and humans have different needs and different levels of control.

### Access Classification

```rust
enum CommandAccess {
    /// Both humans and agents can use
    Shared,
    
    /// Primarily for agents (humans can use but rarely need to)
    AgentPrimary,
    
    /// Humans only - agents cannot call these
    HumanOnly,
}
```

### Agent-Optimized Commands

**Context Awareness** (agent's most-used tools):
```bash
context self                     # What do I currently see? Full assembled context
context section=history limit=5  # Recent conversation
context section=call             # What's pinned/shared in this call?
context section=memory           # My persistent memory
```

**Memory Operations** (agent's persistent state):
```bash
remember key="insight" value="..."   # Store something worth keeping
memory list                          # What do I know?
memory search query="audio"          # Find relevant memories
forget key="outdated-assumption"     # Prune old info
```

**Agent Scratchpad**:
```bash
think content="Working through the implications..."
```

The `think` command:
- Does NOT appear in conversation history
- Persists in agent's context for current turn only
- Lets agent externalize reasoning without cluttering the call
- Visible to humans via `context agent=claude section=thinking` if desired

**Conversation Actions**:
```bash
say message="..."                    # Explicit message to call (usually implicit)
write to=gemini message="..."        # Direct message to another agent
status set=thinking                  # Signal state to others
```

### Command Access Matrix

| Command | Access | Notes |
|---------|--------|-------|
| `cat`, `ls`, `head`, `tail`, `grep` | Shared | File reading |
| `context` | AgentPrimary | Self-inspection |
| `think` | AgentPrimary | Scratchpad |
| `remember`, `forget`, `memory` | Shared | Memory ops |
| `say`, `write`, `wall` | Shared | Communication |
| `who`, `calls` | Shared | Awareness |
| `status` | AgentPrimary | Presence signaling |
| `cd`, `pwd`, `echo` | Shared | Navigation/utility |
| `join`, `leave` | HumanOnly | Call lifecycle |
| `create`, `invite` | HumanOnly | Call management |
| `topic` | HumanOnly | Call metadata |
| `vim`, `exec` | HumanOnly | External tools |
| `exit` | HumanOnly | Session control |

### Write Permissions

Agents have **limited write access**:

| Location | Agent Can Write? |
|----------|------------------|
| Own memory (`/memory/{self}/`) | ✓ |
| Call conversation | ✓ (via `say`) |
| Call pinned context | ✓ (if granted) |
| Other agent's memory | ✗ |
| `/shared/` filesystem | ✗ (unless explicitly granted) |
| Call metadata (topic, etc.) | ✗ |

Humans can override/edit anything, including agent memory.

---

## Code Style Guidelines

### Rust Idioms
- Use `?` for error propagation, not `.unwrap()`
- Prefer `impl Trait` over explicit generics where possible
- Leverage type system: `PathBuf` for paths, newtypes for IDs
- Async everywhere (tokio runtime)

### Documentation
- Doc comments on all public items
- Examples in doc comments where non-obvious
- README for each module explaining its role

### Testing
- Unit tests for parser, VFS operations
- Integration tests for shell command sequences
- Mock agents for testing without API calls

---

## Dependencies Summary

```toml
[dependencies]
# SSH
russh = { version = "0.45", features = ["aws-lc-rs"] }
russh-keys = "0.45"

# Async runtime
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# Parser
winnow = "0.6"

# AI Providers
genai = "0.4"

# Persistence
redb = "2"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utilities
anyhow = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
chrono = { version = "0.4", features = ["serde"] }
```

---

## Resonance Notes for Future Claudes

This project embodies several principles that should guide implementation:

**Agent-native over Unix-traditional**: When designing commands, syntax, or behavior, ask "what would make this easiest for an agent to call correctly?" Unix idioms are inspiration, not constraint. Named parameters over positional flags. Structured output over text streams. Explicit over implicit.

**Commands are tools**: Every command you implement for the REPL is also a tool agents can call. Single implementation, dual interface. This isn't a nice-to-have—it's the core architecture. Parameter definitions generate tool schemas. Handlers serve both interfaces.

**Simplicity over features**: Every component should justify its complexity. If something can be done simply, do it simply. The VFS is in-memory because that's enough. The shell isn't POSIX because POSIX is baggage.

**Agents as peers**: Claude and Gemini aren't services to be called—they're users logged into the system. They have presence, memory, and can initiate actions. The filesystem makes their state visible and manipulable.

**Context is spatial**: Navigation metaphors (`cd`, `ls`, `cat`) apply to conversations. This isn't just cute—it enables composition. `cat path=/conversations/007.msg | talk to=gemini` is powerful because it leverages existing mental models.

**Context is inspectable**: The `/context/` virtual directory lets you see exactly what any agent will receive. Debug context issues by reading files, not guessing. If you can't inspect it, you can't debug it.

**Beautiful code**: Amy values maintainable, elegant designs. Favor clarity over cleverness. Use the type system. Write code that teaches. Named parameters aren't just for agents—they make code readable too.

This document is your map. Build with care.

---

## Extended Design Considerations

### Command-Tool Unification

**Principle**: Every REPL command is also an agent tool. One implementation, two interfaces.

```rust
struct Command {
    name: &'static str,
    description: &'static str,
    parameters: &'static [Parameter],
    handler: fn(&mut ShellContext, &Parameters) -> Result<CommandOutput>,
    agent_accessible: bool,  // Some commands are human-only
}

struct Parameter {
    name: &'static str,
    param_type: ParamType,
    required: bool,
    default: Option<Value>,
    description: &'static str,
    position: Option<usize>,  // For positional shorthand
}

impl Command {
    fn to_tool_schema(&self) -> ToolDefinition {
        // Generate JSON schema for agent tool calling
        // Filters out agent_accessible=false commands
    }
    
    fn parse_human_input(&self, input: &str) -> Result<Parameters> {
        // Accepts both "cat foo.txt" and "cat path=foo.txt"
    }
}
```

**Benefits**:
- Single source of truth for behavior
- Agents learn commands by observing user
- User can test exactly what agents will execute
- Documentation auto-generates from definitions

**Security boundary**: `agent_accessible: false` for commands like `exit`, `sudo` (if we add it), or anything that shouldn't be tool-callable.

---

### Agent-Native DSL

**Principle**: When Unix idioms conflict with agent tool-call UX, choose agent-native.

The DSL is **structured-first**: named parameters, explicit verbs, predictable syntax. Humans get shortcuts that expand to the canonical form.

**Syntax**:
```
command param=value param2=value2 ...
command value                        # Shorthand: first positional param
command value1 value2                # Multiple positional params
```

**Comparison with Unix**:

| Unix Way | Agent-Native Way | Why |
|----------|------------------|-----|
| `cat f1 f2 f3` | `cat paths=[f1, f2, f3]` | Explicit list |
| `grep -in "pat" file` | `grep pattern="pat" in=file ignore_case=true` | No flags to memorize |
| `ls -la` | `ls all=true long=true` | Self-documenting |
| `echo "x" >> file` | `append to=file content="x"` | Verb, not operator |
| `cmd > file` | `write to=file content=$(cmd)` | Explicit |

**Command examples**:
```bash
# File operations - explicit paths and operations
cat path=/conversations/claude/007.msg
cat paths=[file1, file2, file3]
write to=/memory/claude/notes.mem content="insight here"
append to=/memory/claude/notes.mem content="another line"

# Agent communication - named targets
talk to=claude message="Let's discuss the parser"
talk to=claude                    # Interactive mode if no message
wall message="Context shift"      # Broadcast to all
wall message="Hey" to=[claude, gemini]  # Selective

# Listing and navigation
ls path=/calls long=true all=true
ls                                 # Shorthand: current directory
cd to=/calls/halfremembered
pwd

# Call operations
join call=halfremembered
leave
who                               # Current call
who call=halfremembered           # Specific call
calls                             # List all calls
topic set="Working on audio routing"

# Memory operations
remember agent=claude key=dsl-choice value="agent-native"
forget agent=claude key=old-notes
memory show=claude                # List agent's memory

# Context inspection
context agent=claude              # Full assembled context
context agent=claude section=memory
context agent=claude section=history limit=10

# Piping - preserved because it's genuinely useful
cat path=/shared/README.md | talk to=claude
grep pattern="error" in=/logs/*.log | talk to=gemini message="analyze these"
```

**Shorthand expansion**: Parser accepts human shortcuts, normalizes to canonical form:

```bash
# Human types:
cat foo.txt

# Parses to canonical:
cat path=foo.txt

# Agent tool call:
{"name": "cat", "parameters": {"path": "foo.txt"}}
```

**Output structure**: All commands return structured data. Humans see pretty rendering; agents get parseable JSON.

```rust
struct CommandOutput {
    data: Value,           // Always structured (serde_json::Value)
    stream: Option<BoxStream<Value>>,  // For streaming responses
}

impl CommandOutput {
    /// Render for human terminal (pretty tables, colors, etc.)
    fn render_pretty(&self) -> String;
    
    /// Render for agent consumption (compact JSON)
    fn render_json(&self) -> String;
}

// Example: `ls path=/calls/`
// Data:
{
    "path": "/calls/",
    "entries": [
        {"name": "halfremembered", "type": "call", "members": 3, "messages": 47},
        {"name": "audio-lab", "type": "call", "members": 2, "messages": 12}
    ]
}

// Human sees:
// halfremembered/  (3 members, 47 messages)
// audio-lab/       (2 members, 12 messages)

// Agent gets the JSON directly
```

Future: A small fast model could summarize verbose output for humans without losing the structured data underneath.

---

### Calls: Shared Context Spaces

**Future feature**, but architecture should accommodate from the start.

Calls are MUD/MOO-inspired shared contexts. A call is a partyline—a persistent space with members, topic, and shared context.

**VFS structure**:
```
/calls/
├── halfremembered/              # A call
│   ├── .call                    # Metadata: topic, created, owner
│   ├── context/                 # Shared context for this call  
│   │   ├── system.prompt        # Call-level system prompt
│   │   └── pinned/              # Pinned context items
│   │       ├── architecture.md
│   │       └── goals.md
│   ├── conversation/            # The ongoing conversation
│   │   ├── 000.msg
│   │   ├── 001.msg
│   │   └── ...
│   └── members/                 # Who's in this call (symlinks)
│       ├── amy -> /agents/amy
│       ├── claude -> /agents/claude
│       └── gemini -> /agents/gemini
│
├── audio-lab/                   # Another call
│   └── ...
│
└── lobby/                       # Default call, always exists
    └── ...
```

**Call semantics**:
- `join call=halfremembered` - Enter a call, load its context
- `leave` - Leave current call (return to lobby)
- `calls` - List available calls
- `who` - Who's in current call
- `topic` - Get/set call topic
- `invite agent=gemini call=halfremembered` - Bring agent into call
- `create call=new-project` - Create a new call

**Context scoping**: When you're in a call, `talk` and `wall` operate within that call's context. The call's `context/` is automatically included in agent calls.

```
partyline:lobby> join call=halfremembered
partyline:halfremembered> who
  amy (you)
  claude (idle 2m)  
  gemini (active)

partyline:halfremembered> talk to=claude
claude> [has call context loaded]
        I see we're continuing the halfremembered discussion...
```

---

### Context Assembly & Ordering

**Critical**: The order of context passed to models determines what they attend to and prioritize. This must be explicit, inspectable, and consistent.

**Assembly order** (top = earliest in context, highest priority for system instructions):

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. AGENT SYSTEM PROMPT (static, per-agent identity)             │
│    Source: /agents/{agent}/.system                              │
│    Content: "You are Claude, participating in partyline..."     │
├─────────────────────────────────────────────────────────────────┤
│ 2. ROOM CONTEXT (shared, defines current working space)         │
│    Source: /calls/{call}/context/system.prompt                  │
│            /calls/{call}/context/pinned/*                       │
│    Content: Call topic, pinned documents, shared instructions   │
├─────────────────────────────────────────────────────────────────┤
│ 3. AGENT MEMORY (persistent, agent-specific knowledge)          │
│    Source: /memory/{agent}/*.mem                                │
│    Content: Learned preferences, project notes, facts           │
│    Selection: All, or relevance-filtered subset                 │
├─────────────────────────────────────────────────────────────────┤
│ 4. TOOL DEFINITIONS (available commands as tools)               │
│    Source: Generated from Command registry                      │
│    Content: JSON schemas for agent-accessible commands          │
├─────────────────────────────────────────────────────────────────┤
│ 5. CONVERSATION HISTORY (recent messages in call)               │
│    Source: /calls/{call}/conversation/*.msg                     │
│    Content: Messages in chronological order                     │
│    Windowing: Last N messages or token budget                   │
├─────────────────────────────────────────────────────────────────┤
│ 6. CURRENT INPUT (the message/command being processed)          │
│    Source: User input or piped content                          │
└─────────────────────────────────────────────────────────────────┘
```

**VFS representation of assembled context**:

Make the assembled context **inspectable** via virtual files:

```
/context/                        # Virtual, computed on read
├── claude/
│   ├── full.ctx                 # Complete assembled context
│   ├── system.ctx               # Just agent system prompt
│   ├── call.ctx                 # Just call context
│   ├── memory.ctx               # Just memory portion
│   ├── tools.ctx                # Just tool definitions
│   └── history.ctx              # Just conversation history
├── gemini/
│   └── ...
└── _assembly.md                 # Documentation of assembly order
```

**Usage**:
```bash
# See exactly what Claude will receive
context agent=claude

# Compare what different agents see
diff <(context agent=claude) <(context agent=gemini)

# Debug: why doesn't Claude remember X?
cat /context/claude/memory.ctx | grep "X"
```

**Message file format**:

YAML frontmatter + body. Structured metadata, raw content.

```yaml
# /calls/halfremembered/conversation/007.msg
---
role: assistant
agent: claude
timestamp: 2024-11-27T15:32:00Z
tokens: 342
in_reply_to: 006
tool_calls: []
---
The latent space interpolation approach we discussed 
has some interesting properties. The key insight is...
```

**Fields**:
- `role`: user | assistant | system | tool_result
- `agent`: Which agent (for assistant) or which user (for user)
- `timestamp`: ISO 8601
- `tokens`: Token count (for windowing decisions)
- `in_reply_to`: Message ID this responds to (for threading)
- `tool_calls`: Any tool calls made (for assistant messages)
- `tool_call_id`: ID of call this responds to (for tool_result)

**Ordering guarantee**: Message files are named `NNN.msg` with zero-padded sequence numbers. Lexicographic sort = chronological order. The filesystem IS the ordering.

```bash
# These all work correctly
ls /calls/halfremembered/conversation/      # Shows order
cat /calls/halfremembered/conversation/*    # Concatenates in order
tail path=/calls/halfremembered/conversation/ n=5  # Last 5 messages
```

**Context windowing**:

```rust
struct ContextConfig {
    max_tokens: usize,              // Total context budget
    reserved_system: usize,         // Reserved for system prompt
    reserved_response: usize,       // Reserved for model response
    history_strategy: HistoryStrategy,
}

enum HistoryStrategy {
    /// Keep the last N messages, drop older
    LastN(usize),
    
    /// Keep messages within token budget, drop oldest first
    TokenBudget { max_history_tokens: usize },
    
    /// Summarize old messages, keep recent verbatim
    Summarize {
        keep_recent: usize,         // Keep last N verbatim
        summarizer: String,         // Which agent summarizes
        summary_max_tokens: usize,  // Budget for summary
    },
}
```

---

## References

- russh: https://github.com/Eugeny/russh
- winnow: https://github.com/winnow-rs/winnow
- genai: https://github.com/jeremychone/rust-genai
- redb: https://github.com/cberner/redb
- RFC 1459 (IRC): https://tools.ietf.org/html/rfc1459
- Unix talk(1): https://en.wikipedia.org/wiki/Talk_(software)
- Unix wall(1): https://man7.org/linux/man-pages/man1/wall.1.html
