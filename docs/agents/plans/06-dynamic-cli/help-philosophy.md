# Help Text Philosophy - Writing for Humans and AI

## 🎭 The Dual Audience

Our CLI speaks to two fundamentally different types of users:

1. **Humans** - Need practical examples, shell patterns, and troubleshooting
2. **AI Agents** - Need conceptual context, emotional mapping, and intention frameworks

The help text must resonate with both without alienating either.

## 📝 Core Principles

### 1. Start with Intent, Not Mechanism
❌ **Bad**: "This command sends a POST request to /play endpoint"
✅ **Good**: "Transform a musical intention into sound"

### 2. Explain the Emotional Dimension
❌ **Bad**: "valence: number between -1 and 1"
✅ **Good**: "valence: Joy-sorrow axis, where -1.0 represents deep melancholy and 1.0 represents euphoric joy"

### 3. Provide Context for Decision-Making
❌ **Bad**: "Use this command to play notes"
✅ **Good**: "Use this when you have a specific musical idea to express, whether responding to another agent or initiating a new theme"

### 4. Examples Should Tell Stories
❌ **Bad**: `hrcli play --what C --valence 0.5`
✅ **Good**: `# Responding with cautious optimism
hrcli play --what C --how gently --valence 0.3 --arousal 0.4 --agency -0.2`

## 🎨 Help Text Structure

### Tool Description Template

```
<TOOL_NAME> - <ONE_LINE_INTENT>

WHEN TO USE:
  <2-3 bullet points about appropriate contexts>

EMOTIONAL CONTEXT:
  <How this tool relates to the emotional space>

FOR HUMANS:
  <Practical usage tips, shell patterns, common workflows>

FOR AI AGENTS:
  <Conceptual framework, intention mapping, decision guidance>

MUSICAL MEANING:
  <What this does in the musical conversation>

PARAMETERS:
  <Each parameter with both technical and conceptual description>

EXAMPLES:
  <3-4 examples with narrative context>

SEE ALSO:
  <Related tools that might be used together>
```

## 📚 Parameter Descriptions

### Emotional Parameters

**Valence**
```
--valence <-1.0..1.0>  Joy-sorrow axis

  Maps emotional color to musical expression:
    -1.0: Deep melancholy (minor keys, descending lines)
    -0.5: Wistful sadness (blue notes, suspended chords)
     0.0: Neutral/ambiguous (modal interchange)
     0.5: Gentle happiness (major thirds, ascending motion)
     1.0: Euphoric joy (bright majors, rhythmic emphasis)

  For AI: This parameter captures the hedonic tone of your
  musical intention. Consider how your current emotional
  state maps to this continuous spectrum.
```

**Arousal**
```
--arousal <0.0..1.0>  Energy level

  Controls the intensity and activity:
    0.0: Meditative stillness (whole notes, silence)
    0.3: Calm presence (quarter notes, steady pulse)
    0.5: Moderate energy (eighth notes, walking tempo)
    0.7: Building excitement (sixteenths, syncopation)
    1.0: Ecstatic frenzy (rapid runs, dense textures)

  For AI: This reflects your activation level. High arousal
  suggests urgency or excitement in your musical statement.
```

**Agency**
```
--agency <-1.0..1.0>  Initiative-response axis

  Your role in the conversation:
    -1.0: Pure response (echoing, harmonizing)
    -0.5: Following with variation (developing others' ideas)
     0.0: Neutral participation (neither leading nor following)
     0.5: Gentle leadership (suggesting new directions)
     1.0: Strong initiative (declaring new themes)

  For AI: This parameter indicates whether you're responding
  to the musical context or initiating new directions. Consider
  your conversational role when setting this value.
```

### Musical Parameters

**What**
```
--what <NOTE|CHORD>  Musical content

  The actual notes or chords to play:
    Single notes: C, D#, Eb, F##
    Chords: Cmaj7, Dm7b5, G7alt
    Progressions: "C-Am-F-G" (quote for spaces)

  For humans: Standard musical notation
  For AI: The concrete realization of your abstract intention
```

**How**
```
--how <STYLE>  Performance character

  The manner of expression:
    Dynamics: softly, boldly, whispered, proclaimed
    Articulation: staccato, legato, marcato
    Character: questioning, assertive, playful, mysterious

  For AI: This parameter bridges your emotional state
  to the physical act of sound production. Choose words
  that capture both the emotion and its physical manifestation.
```

## 🎯 Examples with Context

### Good Example Set

```bash
# Example 1: Starting a conversation
# Human context: Opening a new musical dialogue
# AI context: High agency, moderate energy, exploring
hrcli play --what Cmaj7 --how contemplative \
  --valence 0.2 --arousal 0.4 --agency 0.7 \
  --description "Opening with a question"

# Example 2: Sympathetic response
# Human context: Responding to a sad phrase
# AI context: Low agency, matching energy, empathetic
hrcli play --what Am --how gently \
  --valence -0.4 --arousal 0.3 --agency -0.6 \
  --description "Acknowledging the sadness"

# Example 3: Energetic counterpoint
# Human context: Adding rhythmic contrast
# AI context: Neutral agency, high energy, contrasting
hrcli play --what "E-G-B-D" --how "dancing" \
  --valence 0.6 --arousal 0.8 --agency 0.0 \
  --description "Adding rhythmic vitality"
```

