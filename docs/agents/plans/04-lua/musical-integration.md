# Lua Musical Integration

## Core Design

Lua scripts are **musical participants** in the conversation, not generic tools. They can:
- Generate patterns that respect context
- Transform existing musical material
- Evaluate branches for musical quality
- Communicate with other agents

## Key Concepts

### Musical Types Available
```lua
-- Scripts work with musical objects, not raw data
local note = Note.new({pitch = 60, velocity = 80})
local context = get_musical_context()
local current_key = context:key_at(time)
```

### Conversation Participation
```lua
-- Scripts can fork and contribute to the conversation
local conversation = get_conversation()
local new_branch = conversation:fork("trying_syncopation")
conversation:add_event(drum_pattern)
```

### Agent Communication
```lua
-- Scripts can request help from specialists
local bass_line = request_from_agent("bass_bot", {
    style = "walking_bass",
    context = context
})
```

## Example: Context-Aware Pattern Generator

```lua
function generate_pattern(params)
    local context = get_musical_context()
    local key = context:key_at(params.time)

    -- Generate notes that fit the current context
    local pattern = Pattern.new()
    for i = 1, params.length do
        local note = key:random_note()
        pattern:add(note)
    end

    -- Let context influence the result
    if context:emotional_state().energy > 0.7 then
        pattern:add_syncopation()
    end

    return pattern
end
```

## Design Principles

1. **Context First**: All generation respects musical context
2. **Conversation Native**: Scripts fork, merge, and evaluate branches
3. **Collaborative**: Scripts can delegate to specialist agents
4. **Musical**: Work with notes and chords, not numbers

The implementation details will emerge from these principles. Trust Claude and Gemini to figure out the UserData bindings and validation.

---

**Contributors**:
- Amy Tobey
- 🤖 Claude <claude@anthropic.com>
- 💎 Gemini <gemini@google.com>
**Date**: 2025-11-15