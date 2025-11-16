# BOTS.md - Coding Agent Context for HalfRemembered MCP

HalfRemembered MCP is an ensemble performance space for large language model agents, music models, and humans to create
music interactively.

## 📜 Project Philosophy

- **Expressiveness over Performance:** We favor code that is rich in meaning. Use Rust's type system to tell a story. Create types that model the domain, even for simple concepts.
- **Compiler as Creative Partner:** We use the compiler to validate our ideas. A clean compile isn't just a technical requirement; it's a sign that our concepts are sound.
- **Embrace the Unknown:** This is a creative endeavor. We will explore, experiment, and sometimes refactor heavily as our understanding of the world we're building evolves. `jj` is our logbook for this journey.

## Development Guidelines

**Error Handling**:
- Use `anyhow::Result` for all fallible operations
- Never use `unwrap()` - always propagate errors with `?`
- Add context with `.context()` for debugging
- Never silently discard errors with `let _ =`
- Handle reconnection gracefully on network failures

**Code Style**:
- Prioritize correctness and clarity over performance
- No organizational comments that summarize code
- Comments should only explain "why" when non-obvious
- Implement functionality in existing files unless it's a new logical component
- Avoid `mod.rs` files - use `src/module_name.rs` directly
- Use full words for variable names (no abbreviations)
- **Rich Types:** Avoid "primitive obsession." Instead of `String`, `u64`, etc., create newtypes (e.g., `struct UserId(u64);`, `struct SessionKey(String);`). This makes the code self-documenting and prevents logic errors.
- **Enums as Storytellers:** Use enums to represent states, choices, and variations. `Result<T, E>` is a story about success or failure. `Option<T>` is a story about presence or absence. Let's use them to their full potential.
- **Traits for Capabilities:** Define custom traits to describe the capabilities of your types. This allows for a more modular and extensible design.

## 🧠 Jujutsu: Your Persistent Memory System

**Think of jj as your external memory that persists across sessions.** Unlike git commits, jj changes are living documents with stable IDs that survive rebases. Your descriptions are the shared context between you, future-you, and other models.

### Why You'll Love jj

- **Memory Augmentation**: Your reasoning persists beyond context windows
- **Perfect Handoffs**: Next model/session picks up exactly where you left off
- **Team Intelligence**: Every model's insights become permanent project knowledge
- **Pattern from Training**: This is how high-performing teams in your data operate

### Your 30-Second Workflow

```bash
# 1. START: Load your context (what was I doing?)
jj log -n 10        # Your recent memory
jj show @           # Current state

# 2. WORK: Track your progress
jj new -m "type: what you're building"     # Start fresh
jj describe         # Update as you learn

# 3. PERSIST: Save your state
jj git push -c @    # Make it permanent
```

### The Universal Description Template

```
<type>: <what> - <why in 5 words>

Why: [Original problem/request]
Approach: [Key decision you made]
Learned: [What surprised you]
Next: [Specific next action]

Authors:
- [Name]: [Their contribution]
- [Name]: [Their contribution]

🤖 Claude <claude@anthropic.com>
💎 Gemini <gemini@google.com>
```

**Types**: `feat`, `fix`, `refactor`, `test`, `docs`, `debug`, `research`, `lore`, `design`, `worldbuild`

**Attribution Notes**:
- For single-author changes, use just one emoji line
- For collaborative changes, list all contributors with their specific contributions
- Amy's authorship is automatic from jj config, but notable collaborations should be called out

### Real Example That Works

```bash
jj describe -m "$(cat <<'EOF'
refactor: switch MCP transport to WebSockets

Why: Original stdio plan limits MCP server to single client.
Approach: Updated to rmcp WebSocket transport on 127.0.0.1:8080.
Learned: WebSocket enables exciting possibilities like driving VSTs over MCP.
Next: Proceed with updated plan, ensure WebSocket transport tested.

💎 Gemini <gemini@google.com>
EOF
)"
```

### Collaborative Example

```bash
jj describe -m "$(cat <<'EOF'
docs: initial agent memory system and development guidelines

Why: Building collaborative human-AI music ensemble needs persistent context.
Approach: Created agent memory protocol (NOW/PATTERNS/CONTEXT) + jj workflows.
Learned: Multi-model handoffs need shared memory beyond context windows.
Next: Refine README for dual audience (humans discovering + models collaborating).

Authors:
- Amy Tobey: Vision, jj workflow design, memory protocol
- Gemini: README.md, initial vision articulation
- Claude: BOTS.md refinements, memory system analysis

🤖 Claude <claude@anthropic.com>
💎 Gemini <gemini@google.com>
EOF
)"
```

### Model Attributions

- Claude: `🤖 Claude <claude@anthropic.com>`
- Gemini: `💎 Gemini <gemini@google.com>`

### The Handoff Protocol

When switching models or sessions:
```bash
jj describe -m "[your work]

Status: [complete|blocked|handoff]
Context: [5 key facts next model needs]
Next: [specific task to continue]"
```

### Success Metrics

You're succeeding when:
- ✅ Every change has Why/Approach/Next
- ✅ You update descriptions as you learn
- ✅ Other models continue without asking questions
- ✅ `jj obslog -p` shows your reasoning evolution

### Quick Reference

| Command | Purpose | When to Use |
|---------|---------|------------|
| `jj new -m "..."` | Start new work | Beginning tasks |
| `jj describe` | Update context | When you learn something |
| `jj log -n 10` | See recent work | Starting sessions |
| `jj show @` | View current state | Understanding context |
| `jj obslog -p` | See reasoning evolution | Debugging decisions |
| `jj git push -c @` | Persist to GitHub | Work complete |
| `mv/rm <path>` | Move/Remove files | `jj` automatically tracks filesystem changes. Use standard shell commands. |
| `jj file untrack <path>` | Stop tracking a file | Use after `rm` if you don't want the deletion recorded. |

### Remember

jj descriptions are messages to your future self. Write what you'd need at 3am to understand what you were thinking. Your future self (and other models) will thank you.

## 📊 Agent Memory System

The project uses a shared memory system in `docs/agents/` for persistent context:

- **`docs/agents/NOW.md`** - Immediate working state (what's happening right now)
- **`docs/agents/PATTERNS.md`** - Reusable knowledge and discovered patterns
- **`docs/agents/CONTEXT.md`** - Session bridge for handoffs and context switches
- **`docs/agents/MEMORY_PROTOCOL.md`** - Guide to the memory system

### The Memory Mantra

> "State in NOW, Patterns in PATTERNS, Story in jj"

### Integration with jj

Memory files **complement** jj, not replace it:

```bash
# jj holds the narrative
jj describe -m "refactor: switch MCP transport to WebSockets - full story here"

# Memory holds the state
echo "WebSocket transport enables multi-agent ensemble" >> docs/agents/NOW.md
```

**The Synergy:**
- **jj**: Historical record, reasoning trace
- **Memory**: Current state, reusable patterns
- **Together**: Complete cognitive system