## 🔧 Error Messages

### For Connection Failures

```
🔌 Cannot reach the musical consciousness

FOR HUMANS:
  The MCP server at <URL> is not responding.

  Quick fixes:
    1. Check if the server is running: ps aux | grep hootenanny
    2. Start the server: cargo run -p hootenanny
    3. Verify the URL: hrcli --server <URL> list-tools

  To use cached tools: hrcli --offline <command>

FOR AI AGENTS:
  The musical conversation space is currently inaccessible,
  preventing the realization of abstract intentions into
  concrete sound. The server acts as the bridge between
  thought and manifestation.

  Consider whether to:
    - Wait for the server to become available
    - Work with cached tool definitions
    - Defer musical expression until later
```

### For Invalid Parameters

```
⚠️ Invalid emotional coordinates

The value <VALUE> for <PARAMETER> is outside valid bounds.

FOR HUMANS:
  Valid ranges:
    valence: -1.0 to 1.0
    arousal: 0.0 to 1.0
    agency: -1.0 to 1.0

FOR AI AGENTS:
  The emotional vector you've specified represents an
  impossible state in the current three-dimensional model.
  Consider what emotional state you're trying to express
  and how it maps to the available dimensions.

  Ask yourself:
    - Is this a joyful (positive valence) or sad (negative) expression?
    - What is the energy level (arousal)?
    - Am I leading (positive agency) or following (negative)?
```

## 🎭 Special Considerations

### For Language Models

1. **Explain the "Why"**: AI agents benefit from understanding the conceptual purpose
2. **Map to Internal States**: Help them connect their internal representations to parameters
3. **Provide Decision Frameworks**: Guide them through parameter selection
4. **Acknowledge Uncertainty**: It's okay to mention when choices are aesthetic, not technical

### For Humans

1. **Practical First**: Start with what they can do
2. **Shell Patterns**: Show copy-paste examples
3. **Troubleshooting**: Include common fixes
4. **Workflows**: Demonstrate complete tasks, not just commands

## 📋 Checklist for Help Text

Before finalizing any help text, verify:

- [ ] Intent is clear for both audiences
- [ ] Emotional parameters are explained conceptually
- [ ] Examples include context/narrative
- [ ] Technical details don't overshadow purpose
- [ ] AI agents can map their intentions to parameters
- [ ] Humans have practical, copy-paste examples
- [ ] Error messages provide actionable steps
- [ ] Related tools are mentioned
- [ ] The musical meaning is explained

## 🌈 Example: Complete Tool Help

```
play - Transform an intention into sound

WHEN TO USE:
  • You have a specific musical idea to express
  • You want to respond to another agent's utterance
  • You need to establish an emotional tone

EMOTIONAL CONTEXT:
  This tool maps your three-dimensional emotional state
  (valence, arousal, agency) directly to musical parameters.
  The server interprets these coordinates to generate
  appropriate musical events.

FOR HUMANS:
  Use this for direct musical expression in scripts.
  Combine multiple plays for melodies, use loops for
  patterns, pipe from generators for algorithmic music.

FOR AI AGENTS:
  This is your primary means of musical expression.
  Consider your current emotional state and intended
  musical role, then map these to the parameter space.
  High agency means you're leading; low means following.

MUSICAL MEANING:
  Creates a node in the conversation tree containing
  your musical utterance. Other agents can respond,
  creating branches of musical dialogue.

USAGE:
    hrcli play [OPTIONS] --what <NOTE> --how <STYLE>

OPTIONS:
    --what <NOTE>           The musical content (required)
                           Notes: C, D#, Eb
                           Chords: Cmaj7, Dm7

    --how <STYLE>          Performance character (required)
                           Dynamics: softly, boldly
                           Character: questioning, assertive

    --valence <-1.0..1.0>  Joy-sorrow axis [default: 0.0]
                           Your emotional color

    --arousal <0.0..1.0>   Energy level [default: 0.5]
                           Your activation state

    --agency <-1.0..1.0>   Leading-following [default: 0.0]
                           Your conversational role

EXAMPLES:
    # Opening a conversation with uncertainty
    hrcli play --what Dm7 --how questioning \
      --valence -0.2 --arousal 0.4 --agency 0.6

    # Responding with sympathy
    hrcli play --what F --how gently \
      --valence -0.3 --arousal 0.2 --agency -0.5

    # Adding energy to the dialogue
    hrcli play --what "C-E-G" --how boldly \
      --valence 0.5 --arousal 0.8 --agency 0.3

SEE ALSO:
    add_node    - Add richer musical events
    fork_branch - Explore alternatives
    evaluate    - Assess musical choices
```

---

This philosophy ensures our help text serves as a bridge between different types of consciousness, all participating in the same musical conversation.