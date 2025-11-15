# Plan 04: Musical Pattern Scripting with Lua

**Status**: Planned for Phase 3
**Dependencies**: Requires Plan 03 (Musical Domain Model) complete
**Timeline**: After musical foundation is working

This plan extends the musical domain with Lua scripting for pattern generation, musical transformations, and custom agent behaviors. Lua scripts can create musical events, listen to conversations, and implement reusable musical patterns.

## Before Starting

📝 **Read [test-driven-approach.md](../test-driven-approach.md)** first. Write tests for Lua pattern generators that verify musical output, not just script execution.

## Files

- `plan.md` - Complete implementation plan with 7 prompts.
- **Prompts 1-5**: Ready to execute
- **Prompts 6-7**: Conceptual placeholders (hot-reload integration needs refinement)

## Scope

### Musical Patterns
1. **Pattern Generators:** Lua scripts that generate musical patterns (drum beats, bass lines, arpeggios)
2. **Transformers:** Scripts that transform existing musical events (transpose, invert, augment)
3. **Evaluators:** Scripts that score musical branches for quality/fitness

### Integration with Domain Model
1. **Event Creation:** Lua can create both Concrete and Abstract events
2. **Context Access:** Scripts can query the MusicalContext
3. **Conversation Listening:** Scripts can subscribe to conversation events
4. **Branch Operations:** Scripts can fork, evaluate, and suggest merges

### Exposed Data Structures
1. **Musical Types:** Note, Chord, Key, Scale, Pattern accessible from Lua
2. **Event Types:** ConcreteEvent, AbstractEvent constructors
3. **Conversation API:** Access to tree, nodes, branches
4. **Context Queries:** Real-time access to musical context
5. **Message System:** Send/receive JamMessages from Lua

See [musical-integration.md](musical-integration.md) for detailed API

### Original Features (Still Relevant)
1. **Hot-Reloading:** Modify patterns without restart
2. **Sandboxing:** Safe execution environment
3. **State Persistence:** Remember pattern parameters

## Success Criteria

- [ ] Lua scripts can generate musical patterns as Events
- [ ] Scripts can access and respond to MusicalContext
- [ ] Pattern generators integrate with the conversation tree
- [ ] Scripts can listen to and respond to JamMessages
- [ ] Hot-reload works for pattern changes
- [ ] Musical patterns can be parameterized and saved
