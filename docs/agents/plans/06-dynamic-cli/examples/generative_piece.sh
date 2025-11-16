#!/bin/bash
# generative_piece.sh - Algorithmic music generation
# Demonstrates using math, randomness, and rules to create music

set -e

echo "🎲 Generative Music Piece"
echo "   Algorithmic composition with emotional evolution"
echo

# Parameters for the generative system
SEED=${1:-$$}  # Use PID as seed if not provided
DURATION=${2:-30}  # Number of musical events
RANDOM=$SEED

echo "  Seed: $SEED"
echo "  Events: $DURATION"
echo

# Musical material pools
NOTES_POOL=("C" "D" "E" "F" "G" "A" "B")
CHROMATIC=("C" "C#" "D" "D#" "E" "F" "F#" "G" "G#" "A" "A#" "B")
MODES=("ionian" "dorian" "phrygian" "lydian" "mixolydian" "aeolian" "locrian")

# Function to generate note based on mode
generate_modal_note() {
    local mode=$1
    local root=$2

    # Define intervals for each mode (from root)
    case $mode in
        ionian)     intervals=(0 2 4 5 7 9 11) ;;  # Major
        dorian)     intervals=(0 2 3 5 7 9 10) ;;
        phrygian)   intervals=(0 1 3 5 7 8 10) ;;
        lydian)     intervals=(0 2 4 6 7 9 11) ;;
        mixolydian) intervals=(0 2 4 5 7 9 10) ;;
        aeolian)    intervals=(0 2 3 5 7 8 10) ;;  # Natural minor
        locrian)    intervals=(0 1 3 5 6 8 10) ;;
    esac

    # Pick random interval from mode
    local interval=${intervals[$((RANDOM % ${#intervals[@]}))]}

    # Find root index
    local root_idx=0
    for i in "${!CHROMATIC[@]}"; do
        if [[ "${CHROMATIC[$i]}" == "$root" ]]; then
            root_idx=$i
            break
        fi
    done

    # Calculate note
    local note_idx=$(( (root_idx + interval) % 12 ))
    echo "${CHROMATIC[$note_idx]}"
}

# Emotional evolution function (sine wave + noise)
calculate_emotion() {
    local step=$1
    local max_steps=$2
    local dimension=$3  # valence, arousal, or agency

    # Base sine wave for smooth evolution
    local phase=$(echo "scale=4; $step / $max_steps * 3.14159 * 2" | bc)

    case $dimension in
        valence)
            # Valence evolves from negative to positive
            base=$(echo "scale=2; s($phase) * 0.5" | bc -l)
            ;;
        arousal)
            # Arousal has faster oscillation
            phase=$(echo "scale=4; $phase * 2" | bc)
            base=$(echo "scale=2; (c($phase) + 1) * 0.3 + 0.2" | bc -l)
            ;;
        agency)
            # Agency alternates between leading and following
            base=$(echo "scale=2; s($phase * 3) * 0.6" | bc -l)
            ;;
    esac

    # Add some controlled randomness
    noise=$(echo "scale=2; ($RANDOM % 20 - 10) / 100" | bc)
    result=$(echo "scale=2; $base + $noise" | bc)

    # Clamp to valid ranges
    if [ "$dimension" == "arousal" ]; then
        # Arousal is 0 to 1
        if (( $(echo "$result < 0" | bc -l) )); then result="0"; fi
        if (( $(echo "$result > 1" | bc -l) )); then result="1"; fi
    else
        # Valence and agency are -1 to 1
        if (( $(echo "$result < -1" | bc -l) )); then result="-1"; fi
        if (( $(echo "$result > 1" | bc -l) )); then result="1"; fi
    fi

    echo "$result"
}

# Create a new branch for this generative piece
echo "🌳 Creating generative piece branch..."
BRANCH=$(hrcli fork_branch \
    --name "generative-$SEED-$(date +%s)" \
    --reason "Algorithmic composition with seed $SEED" \
    --participants "algorithm" \
    | jq -r '.branch_id' 2>/dev/null || echo "generative-branch")

# Select starting mode and root
ROOT=${NOTES_POOL[$((RANDOM % ${#NOTES_POOL[@]}))]}
MODE=${MODES[$((RANDOM % ${#MODES[@]}))]}

echo "  Root: $ROOT"
echo "  Mode: $MODE"
echo

# Generate the piece
echo "🎼 Generating music..."
echo

for ((i=1; i<=DURATION; i++)); do
    # Calculate emotional coordinates
    valence=$(calculate_emotion $i $DURATION "valence")
    arousal=$(calculate_emotion $i $DURATION "arousal")
    agency=$(calculate_emotion $i $DURATION "agency")

    # Generate note based on current mode
    note=$(generate_modal_note "$MODE" "$ROOT")

    # Performance style based on emotions
    if (( $(echo "$arousal > 0.7" | bc -l) )); then
        how="energetic"
    elif (( $(echo "$arousal < 0.3" | bc -l) )); then
        how="gentle"
    elif (( $(echo "$valence > 0.3" | bc -l) )); then
        how="bright"
    elif (( $(echo "$valence < -0.3" | bc -l) )); then
        how="melancholic"
    else
        how="neutral"
    fi

    # Occasionally change mode (10% chance)
    if [ $((RANDOM % 10)) -eq 0 ]; then
        OLD_MODE=$MODE
        MODE=${MODES[$((RANDOM % ${#MODES[@]}))]}
        echo "  [Mode shift: $OLD_MODE → $MODE]"
    fi

    # Occasionally change root (5% chance)
    if [ $((RANDOM % 20)) -eq 0 ]; then
        OLD_ROOT=$ROOT
        ROOT=${NOTES_POOL[$((RANDOM % ${#NOTES_POOL[@]}))]}
        echo "  [Root shift: $OLD_ROOT → $ROOT]"
    fi

    # Progress indicator
    printf "  Event %2d/%d: %s (%s) [v:%+.2f a:%.2f g:%+.2f]\n" \
        $i $DURATION "$note" "$how" $valence $arousal $agency

    # Play the note
    hrcli play \
        --what "$note" \
        --how "$how" \
        --valence "$valence" \
        --arousal "$arousal" \
        --agency "$agency" \
        --agent-id "algorithm" \
        --description "Generative event $i/$DURATION" \
        2>/dev/null

    # Timing based on arousal (higher arousal = faster)
    delay=$(echo "scale=2; 0.5 - $arousal * 0.3" | bc)
    sleep "$delay"

    # Structural markers
    if [ $((i % 8)) -eq 0 ]; then
        echo "  --- Phrase boundary ---"

        # Possibly create variation branch
        if [ $((RANDOM % 3)) -eq 0 ]; then
            echo "  [Exploring variation...]"
            VARIATION=$(hrcli fork_branch \
                --name "variation-$i" \
                --reason "Exploring alternative at measure $((i/8))" \
                --participants "algorithm" \
                2>/dev/null | jq -r '.branch_id' 2>/dev/null)
        fi
    fi
done

# Final resolution
echo
echo "🏁 Resolving to root..."

hrcli play \
    --what "$ROOT" \
    --how "final" \
    --valence 0.0 \
    --arousal 0.2 \
    --agency 0.0 \
    --agent-id "algorithm" \
    --description "Final resolution to $ROOT"

# Analysis
echo
echo "📊 Piece Analysis"
echo "-----------------"
echo "  Seed: $SEED (reproducible with: $0 $SEED $DURATION)"
echo "  Total events: $DURATION"
echo "  Final mode: $MODE"
echo "  Final root: $ROOT"
echo "  Branch: $BRANCH"

# Get statistics
STATS=$(hrcli get_tree_status 2>/dev/null || echo "{}")

echo
echo "Generation complete!"
echo
echo "This piece demonstrated:"
echo "  • Modal note generation"
echo "  • Emotional evolution using sine waves"
echo "  • Probabilistic mode and root changes"
echo "  • Timing variations based on arousal"
echo "  • Reproducible generation with seeds"
echo
echo "To regenerate this exact piece: $0 $SEED $DURATION"