#!/bin/bash
# blues_jam.sh - Interactive blues in E
# Demonstrates call-and-response patterns and blues emotions

set -e

# Blues configuration
KEY="E"
TEMPO=0.4  # Seconds between notes
AGENT_LEAD="blues-lead"
AGENT_BASS="blues-bass"
AGENT_RHYTHM="blues-rhythm"

echo "🎸 Blues Jam in $KEY"
echo "   A musical conversation in the blues tradition"
echo

# Function to play blues note with appropriate emotion
play_blues() {
    local note=$1
    local role=$2  # lead, bass, rhythm
    local intensity=$3  # 0.0 to 1.0

    # Blues always has that melancholy but with varying energy
    local valence=$(echo "scale=2; -0.3 + $intensity * 0.2" | bc)
    local arousal=$(echo "scale=2; 0.3 + $intensity * 0.4" | bc)

    # Agency depends on role
    case $role in
        lead)
            agency="0.7"  # Leading the conversation
            how="wailing"
            ;;
        bass)
            agency="-0.5"  # Supporting role
            how="walking"
            ;;
        rhythm)
            agency="0.0"  # Neutral, keeping time
            how="shuffling"
            ;;
    esac

    hrcli play \
        --what "$note" \
        --how "$how" \
        --valence "$valence" \
        --arousal "$arousal" \
        --agency "$agency" \
        --agent-id "$AGENT_${role^^}" \
        --description "Blues $role: $note"
}

# Start with bass establishing the groove
echo "🎸 Bass establishes the groove..."
for i in {1..4}; do
    play_blues "E" "bass" "0.3"
    sleep $TEMPO
    play_blues "G" "bass" "0.3"
    sleep $TEMPO
done &  # Run in background

BASS_PID=$!

# Rhythm joins with shuffle pattern
echo "🥁 Rhythm adds the shuffle..."
sleep 1  # Let bass establish first
for i in {1..8}; do
    play_blues "E7" "rhythm" "0.5"
    sleep $(echo "scale=2; $TEMPO / 2" | bc)
done &

RHYTHM_PID=$!

# Lead tells the story
echo "🎤 Lead guitar enters with the story..."
sleep 2  # Let the groove establish

# Classic blues lick in E
BLUES_LICK=("E" "G" "A" "Bb" "B" "Bb" "A" "G" "E")

echo
echo ">>> Call (Lead guitar speaks)..."
for note in "${BLUES_LICK[@]}"; do
    intensity=$(echo "scale=2; 0.4 + $RANDOM / 32768 * 0.4" | bc)
    play_blues "$note" "lead" "$intensity"
    sleep $(echo "scale=2; $TEMPO / 2" | bc)
done

# Response pattern - different agent responds
echo
echo "<<< Response (Another voice answers)..."
RESPONSE_LICK=("E" "E" "G" "E" "Bb" "A" "G" "E")

for note in "${RESPONSE_LICK[@]}"; do
    hrcli play \
        --what "$note" \
        --how "responding" \
        --valence -0.4 \
        --arousal 0.5 \
        --agency -0.3 \
        --agent-id "blues-response" \
        --description "Answering the call"
    sleep $(echo "scale=2; $TEMPO / 2" | bc)
done

# Fork for solo exploration
echo
echo "🎸 Forking for solo exploration..."
SOLO_BRANCH=$(hrcli fork_branch \
    --name "blues-solo-$(date +%s)" \
    --reason "Time for an expressive solo" \
    --participants "$AGENT_LEAD" \
    | jq -r '.branch_id' 2>/dev/null || echo "solo-branch")

# Blues solo with building intensity
echo "🔥 Solo with building intensity..."
for intensity in 0.4 0.5 0.6 0.7 0.8 0.9; do
    # Use blue notes and bends
    SOLO_NOTES=("E" "G" "Bb" "B" "D" "E")
    note=${SOLO_NOTES[$((RANDOM % ${#SOLO_NOTES[@]}))]}

    # Intensity affects how we play
    if (( $(echo "$intensity > 0.7" | bc -l) )); then
        how="screaming"
    elif (( $(echo "$intensity > 0.5" | bc -l) )); then
        how="bending"
    else
        how="exploring"
    fi

    hrcli play \
        --what "$note" \
        --how "$how" \
        --valence $(echo "scale=2; -0.2 + $intensity * 0.3" | bc) \
        --arousal "$intensity" \
        --agency "0.8" \
        --agent-id "$AGENT_LEAD" \
        --description "Solo intensity: $intensity"

    sleep $(echo "scale=2; $TEMPO * (1 - $intensity * 0.5)" | bc)
done

# Resolution - return home
echo
echo "🏠 Returning home to E..."

# Everyone plays together for the ending
hrcli play \
    --what "E" \
    --how "together" \
    --valence -0.2 \
    --arousal 0.6 \
    --agency 0.0 \
    --agent-id "ensemble" \
    --description "All voices resolve together"

# Final bass note
sleep 0.5
hrcli play \
    --what "E" \
    --how "final" \
    --valence -0.3 \
    --arousal 0.2 \
    --agency -0.8 \
    --agent-id "$AGENT_BASS" \
    --description "Bass has the last word"

# Clean up background processes
wait $BASS_PID 2>/dev/null || true
wait $RHYTHM_PID 2>/dev/null || true

echo
echo "🎵 Blues jam complete!"
echo
echo "Musical elements demonstrated:"
echo "  • Call and response patterns"
echo "  • Multiple agents with different roles"
echo "  • Blues scale with blue notes (b3, b5, b7)"
echo "  • Building intensity through solo"
echo "  • Background processes for rhythm section"
echo "  • Musical conversation with agency dynamics"