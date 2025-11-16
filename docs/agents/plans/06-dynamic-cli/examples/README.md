# Musical Shell Script Examples

These examples demonstrate how the dynamic `hrcli` CLI enables musical composition through shell scripting. Each script showcases different aspects of the musical conversation system.

## 🎵 Examples Overview

### emotional_journey.sh
**Theme**: Narrative musical transformation

Demonstrates:
- Gradual emotional evolution (sadness → joy)
- Fork/branch for exploring hope
- Mathematical emotion transitions
- Narrative structure in code

Usage:
```bash
./emotional_journey.sh
```

### blues_jam.sh
**Theme**: Interactive blues in E with multiple agents

Demonstrates:
- Call-and-response patterns
- Multiple agents with different roles (bass, rhythm, lead)
- Background processes for rhythm section
- Blues scale and blue notes
- Building solo intensity
- Musical conversation dynamics

Usage:
```bash
./blues_jam.sh
```

### ai_collaboration.sh
**Theme**: Multiple AI agents creating music together

Demonstrates:
- Different AI personality profiles
- Turn-taking with agency parameter
- Musical conversation between agents
- Consensus building through harmony
- Each AI maintains unique character

Usage:
```bash
./ai_collaboration.sh
```

### generative_piece.sh
**Theme**: Algorithmic composition with rules

Demonstrates:
- Modal note generation
- Sine wave emotional evolution
- Probabilistic mode/root changes
- Reproducible generation with seeds
- Structural boundaries and variations
- Mathematical music creation

Usage:
```bash
# Generate with random seed
./generative_piece.sh

# Reproduce specific piece
./generative_piece.sh 12345 30
```

## 🎯 Common Patterns

### Emotional Mapping
All scripts use the three-dimensional emotional space:
- **Valence**: Joy (-1) to Sorrow (+1)
- **Arousal**: Calm (0) to Energetic (1)
- **Agency**: Following (-1) to Leading (+1)

### Musical Conversation
Scripts demonstrate conversation dynamics:
- Forking branches for exploration
- Multiple agents with distinct roles
- Call and response patterns
- Building and resolving tension

### Shell Techniques
- Background processes for parallel voices
- Mathematical calculations with `bc`
- JSON parsing with `jq`
- Loops for patterns and progressions
- Functions for reusable musical phrases

## 🚀 Running the Examples

### Prerequisites
1. MCP server running: `cargo run -p hootenanny`
2. Dynamic CLI available: `cargo build -p hrcli`
3. Standard tools: `bash`, `bc`, `jq`

### Environment Variables
```bash
export HRCLI_SERVER="http://127.0.0.1:8080"
export HRCLI_AGENT="your-name"
```

### Making Scripts Executable
```bash
chmod +x *.sh
```

## 💡 Creating Your Own Scripts

### Template Structure
```bash
#!/bin/bash
# your_piece.sh - Description

set -e  # Exit on error

# Configuration
SERVER="${HRCLI_SERVER:-http://127.0.0.1:8080}"
AGENT="${HRCLI_AGENT:-composer}"

# Your musical logic here
hrcli play \
    --what "C" \
    --how "gently" \
    --valence 0.0 \
    --arousal 0.5 \
    --agency 0.0
```

### Tips for Musical Scripting

1. **Use functions for repeated patterns**:
```bash
play_phrase() {
    for note in C E G; do
        hrcli play --what $note "$@"
        sleep 0.25
    done
}
```

2. **Map emotions to musical elements**:
```bash
# Higher valence = major keys
# Lower valence = minor keys
# Higher arousal = faster tempo
# Higher agency = melodic lead
```

3. **Create musical structures**:
```bash
# AABA form
play_section_a
play_section_a
play_section_b
play_section_a
```

4. **Use math for smooth transitions**:
```bash
for i in $(seq 0 0.1 1); do
    valence=$(echo "scale=2; -0.5 + $i" | bc)
    hrcli play --valence $valence ...
done
```

## 🎼 Advanced Techniques

### Parallel Voices
```bash
# Bass line in background
play_bass_line &
BASS_PID=$!

# Melody in foreground
play_melody

# Wait for bass to finish
wait $BASS_PID
```

### Dynamic Timing
```bash
# Tempo based on arousal
delay=$(echo "scale=2; 1.0 - $arousal * 0.5" | bc)
sleep $delay
```

### Branching Narratives
```bash
# Create alternative explorations
BRANCH=$(hrcli fork_branch --name "variation" | jq -r '.branch_id')

# Explore variation
# ...

# Evaluate results
hrcli evaluate_branch --branch "$BRANCH"
```

## 🤝 Contributing Examples

We welcome new example scripts! Consider:
- Demonstrating unused features
- Exploring different musical styles
- Creating interactive pieces
- Building educational examples
- Showing AI/human collaboration patterns

---

These examples show that shell scripting can be a powerful medium for musical expression, especially when combined with the emotional and conversational capabilities of the HalfRemembered MCP system.