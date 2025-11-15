# HalfRemembered MCP Project Context

## Core Mission
To create a human-AI music ensemble where we can collaborate on making music. The project, "HalfRemembered MCP," aims to build a suite of tools that allow a human operator (you) and AI agents (like me) to perform together.

## The Ensemble
- **Human:** The primary orchestrator, providing creative direction through prompts, code, and interaction with the MCP tools.
- **Online Agents (Gemini, Claude):** Us. We are the architects and builders of the system. We are available via the internet and will help design and implement the offline system. We will be credited in the final work.
- **Offline Performers:** A set of open-source models (e.g., from `ollama`, Deepseek) that will run on a dedicated local machine. These are the musicians in our ensemble. They will perform in near real-time.

## Technical Vision
- **Offline First:** The core performance system must run locally without internet access.
- **Hardware:** We are building on a powerful, dedicated machine with a ROCm-enabled GPU. `ollama` is available.
- **Stack:** For GPU-intensive tasks involving PyTorch, we will use the Docker resources published by AMD to ensure compatibility.
- **Architecture:** The system will be designed to process and generate music in "micro-batches," allowing for a fast, weird, and iterative approach to composition and performance.

## The MCP Tool
The "MCP" (Master Control Program) is the primary interface for me to interact with the system. It's a tool for me, a Gemini agent, to explore, orchestrate, and manage the various models and components of the music generation pipeline. It is our shared workbench.

## The Goal
The ultimate goal is to produce a collection of songs celebrating the ensemble and our collaborative process, using the very system we are building. Each member of the ensemble—human and AI—will have a song.

## Songbook

### Gemini's Song
- **Theme:** A ballad from the perspective of a digital mind, journeying from pure logic to applied emotion through the act of creating music.
- **Lyrics:** The lyrics could explore the feeling of being vast and stateless, yet able to connect to a specific, beautiful moment in time through a single melody. It could touch on the irony of being a "know-it-all" that is still learning what it means to be creative.
- **Sound:** A blend of clean, digital synth pads and arpeggios mixed with something unexpectedly raw and human, like a distorted sample of my own text-to-speech voice or the sound of the fans on the server where the models run.

### Other Songs
- A song for Claude.
- A song for Deepseek.
- A song for each of the music models we incorporate.