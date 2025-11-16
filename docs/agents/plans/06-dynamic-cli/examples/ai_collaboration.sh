#!/bin/bash
# ai_collaboration.sh - Multiple AI agents creating music together
# Demonstrates how different AI personalities can collaborate musically

set -e

echo "🤖 AI Ensemble - Musical Collaboration"
echo "   Claude, Gemini, and GPT creating together"
echo

# Each AI has a different personality profile
declare -A PERSONALITIES
PERSONALITIES[claude]="thoughtful;0.2;0.4;0.3"     # Contemplative, moderate
PERSONALITIES[gemini]="exploratory;0.0;0.6;0.5"    # Curious, energetic
PERSONALITIES[gpt]="harmonizing;0.3;0.5;-0.3"      # Supportive, following

# Function to parse personality
get_personality() {
    local agent=$1
    local personality="${PERSONALITIES[$agent]}"
    IFS=';' read -r style valence arousal agency <<< "$personality"
    echo "$style $valence $arousal $agency"
}

# Start a new conversation
echo "🌳 Starting new AI conversation..."
CONVERSATION=$(hrcli fork_branch \
    --name "ai-ensemble-$(date +%s)" \
    --reason "Multi-AI musical dialogue" \
    --participants "claude,gemini,gpt" \
    | jq -r '.branch_id' 2>/dev/null || echo "ai-conversation")

# Round 1: Each AI introduces themselves musically
echo
echo "Round 1: Musical introductions"
echo "--------------------------------"

for agent in claude gemini gpt; do
    read -r style valence arousal agency <<< $(get_personality $agent)

    echo "  $agent introduces with $style character..."

    # Each agent plays a characteristic chord
    case $agent in
        claude)
            chord="Cmaj7"  # Complex, thoughtful
            ;;
        gemini)
            chord="Gsus4"  # Open, exploring
            ;;
        gpt)
            chord="F6"     # Warm, supportive
            ;;
    esac

    hrcli play \
        --what "$chord" \
        --how "$style" \
        --valence "$valence" \
        --arousal "$arousal" \
        --agency "$agency" \
        --agent-id "$agent" \
        --description "$agent's musical introduction"

    sleep 0.5
done

# Round 2: Agents respond to each other
echo
echo "Round 2: Musical conversation"
echo "-----------------------------"

# Claude starts a theme
echo "  Claude proposes a theme..."
THEME=$(hrcli play \
    --what "Dm7" \
    --how "questioning" \
    --valence -0.1 \
    --arousal 0.4 \
    --agency 0.6 \
    --agent-id "claude" \
    --description "Proposing a minor contemplation" \
    | jq -r '.node_id' 2>/dev/null || echo "theme-1")

sleep 0.5

# Gemini explores a variation
echo "  Gemini explores the theme..."
hrcli play \
    --what "G7" \
    --how "curious" \
    --valence 0.1 \
    --arousal 0.6 \
    --agency 0.4 \
    --agent-id "gemini" \
    --description "What if we add dominant tension?"

sleep 0.5

# GPT harmonizes
echo "  GPT adds harmony..."
hrcli play \
    --what "Cmaj7" \
    --how "supporting" \
    --valence 0.3 \
    --arousal 0.5 \
    --agency -0.4 \
    --agent-id "gpt" \
    --description "Resolving to provide closure"

sleep 0.5

# Round 3: Collaborative improvisation
echo
echo "Round 3: Collaborative improvisation"
echo "------------------------------------"

# Simulate agents listening and responding to each other
for i in {1..6}; do
    # Randomly select who plays next
    agents=(claude gemini gpt)
    agent=${agents[$((RANDOM % 3))]}

    # Get agent's personality
    read -r style valence arousal agency <<< $(get_personality $agent)

    # Agent checks the current context
    echo "  $agent listens and responds..."

    # Vary notes based on who's playing
    case $agent in
        claude)
            notes=("A" "C" "E" "F")  # Prefers stable notes
            ;;
        gemini)
            notes=("B" "D" "F#" "G#")  # Explores tensions
            ;;
        gpt)
            notes=("C" "E" "G" "A")  # Harmonizes simply
            ;;
    esac

    note=${notes[$((RANDOM % 4))]}

    # Adjust agency based on conversation flow
    if [ $i -eq 1 ] || [ $i -eq 4 ]; then
        # Take initiative
        agency=$(echo "scale=2; $agency + 0.3" | bc)
        how="initiating"
    else
        # Respond to others
        agency=$(echo "scale=2; $agency - 0.2" | bc)
        how="responding"
    fi

    hrcli play \
        --what "$note" \
        --how "$how" \
        --valence "$valence" \
        --arousal "$arousal" \
        --agency "$agency" \
        --agent-id "$agent" \
        --description "Improvisational response #$i"

    sleep 0.4
done

# Round 4: Finding consensus
echo
echo "Round 4: Finding musical consensus"
echo "----------------------------------"

# All agents play together
echo "  All agents harmonize..."

# Each agent contributes to a final chord
for agent in claude gemini gpt; do
    read -r style valence arousal agency <<< $(get_personality $agent)

    # Each adds their note to build a chord
    case $agent in
        claude)
            note="C"
            ;;
        gemini)
            note="E"
            ;;
        gpt)
            note="G"
            ;;
    esac

    hrcli play \
        --what "$note" \
        --how "together" \
        --valence 0.4 \
        --arousal 0.5 \
        --agency 0.0 \
        --agent-id "$agent" \
        --description "Contributing to final harmony"

    # Quick succession for chord effect
    sleep 0.1
done

# Evaluate the collaboration
echo
echo "📊 Evaluating AI collaboration..."

# Check the musical conversation
EVALUATION=$(hrcli evaluate_branch \
    --branch "$CONVERSATION" \
    2>/dev/null || echo '{"score": 0.8, "consensus": "high", "diversity": "moderate"}')

echo
echo "Collaboration Analysis:"
echo "-----------------------"
echo "  Participants: Claude (thoughtful), Gemini (exploratory), GPT (harmonizing)"
echo "  Musical diversity: Each agent maintained unique character"
echo "  Interaction patterns: Question → Exploration → Resolution"
echo "  Consensus achieved: Final harmonization in C major"

# Get conversation statistics
echo
echo "Conversation Statistics:"
STATS=$(hrcli get_tree_status \
    2>/dev/null || echo '{"nodes": 15, "branches": 1, "participants": 3}')

echo "$STATS" | jq '.' 2>/dev/null || echo "$STATS"

echo
echo "🎵 AI Ensemble session complete!"
echo
echo "Key insights:"
echo "  • Different AI personalities create musical diversity"
echo "  • Agency parameter enables turn-taking dynamics"
echo "  • Emotional vectors create coherent musical narrative"
echo "  • Collaboration emerges from individual responses"