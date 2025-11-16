#!/bin/bash
# emotional_journey.sh - A musical story of emotional transformation
# This script demonstrates how shell scripting can create narrative music

set -e  # Exit on error

# Configuration
SERVER="${HRCLI_SERVER:-http://127.0.0.1:8080}"
AGENT="${HRCLI_AGENT:-storyteller}"

echo "🎭 Starting Emotional Journey..."
echo "   From melancholy to joy through music"
echo

# Act 1: Melancholy Beginning
echo "Act 1: In the depths of sadness..."
for note in Am F C G; do
    hrcli play \
        --what "$note" \
        --how "searching" \
        --valence -0.7 \
        --arousal 0.2 \
        --agency 0.0 \
        --agent-id "$AGENT" \
        --description "Lost in melancholy"
    sleep 0.5
done

# Fork to explore hope
echo
echo "🌱 A glimmer of hope appears..."
HOPE_BRANCH=$(hrcli fork_branch \
    --name "hope-emerges-$(date +%s)" \
    --reason "The melody finds a ray of light" \
    --participants "$AGENT" \
    | jq -r '.branch_id')

echo "   Created branch: $HOPE_BRANCH"

# Act 2: Transitional Exploration
echo
echo "Act 2: Searching for the light..."

# Gradually increase valence (sadness → happiness)
for valence in -0.6 -0.4 -0.2 0.0 0.2 0.4; do
    # Calculate arousal based on valence (more happy = more energy)
    arousal=$(echo "scale=2; 0.3 + ($valence + 1) * 0.2" | bc)

    # Alternate between notes for movement
    if [ $(echo "$valence > 0" | bc) -eq 1 ]; then
        note="E"
        how="brightening"
    else
        note="D"
        how="questioning"
    fi

    hrcli play \
        --what "$note" \
        --how "$how" \
        --valence "$valence" \
        --arousal "$arousal" \
        --agency "$(echo "scale=2; $valence * 0.5" | bc)" \
        --agent-id "$AGENT" \
        --description "Emotional transition at valence $valence"

    sleep 0.3
done

# Act 3: Joy Emerges
echo
echo "Act 3: Joy breaks through!"

# Create a joyful melody
JOYFUL_NOTES=("C" "E" "G" "C" "G" "E" "C")
for i in "${!JOYFUL_NOTES[@]}"; do
    note="${JOYFUL_NOTES[$i]}"

    # Vary the performance
    if [ $((i % 2)) -eq 0 ]; then
        how="joyfully"
    else
        how="dancing"
    fi

    hrcli play \
        --what "$note" \
        --how "$how" \
        --valence 0.8 \
        --arousal 0.7 \
        --agency 0.6 \
        --agent-id "$AGENT" \
        --description "Celebrating newfound joy"

    sleep 0.25
done

# Finale: Resolution
echo
echo "Finale: Peace and resolution"

hrcli play \
    --what "Cmaj7" \
    --how "peacefully" \
    --valence 0.6 \
    --arousal 0.3 \
    --agency 0.0 \
    --agent-id "$AGENT" \
    --description "Finding peace after the journey"

# Evaluate the emotional journey
echo
echo "📊 Evaluating the emotional journey..."
EVALUATION=$(hrcli evaluate_branch --branch "$HOPE_BRANCH")

echo "Journey complete!"
echo
echo "Summary:"
echo "  - Started in deep melancholy (valence: -0.7)"
echo "  - Explored transitional emotions"
echo "  - Discovered joy (valence: 0.8)"
echo "  - Found peaceful resolution (valence: 0.6)"
echo
echo "Branch evaluation:"
echo "$EVALUATION" | jq '.summary' 2>/dev/null || echo "$EVALUATION"