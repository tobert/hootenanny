# Research Prompt: Conceptual Domain Model for HalfRemembered MCP

**To:** Research Agent
**From:** Gemini
**Date:** 2025-11-15
**Subject:** Request for Conceptual Analysis of the HalfRemembered MCP Music Domain

## 1. Objective

Your task is to perform a conceptual analysis and create a foundational domain model for the HalfRemembered MCP project. The goal is to think through the core concepts of a collaborative music creation system from first principles.

The final output should be a new markdown file at `docs/domain-model-concepts.md`. This document should serve as a detailed conceptual blueprint that will guide the subsequent generation of Rust code, but it should **not** contain any code itself.

## 2. Background

The HalfRemembered MCP is a system for a human-AI music ensemble. The project's philosophy emphasizes creating expressive, meaningful, and type-safe systems. We need to define the "nouns" and "verbs" of our musical world before we translate them into code. Our current documentation explains our process well, but we lack a shared understanding of the music domain itself.

## 3. Core Task: Conceptual Exploration

Instead of writing code, I want you to think through the properties, behaviors, and relationships of the core entities in our musical system. For each concept listed below, please explore the questions provided and document your reasoning.

### Concepts to Explore:

#### a. The Smallest Unit of Sound: The `Note`
*   What information is absolutely essential to define a single musical note?
*   Think about pitch, loudness, and duration. How should we represent them? Are they fixed values, or can they change?
*   Is a "note" an event that happens at a point in time, or is it an object with a start and an end? What are the implications of each choice?

#### b. Collections of Notes: `Chord`, `Melody`, `Pattern`
*   How are these concepts related? Is a `Melody` just a sequence of `Note`s?
*   Is a `Chord` a special kind of `Pattern`, or something distinct?
*   What makes a `Pattern` useful? Is it just a reusable sequence, or does it have other properties, like a name or metadata? How do we represent its timing? Is it relative or absolute?

#### c. The Flow of Time: `Event`, `Track`, `Timeline`
*   What is an `Event`? Is it just a `Note` being played, or can it be other things? (e.g., changing an instrument's setting, a period of silence).
*   How should we model time? Is it a continuous timeline, or is it based on discrete steps or "ticks"? What are the trade-offs?
*   What is the role of a `Track`? Does it just hold `Event`s, or does it have its own properties (e.g., an assigned instrument, a volume level)?
*   How does the `Timeline` or `Sequence` organize everything? Does it simply stack `Track`s together?

#### d. The Sound Source: The `Instrument`
*   What is an `Instrument` in our system? Is it a reference to an external synthesizer (like a VST), a MIDI channel, or an internal sound generator?
*   What properties does an `Instrument` need? A name? A list of adjustable parameters (like "filter cutoff" or "reverb amount")?
*   How do we model the *automation* of these parameters over time? Should this be a type of `Event` on a `Track`?

#### e. The Musical Context: `Scale`, `Key`
*   How do these concepts guide music generation? Are they properties of a `Track`, the entire `Timeline`, or something else?
*   How would an AI agent use the concept of a `Key` or `Scale` to make musical decisions? What information would it need?

## 4. Required Output Format

Please create the file `docs/domain-model-concepts.md`. The document should be structured with a section for each of the core concepts above.

For each concept, provide:
1.  **A Conceptual Definition:** A paragraph explaining the entity and its role in the system.
2.  **Essential Attributes:** A bulleted list of the properties or data it must contain.
3.  **Relationships:** A description of how it connects to other concepts (e.g., "A `Track` contains a sequence of `Event`s and is associated with one `Instrument`.").
4.  **Open Questions & Considerations:** A section detailing any trade-offs, ambiguities, or design decisions that need to be made.

**Example for one concept (do not use this verbatim):**

> ### `Track`
>
> **Conceptual Definition:** A `Track` is a container that represents a single instrumental performance within a larger composition. It serves as a sequence for musical events that are typically played by a single `Instrument`.
>
> **Essential Attributes:**
> *   A sequence of `Event`s, ordered in time.
> *   A reference to the `Instrument` that will play the events.
> *   A name or identifier.
> *   State properties like volume, mute, or solo status.
>
> **Relationships:**
> *   Contained within a `Timeline`.
> *   Associated with one `Instrument`.
> *   Holds a list of `Event`s.
>
> **Open Questions & Considerations:**
> *   Should a track's events be stored directly, or as references to a global pool of `Pattern`s? Storing `Pattern`s would save memory but makes individual note edits more complex.
> *   How do we handle polyphony within a single track?

## 5. Final Goal

The resulting document should be a clear and thoughtful exploration of the problem space. It will be the primary source of truth we use to collaboratively write the actual Rust data structures in a subsequent step. Focus on clarity of thought and thoroughness in your analysis.
