

# **HalfRemembered MCP: A Conceptual Domain Model for Agentic Music Creation**

## **I. The "HalfRemembered" Paradigm: A Conceptual Model for Agentic Music Creation**

### **A. Introduction: From Passive Recorder to Active Collaborator**

The dominant paradigm in digital music creation, the Digital Audio Workstation (DAW), is architecturally modeled as a passive recorder. Its primary function, inherited from multitrack tape, is to faithfully capture, store, and allow the manipulation of human-originated input. This model, while powerful, is analogous to a word processor: it has no inherent understanding of the *content* it contains, whether that content is a musical phrase or a line of text. The DAW is a tool, not a participant; it is an environment for *documentation*, not *collaboration*.1

The "HalfRemembered" Music Creation Platform (MCP) is conceptualized to fundamentally break this paradigm. It is architected from the ground up as an *active collaborator*. The system's architecture is not designed merely to *store* data but to *understand, interpret, and generate* it. The name "HalfRemembered" itself informs this core functional goal: the system is designed to take a user's *fragment*—a "half-remembered" melody, a vague textural idea, an abstract emotional prompt—and, acting as a co-creator, help to "remember" or *realize* the complete musical concept.1

This new, agentic paradigm is built upon three foundational technological pillars, which form the structure of this domain model:

1. **High-Resolution Expressive Data:** The *lingua franca* of the system must be able to represent musical nuance at a level rivaling acoustic performance. This data model must natively capture the continuous, high-resolution, and polyphonic expressive details that define "human" playing.4 This requirement is met by the adoption of the MIDI 2.0 protocol as the system's native data format.  
2. **Agentic Generative Models:** The *creative engine* of the system must be a generative model. This requires moving beyond simple "generative AI" (which merely creates content in response to a prompt) to "agentic AI." An agentic model is capable of multi-step planning, reasoning, and autonomous action within its environment, enabling it to act as a true creative partner.7  
3. **The Universal Timeline:** The *central nervous system* of the architecture. This is a novel temporal model designed to host, manage, and orchestrate the complex, real-time interaction between high-resolution performance data (from humans) and high-level abstract instructions (for AI agents).

### **B. Core Research Challenges and Objectives**

The primary research challenge in designing the "HalfRemembered" MCP is to create a conceptual domain model that treats *human creative intention* and *AI generative instruction* as first-class citizens, co-equal with the *performance data* (e.g., notes, audio) they produce. In a traditional DAW, performance data is the *only* first-class event. Instructions (e.g., "make this part sound 'sadder'") are metadata at best, or exist entirely outside the system in a human's mind. This model cannot support true co-creation.

An analysis of the current technological landscape reveals the components that make this new model feasible. The MIDI 2.0 protocol provides the necessary data resolution and per-note expressive control.9 The CLAP (CLever Audio Plugin) standard provides an open-source, non-proprietary pathway for this high-resolution data to be processed by sound-generating instruments without a "black box" translation layer.11 Conversely, incumbent standards like VST3, which *do not* pass MIDI 2.0 data natively and instead rely on a proprietary translation layer, present a significant architectural bottleneck that would compromise the system's core data integrity.12

The objective of this report is to deliver a complete conceptual blueprint for the "HalfRemembered" MCP. This report will define the core entities of the domain model—the Event, the Stream, and the MusicalContext—and fully specify their attributes and relationships. This model provides the architectural foundation for a new generation of agentic, collaborative, and deeply expressive music creation systems.14

## **II. The Universal Timeline: A Multi-Modal, Event-Centric Stage**

### **A. Deconstructing the "Tick-Based" Sequencer**

The traditional music sequencer is, at its core, a "tick-based" system. This model is fundamentally a two-dimensional matrix, mapping musical pitch against a linear, discrete representation of time (often "pulses per quarter note," or $PPQN$). This paradigm, represented visually by the "piano roll," is excellent for its original purpose: the discrete, step-by-step recording and editing of MIDI 1.0 note data.16 However, for an LLM-centric system, this model is wholly insufficient. It has no native capacity to represent abstract concepts, generative instructions, or branching possibilities.

The "Universal Timeline" is proposed as its replacement. This timeline is not a 2D matrix; it is an *abstract event container*. It is a temporal stage that does not *only* store notes. It is designed to store, sequence, and manage *all creative actions*.18 This unified event space can host, with equal priority:

* Human-generated performances (ConcreteEvents).  
* AI-generated performances (ConcreteEvents).  
* Text-based generative instructions (AbstractEvents).20  
* Musical-theoretical constraints (AbstractEvents).21  
* High-level agent orchestration commands (AbstractEvents).

This unified model, drawing from research in conceptual sequencers 18, is the only way to facilitate a fluid, collaborative workflow where human and AI actions are interleaved and mutually intelligible.

A critical architectural feature of the Universal Timeline, drawn from event-based visualization research 22, is that it is not a single, flat list of events. It is modeled as a "tree of sequences." This hierarchical, branching structure is a cornerstone of the "HalfRemembered" paradigm. It allows the timeline to manage *multiple, branching possibilities* simultaneously. For example, a user can place a generative instruction for an AI agent on the timeline. The agent can respond by generating *five* distinct variations of the requested part.3 In a traditional, linear timeline, these five variations would have to be "muted" or spread across five separate "tracks," cluttering the workspace. In the Universal Timeline, they co-exist as five *branches* of a single event-node. This model directly supports a workflow where the human user's role shifts from "creator" to "arranger" or "curator," selecting, merging, and guiding the "wild creativity" of their AI collaborators.23

Furthermore, the Universal Timeline is designed to manage *both* absolute and relative time. This is a crucial distinction. Absolute time (e.g., wall-clock time) and tick-based time are essential for anchoring concrete performance data, such as a recorded note.24 However, generative instructions are often *musically relative* (e.g., "at the downbeat of bar 16," or "on the fourth chord of the chorus progression").25 The Universal Timeline's domain model allows concrete Note objects to be anchored to an absolute time, while abstract Prompt objects can be anchored to a *musical* time. This bifurcation allows the system to intelligently reinterpret and re-execute generative instructions even if the user makes global changes, such as altering the tempo or time signature of the entire composition.

### **B. Core Entities of the "HalfRemembered" Domain Model**

The Universal Timeline serves as the central orchestration stage for the three primary entities in the "HalfRemembered" domain model. The entire architecture is an expression of the relationships between these three classes:

1. **Event:** The atomic unit of action or data on the timeline. This is the "verb" of the system—the "what happened" or "what to do."  
2. **Stream:** The agentic processor that creates, modifies, or routes Events. This is the "noun" or "actor" of the system, whether human- or AI-driven.  
3. **MusicalContext:** The shared "world model" or knowledge base that informs and constrains the behavior of all Streams. This is the shared "consciousness" of the system.

## **III. The Event Entity: Unifying Performance and Intention**

The Event is the most fundamental entity in the domain model. The central innovation of the "HalfRemembered" architecture is the "Event Duality"—the proposition that the system must manage two co-equal and distinct classes of events: ConcreteEvents (performance data) and AbstractEvents (generative intention).

### **A. The ConcreteEvent: Modeling High-Resolution Performance**

A ConcreteEvent represents *immutable, discrete data of record*. It is the "what happened" in the system, whether that action was a key-press from a human performer or a note generated by an AI agent. To capture the full nuance of musical expression, the data model for ConcreteEvents is based entirely on the MIDI 2.0 specification.

#### **The Note Entity**

This is the primary data structure for discrete musical events. It fully encapsulates the high-resolution data provided by the MIDI 2.0 protocol.9

* **Attributes:**  
  * NoteID: A unique identifier for this specific note instance, used to bind all associated per-note expression data.  
  * Timestamp: A high-resolution timestamp of the note-on message.  
  * Duration: The time between the note-on and note-off messages.  
  * NoteNumber: The pitch of the note.  
  * Velocity: The 16-bit note-on velocity. This provides 65,536 steps of dynamic resolution, a monumental leap from the 128 steps (7-bit) available in MIDI 1.0, which was a primary source of "machine-like" stiffness in digital music.28  
  * ReleaseVelocity: The 16-bit note-off velocity.  
  * ChannelGroup: The UMP (Universal MIDI Packet) group this note belongs to.  
* **Note-On Attributes (Articulation):** The Note entity contains a dedicated field for the MIDI 2.0 Note-On Attribute.9 This represents a paradigm shift in symbolic music production. Historically, articulation information (e.g., *staccato*, *pizzicato*, *legato*, *marcato*) was handled by "keyswitches"—a brittle and archaic workaround where specific, non-sounding notes in the low-register of a keyboard (e.g., C0) were used to trigger a change in the instrument's playing style.29 The MIDI 2.0 Note-On Attribute field *embeds this articulation information directly into the note event itself*.30 This makes the articulation an immutable part of the note's data, eliminating an entire class of workflow friction and making the generative output of AI agents (which can now specify *both* note and articulation) far more robust.

#### **The ExpressionStream Entity**

This entity models the *continuous expressive data* that occurs *during* a Note's lifecycle.33 This is the data that captures human nuance, such as vibrato, pitch bends, or changes in timbre *after* the note has been struck.

The conceptual model for this was proven by MIDI Polyphonic Expression (MPE). MPE was a "clever workaround" 4 that "hacked" the MIDI 1.0 specification to achieve per-note control.5 It did this by assigning each *note* its own MIDI channel (from 2-16), and then using standard *channel-wide* messages on that unique channel.6 MPE defined three primary dimensions of per-note expression 28:

1. **Pitch (X-Axis):** Mapped to Channel Pitch Bend.39  
2. **"Slide" (Y-Axis):** Mapped to Control Change 74\.39  
3. **"Pressure" (Z-Axis):** Mapped to Channel Pressure (Aftertouch).39

MPE was, as one of its originators called it, the "bridge between MIDI 1.0 and MIDI 2.0".36 MIDI 2.0 *nativizes* this concept, making it the foundational solution rather than a workaround.

The ExpressionStream entity is the "HalfRemembered" model's implementation of this concept. It is not a collection of channel-wide CCs; it is a high-resolution (32-bit) data stream of MIDI 2.0 Registered Per-Note Controllers (RPNCs) and Assignable Per-Note Controllers (APNCs).43

* **Attributes:**  
  * ParentNoteID: The foreign key that binds this entire stream of expression to a single Note entity.  
  * TimestampVector: An array of timestamps relative to the parent note's start.  
  * ValueVector: An array of 32-bit values.  
  * ControllerType: An enumeration specifying which controller this stream represents (e.g., RPNC\_01 (Modulation), RPNC\_03 (Pitch 7.25), or an assignable controller).44

This data structure allows for the capture and generation of incredibly complex and detailed musical performances, moving far beyond the simple, discrete note events of the past.

#### **Table 1: Evolution of Per-Note Expression (MPE vs. MIDI 2.0)**

| Feature | MPE (MIDI 1.0 Workaround) | MIDI 2.0 (Native Protocol) | Architectural Implication |
| :---- | :---- | :---- | :---- |
| **Per-Note Method** | Uses a separate MIDI Channel (2-16) per note.6 | Native Per-Note Channel Voice Messages. A single Channel (or Group) handles all notes polyphonically.4 | MIDI 2.0 is exponentially more efficient and scalable, not artificially limited by 15 channels/notes of polyphony. |
| **Pitch Bend** | Uses Channel Pitch Bend message.39 | Native Per-Note Pitch Bend Channel Voice Message.36 | MIDI 2.0 has a dedicated, unambiguous message, not a re-purposed channel message. |
| **"Slide" (Y-Axis)** | Mapped to Control Change 74\.39 | Mapped to a Registered Per-Note Controller (RPNC).44 | Standardized, high-resolution, and part of a discoverable, registered system, not an arbitrary CC number. |
| **"Pressure" (Z-Axis)** | Mapped to Channel Pressure or Polyphonic Aftertouch.39 | Native Per-Note Channel Pressure Channel Voice Message.9 | Higher resolution, native, and unambiguous. |
| **Data Resolution** | 7-bit (CCs) or 14-bit (Pitch Bend).4 | 32-bit for all expressive controllers.10 | An exponential leap in expressive fidelity ($\>4$ billion steps vs. 128 or 16,384). This is crucial for capturing the subtle nuance of AI-generated expression. |
| **Articulation** | No native support. Relies on external MIDI 1.0 keyswitches. | Native Attribute Type and Attribute Data fields in the Note-On message.9 | Solves a decades-old workflow problem by embedding articulation *directly in the note*, making symbolic music data self-describing. |

### **B. The AbstractEvent: Modeling Generative Intent**

This entity is the report's central experimental thesis and the key to the "HalfRemembered" paradigm: *generative instructions are events that exist on the timeline*.

This creates the "Event Duality." The Universal Timeline holds both ConcreteEvents (the "data" or "performance") and AbstractEvents (the "instructions" or "intention"). This duality allows the user and the AI to communicate *through the timeline itself*. Instead of opening a separate "AI" window, the user places an instruction directly into the musical context, and the AI responds *on that same timeline*. This concept is inspired by advanced tangible sequencers that differentiate between event types (e.g., Event, Effect, Pattern tokens) 18 and the distinction in agentic AI between a "generative action" and a simple "event".7

#### **The PromptEvent (Text-to-Symbolic)**

This entity represents a free-form, natural language instruction placed at a specific point on the timeline.47 It is the most direct and intuitive way for a user to express a "half-remembered" idea.

* **Purpose:** To provide a high-level, creative instruction to an AI agent.  
* **Payload Example:** A PromptEvent placed at Bar 16, targeting the "Drummer" agent: "Generate a 4-bar drum fill, starting sparse and becoming much more complex and intense, leading into the chorus at Bar 20".49  
* **Rationale:** Large Language Models (LLMs) have demonstrated a remarkable, if implicit, ability to infer musical structures, temporal relationships, and even emotional color from text-based prompts alone.47 The PromptEvent leverages this capability as a primary workflow.

#### **The ConstraintEvent (Structured Control)**

This is the *critical* counterpart to the PromptEvent. A prompt is "what," but a constraint is "how." LLMs, when trained on text, lack *explicit musical context* 47 and can produce "erratic" or "musically meaningless" output.47 The ConstraintEvent solves this by providing that context *as a structured, machine-readable event*.

* **Purpose:** To place a set of *rules* or *boundaries* over a specific region of the timeline, which *all* generative agents must obey.  
* **Payload Example:** A ConstraintEvent with a duration from Bar 8 to Bar 16, with a JSON payload 54:  
  JSON  
  {  
    "type": "HarmonicConstraint",  
    "key": "C-minor",  
    "scale": "Harmonic",  
    "chord\_progression": \["Cm", "G7", "Cm", "Fm"\]  
  }

* **Rationale:** This entity operationalizes "constraint-based generation" 47 as a direct, user-editable object on the timeline. It moves this critical data from an abstract "prompt" into a concrete "domain model".14

#### **The OrchestrationEvent (Agentic Action)**

This is a higher-level, "agentic" command 7 that instructs an agent to perform a complex *task* that may involve multi-step reasoning 60, analysis of other events, and conditional logic.

* **Purpose:** To trigger complex, multi-stage generative processes.  
* **Payload Example:** "Listen to the 'Melody\_Stream' from Bar 1-8, then generate a harmonized counter-melody on 'Harmony\_Stream' that follows its rhythmic contour but moves in contrary motion".1  
* **Rationale:** This models the AI not as a simple content generator, but as an autonomous agent that can perceive, analyze, plan, and execute complex musical tasks.

#### **Table 2: Conceptual Domain Model for Timeline Events**

| Entity Property | ConcreteEvent (Performance) | AbstractEvent (Intention) |
| :---- | :---- | :---- |
| **Purpose** | A data-of-record. The "what happened." 17 | An instruction or command. The "what to do." 7 |
| **EventType** | Note, ExpressionStream, ContinuousControl | Prompt, Constraint, Orchestration 18 |
| **Timestamp** | Absolute (e.g., 2:01.345) or Tick-based (e.g., 4.3.020) | Absolute, Tick-based, or Musically Relative (e.g., "Start of Chorus") |
| **Payload** | Binary/Numeric Data (e.g., 16-bit Velocity, Articulation ID) 9 | Structured Text (e.g., Natural Language Prompt) or JSON Schema 47 |
| **Target** | A Processor (e.g., a CLAP plugin) 11 | An Agent (e.g., "DrumBot\_LLM" on Stream 2\) 8 |
| **Lifecycle** | Immutable. Once recorded, it is data. | Executable and Volatile. It is consumed by an agent and *results in* the creation of new ConcreteEvents. |

## **IV. The Stream Entity: From Data Lane to Agentic Stream**

### **A. Redefining the "Track" as a "Stream"**

This domain model argues for the deliberate replacement of the term "Track" with "Stream." "Track" is a legacy concept from multitrack tape. It implies a static "lane" or container for data.2 In modern DAWs, a "track" is still just a container for MIDI or audio "clips".47

"Stream," by contrast, implies a dynamic, processing entity. It is a "stream of creative intention".63 This terminology, borrowed from data-flow programming, better aligns with a modern "human-in-the-loop" co-creative process.23 In the "HalfRemembered" model, a Stream is not just a *container*; it is an *actor*. It is an object that can *host* a processor, and that processor can range from a simple I/O router (for human input) to a full-fledged generative AI agent.

### **B. The PerformanceStream (Human-Input)**

This is the simplest Stream type, serving as the most direct analogue to a traditional "MIDI track."

* **Function:** It is configured to ingest ConcreteEvents from a specific human input device, such as a Korg Keystage or Roland A-88MKII MIDI 2.0 controller.65  
* **Logic:** Its logic is simple Input/Output. It receives the high-resolution MIDI 2.0 data stream from the hardware. It then routes these ConcreteEvents to a designated Processor—a sound-generating instrument, which (as will be argued in Section VI) must be a CLAP-based plugin to understand the native MIDI 2.0 data.11 Its primary role is as a data *conduit* for human expression.

### **C. The AgenticStream (AI-Input)**

This is the core of the LLM-centric model. This Stream *is* an AI agent.8

* **Function:** It hosts a generative model, such as a Transformer (like MuseNet), a Diffusion model, or a multi-modal LLM.70  
* **Behavior:** An AgenticStream is an *active* listener. It does not wait for input. It actively *scans* the Universal Timeline, looking for AbstractEvents (like PromptEvents or ConstraintEvents) that are targeted at its AgentID.  
* **Execution Task:** When it detects and is triggered by an AbstractEvent, it executes a generative task chain:  
  1. **Consume:** It consumes the AbstractEvent (e.g., a PromptEvent with the text "make a bassline").47  
  2. **Query:** It queries the MusicalContext entity (see Section V) for the relevant constraints at the event's timestamp (e.g., "Key is C-minor, Chord is G7").14  
  3. **Generate:** It generates a *new* sequence of ConcreteEvents (e.g., MIDI 2.0 notes with velocity, articulation, and per-note expression) that satisfy *both* the creative prompt and the musical constraints.17  
  4. **Publish:** It places these new ConcreteEvents *back onto the timeline*, either on its own Stream or, as discussed, on a new *branch* for user review.22  
* **Collaborative Model:** This model perfectly aligns with the "co-creator" concept, where the AI is described as a "misbehaving rock star" and the human user acts as the "arranger," "producer," or "curator" of this "endless flow of new ideas".23 It also maps to the "Voice Lanes" concept, where the user can *steer* the AI's generation rather than just accepting a non-deterministic output.3

### **D. The EnsembleStream (Collaborative / Real-Time)**

This is the most advanced Stream type, designed for real-time human-AI improvisation.68

* **Function:** Unlike the AgenticStream, which typically responds to *abstract* prompts, the EnsembleStream responds to *concrete* events.  
* **Behavior:** It actively *listens* to the ConcreteEvents being produced by other Streams (human or AI) *in real time*. It analyzes this input (e.g., "the human just played a C-major chord on the PerformanceStream") and generates a musical response (e.g., "play a C-major arpeggio") *immediately*, publishing its own ConcreteEvents back to the timeline with microsecond latency.  
* **Collaborative Model:** This is the model required for systems like Google's "AI Duet" 79 or the "AI-driven visual synthesizer" described in human-AI performance research.68 It is a true, real-time creative partnership, moving beyond turn-based composition into live, improvised performance.

## **V. The MusicalContext Entity: A Domain-Integrated Knowledge Base**

### **A. The Problem: LLMs' Lack of Explicit Musical Context**

This is the single greatest challenge in symbolic music generation. LLMs, trained on vast corpora of text, have proven they can *infer* rudimentary musical patterns and temporal relationships.47 However, they lack *explicit musical context*.51 They do not *know* they are in the key of C-minor unless they are explicitly told. Even when told via a simple text prompt (e.g., "write a melody in C-minor"), this "prompt engineering" is unreliable and often fails, as the model may not perfectly align its "text" understanding with "symbolic music" theory.49 This leads to "musically meaningless" 53 or "erratic" 47 outputs that, while statistically interesting, are artistically useless.

### **B. The Solution: Domain-Integrated Context Engineering (DICE) for Music**

The "HalfRemembered" MCP solves this problem by formalizing this context. The solution is to apply the concept of **Domain-Integrated Context Engineering (DICE)**.14

In this model, the MusicalContext is a persistent, session-wide, structured data object. It is *not* a prompt. It is the *world model* for the entire project. It is the "domain understanding" 14 that all agents, human and AI, share. It provides the essential background knowledge and constraints needed for generative AI to produce relevant, coherent, and accurate content.15

This MusicalContext is populated and dynamically updated by three sources:

1. **Human Input:** The user explicitly setting a global key, tempo, or chord progression.75  
2. **ConstraintEvents:** An AbstractEvent on the timeline can *override* the global context for a specific region (e.g., "this 8-bar chorus section modulates to F-major").  
3. **AI Analysis:** An AgenticStream can be tasked with analyzing a human PerformanceStream, *inferring* the key and chords, and *populating* the MusicalContext itself.75

### **C. MusicalContext Entity: State Attributes**

This entity is a dynamic, time-aware map of the project's music-theoretical properties. Its attributes are not static values but *lists of time-stamped states*.

* **GlobalContext:**  
  * Tempo: A time-stamped map of BPM changes (e.g., \[ { "timestamp": 0.0, "bpm": 120 }, { "timestamp": 64.0, "bpm": 125 } \]).24  
  * TimeSignature: A time-stamped map of meter changes (e.g., \[ { "timestamp": 0.0, "ts": "4/4" }, { "timestamp": 96.0, "ts": "7/8" } \]).53  
* **HarmonicContext (Time-Based Map):**  
  * Key: A time-stamped map of key centers (e.g., \[ { "timestamp": 0.0, "key": "C-minor" }, { "timestamp": 128.0, "key": "Eb-major" } \]).75  
  * Scale: A time-stamped map of available scales (e.g., Natural, Harmonic, Pentatonic).49  
  * ChordProgression: A time-stamped map of the active harmony (e.g., \[ { "timestamp": 0.0, "chord": "Cm" }, { "timestamp": 4.0, "chord": "G7" } \]).49  
* **RhythmicContext:**  
  * GrooveTemplate: A time-stamped map of rhythmic "feel" (e.g., "16th-note swing 55%").  
* **StructuralContext:**  
  * SectionMarkers: A time-stamped list of formal sections (e.g., "Verse", "Chorus", "Bridge").59

### **D. Application in Constrained Generation and Decoding**

This is *how* the entire system works. The MusicalContext is the "secret sauce" that makes the AI agents musically intelligent.

When an AgenticStream is activated at Timestamp\_X (e.g., by a PromptEvent), its generative task is not just "execute prompt." The full task is:

1. **Query MusicalContext:** The agent's first action is to query the MusicalContext entity: "GetActiveContext(Timestamp\_X)".  
2. **Receive Constraints:** The MusicalContext responds with the *active* musical state for that *exact moment* in time: Key: Eb-major, Chord: G7, Section: Chorus.  
3. **Constrain Generation:** This context is then used to *constrain the generative model*. Instead of a "free" text-to-symbolic process, the LLM is forced to operate within a specific, musically-valid token-space.47  
4. **Implementation:** This is implemented at the lowest level via **Constrained Decoding** or **Finite State Machines (FSMs)**.59 As the LLM generates the next token (note), the FSM checks it against the active MusicalContext. If the note "C\#" is generated, the FSM checks: "Is C\# in the scale of Eb-major?" If not, that token is rejected, and the model is forced to *only* choose from the set of valid tokens (notes) that *are* in the key or chord.

This process ensures that all generative output is, by default, musically coherent and *controllable*, solving the LLM's "explicit context" problem.48

## **VI. Architectural & Implementation Recommendations**

### **A. Plugin Standard Mandate: Why CLAP is the Architecturally-Superior Choice**

A domain model based on high-resolution ConcreteEvents (32-bit expression, per-note articulation) is useless if the *sound-generating plugins* at the end of the chain cannot understand this data. This makes the choice of plugin standard an architectural, non-negotiable, foundational decision.

#### **The VST3 Bottleneck**

The incumbent standard, VST3, is the path of least resistance. However, for this specific domain model, it is an architectural *disaster*.

* **VST3 is Not a MIDI API:** VST3 is explicitly an "audio plugin API." Its creators have stated that its misuse as a "MIDI plug-in" in VST2 was not intended.29  
* **The "Black Box" Translation:** VST3 *does not pass MIDI 2.0 data natively* to the plugin. Instead, it is the *host's* (DAW's) duty to *translate* MIDI 2.0 Per-Note Controllers into VST3's own proprietary NoteExpression format.12  
* **Loss of Data Integrity:** This translation layer is a critical bottleneck. It means our perfectly-formatted, MIDI 2.0-native ConcreteEvent data is intercepted, interpreted, and *translated* by the host *before* the plugin ever sees it.13 This breaks the 1:1 data-chain-of-custody. This translation can, as demonstrated with complex MIDI 1.0 CCs, lead to data being lost, re-ordered, or "mangled," making complex, interleaved articulation and expression impossible to reconstruct.29 We lose the guarantee of our 32-bit data's integrity.

#### **The CLAP Solution**

The CLAP (CLever Audio Plugin) standard is the architecturally-correct and only viable choice for the "HalfRemembered" MCP.

* **Natively Designed for MIDI 2.0:** The CLAP standard was *explicitly* "inspired by MPE and MIDI 2.0".11  
* **Native Per-Note Support:** It supports "per-note automation and modulation (in accordance with the recent MIDI 2.0 specifications)".11 It allows polyphonic plug-ins to have their per-voice parameters modulated for individual notes, a feature its creators describe as "MPE on steroids".11 This maps *directly* to our ConcreteEvent and ExpressionStream entities.  
* **Non-Destructive Modulation:** Critically, CLAP supports *non-destructive parameter modulation*, where a modulation is a temporary *offset* to a parameter, not a new absolute *state*.11 This is a perfect 1:1 mapping to our ExpressionStream concept, where a stream of vibrato data is an offset applied to a base Note pitch.

**Architectural Mandate:** The "HalfRemembered MCP" *must* be built on the CLAP standard. Relying on VST3 would create a central architectural bottleneck that filters, translates, and fundamentally degrades the high-resolution, expressive data the entire system is designed to create, manage, and perform.

#### **Table 3: Architectural Analysis of Plugin Standards for Polyphonic Expression**

| Feature | VST3 (NoteExpression) | CLAP (Native Modulation) | Architectural Recommendation for "HalfRemembered" |
| :---- | :---- | :---- | :---- |
| **MIDI 2.0 Handling** | Host-side *translation*. MIDI 2.0 data is converted to proprietary NoteExpression events.12 | Native. "Can adapt to any future MIDI standard".11 | **CLAP.** A translation-less architecture is required to preserve the 1:1 data integrity of our ConcreteEvent model. |
| **Per-Note Data** | NoteExpression (64-bit float).12 Data is *translated from* 32-bit MIDI 2.0.12 | Per-note automation & modulation, "MPE on steroids".11 | **CLAP.** Native support is architecturally superior to a "black box" translation layer, which risks data scrambling or loss of nuance.29 |
| **Modulation Type** | Destructive/State-based. The parameter's value *is* the NoteExpression value. | *Non-destructive* parameter offsets.11 | **CLAP.** Non-destructive modulation is a perfect 1:1 functional match for our ExpressionStream entity. |
| **Developer Effort** | "Plug-in developers do not have to do anything about it".12 | Developers must explicitly support the modulation/MIDI 2.0 extensions. | **CLAP.** While VST3 *seems* easier, it's a "black box" that abstracts away control. CLAP provides the *explicit control* and transparency necessary for an experimental, high-fidelity system. |
| **Architectural Fit** | **Poor.** The translation layer 13 is a critical bottleneck that *abstracts away* and *mangles* the raw MIDI 2.0 data our entire domain model is built upon. | **Excellent.** The entire standard is built around the same core philosophy as our ConcreteEvent model: high-resolution, per-note, polyphonic expression.11 |  |

### **B. The Generative Event-Flow: A System Diagram (Textual Walkthrough)**

This final section will synthesize the entire domain model by walking through a single user story, demonstrating the interaction of all entities.

1. **Context (Set):** The user, beginning a session, sets the MusicalContext.14 The project is now globally aware that it is in **4/4, 120BPM, Key of A-Minor**.  
2. **Event (Concrete):** The user, on a PerformanceStream linked to a MIDI 2.0 controller 65, plays a 4-bar melody. This is their "half-remembered" idea. The system captures this as a series of ConcreteEvent Notes, each with 16-bit velocity and an embedded "Legato" Note-On Attribute.9  
3. **Event (Abstract):** The user, wanting harmony, creates a new AgenticStream named "Pad-Bot".23 At the start of the melody, they place a single PromptEvent on this new stream: "Create a slow, evolving pad texture based on the melody in 'PerformanceStream'".47  
4. **Stream (Agentic Action):** The "Pad-Bot" AgenticStream activates. It *consumes* the PromptEvent.21  
5. **Context (Query):** The agent's first action is to *query* the MusicalContext. It asks, "GetActiveContext(timestamp: 0.0)". It receives the reply: **Key: A-Minor**.75  
6. **Stream (Analysis):** The agent executes the second part of its OrchestrationEvent task: it analyzes the ConcreteEvents on the "PerformanceStream," identifying the specific notes (A, C, E, G, B) and their rhythmic placement.1  
7. **Stream (Generation):** The agent now generates a new set of ConcreteEvent Notes (e.g., long, whole-note chords like Am, G, C, F). It uses **Constrained Decoding** 47, restricting its output *only* to notes that are valid within the A-Minor scale.  
8. **Event (Generation):** The agent does not just generate static notes. To fulfill the "slow, evolving" prompt, it *also* generates ExpressionStream data for each new Note.35 It binds a continuous stream of RPNC\_01 (Per-Note Modulation) data to each note in the chord, each with a slightly different rate, causing the filter cutoff on a synth to evolve independently for each note.44 This is a level of nuance *impossible* with MIDI 1.0.  
9. **Plugin (Playback):** These new ConcreteEvents (Notes \+ Articulations) and their child ExpressionStreams (Per-Note Modulation) are routed from the "Pad-Bot" stream to a CLAP-based synth plugin.11 The plugin *natively* understands all MIDI 2.0 data, and the rich, expressive, harmonically-correct, and polyphonically-modulated pad texture is played back to the user.

## **VII. Concluding Analysis: The Future of the "HalfRemembered" Co-Creator**

This report has defined a conceptual domain model for the "HalfRemembered MCP." This architecture is not a simple "DAW with AI features" 88 but represents a new *creative paradigm* centered on human-AI collaboration.

The central innovation of this model is the **Event Duality**. The co-existence of ConcreteEvents (high-resolution performance data) and AbstractEvents (generative intention and constraints) on a single **Universal Timeline** is the mechanism that enables a truly collaborative workflow. It allows the user to fluidly move between the roles of *performer* (creating ConcreteEvents), *composer* (placing AbstractEvents), and *arranger* (curating the "tree of sequences" generated by the AI).23

This model elevates the AI from a simple "tool" 89 to a "collaborator".64 The **AgenticStream** concept, guided by the explicit **MusicalContext** (DICE), provides a steerable, musically-intelligent partner 3 rather than a non-deterministic content generator.

The technological foundations for this model are non-negotiable and must be adopted at the project's inception:

1. The expressive *granularity* of the **MIDI 2.0** protocol is required to *represent* the data.9  
2. The architectural *fidelity* of the **CLAP plugin standard** is required to *process* and *perform* this data without a lossy, black-box translation layer.11

Ultimately, the "HalfRemembered" MCP, built on this domain model, fulfills its name. The **Universal Timeline** *is* the memory; the **AbstractEvents** are the user's "half-remembered" fragments; and the **AgenticStreams**, guided by the **MusicalContext**, are the collaborative partners who help complete the idea.1 This is the architecture for true human-AI co-creation.

#### **Works cited**

1. PerTok: Expressive Encoding and Modeling of Symbolic Musical Ideas and Variations, accessed November 15, 2025, [https://arxiv.org/html/2410.02060v1](https://arxiv.org/html/2410.02060v1)  
2. Evaluating Human-AI Interaction via Usability, User Experience and Acceptance Measures for MMM-C: A Creative AI System for Music Composition \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2504.14071v1](https://arxiv.org/html/2504.14071v1)  
3. Novice-AI Music Co-Creation via AI-Steering Tools for Deep Generative Models \- Ryan Louie, accessed November 15, 2025, [https://youralien.github.io/files/cococo\_chi2020\_copy.pdf](https://youralien.github.io/files/cococo_chi2020_copy.pdf)  
4. Adopting MIDI 2.0 and MPE Protocol Advancements \- MasteringBOX, accessed November 15, 2025, [https://www.masteringbox.com/learn/midi-2-0-and-mpe-midi-protocols](https://www.masteringbox.com/learn/midi-2-0-and-mpe-midi-protocols)  
5. MIDI 2.0 and You \- Perfect Circuit, accessed November 15, 2025, [https://www.perfectcircuit.com/signal/midi-2-and-you](https://www.perfectcircuit.com/signal/midi-2-and-you)  
6. MIDI Polyphonic Expression (MPE) Specification Adopted\!, accessed November 15, 2025, [https://midi.org/midi-polyphonic-expression-mpe-specification-adopted](https://midi.org/midi-polyphonic-expression-mpe-specification-adopted)  
7. Agentic AI vs. Generative AI \- IBM, accessed November 15, 2025, [https://www.ibm.com/think/topics/agentic-ai-vs-generative-ai](https://www.ibm.com/think/topics/agentic-ai-vs-generative-ai)  
8. What is LLM Orchestration? \- IBM, accessed November 15, 2025, [https://www.ibm.com/think/topics/llm-orchestration](https://www.ibm.com/think/topics/llm-orchestration)  
9. Details about MIDI 2.0, MIDI-CI, Profiles and Property Exchange (Updated June, 2023), accessed November 15, 2025, [https://midi.org/details-about-midi-2-0-midi-ci-profiles-and-property-exchange-updated-june-2023](https://midi.org/details-about-midi-2-0-midi-ci-profiles-and-property-exchange-updated-june-2023)  
10. M2-100-U MIDI 2.0 Specification Overview, Version 1.1, accessed November 15, 2025, [https://amei.or.jp/midistandardcommittee/MIDI2.0/MIDI2.0-DOCS/M2-100-U\_v1-1\_MIDI\_2-0\_Specification\_Overview.pdf](https://amei.or.jp/midistandardcommittee/MIDI2.0/MIDI2.0-DOCS/M2-100-U_v1-1_MIDI_2-0_Specification_Overview.pdf)  
11. CLAP | Clever Audio Plug-in API \- u-he, accessed November 15, 2025, [https://u-he.com/community/clap/](https://u-he.com/community/clap/)  
12. About MIDI in VST 3 \- VST 3 Developer Portal \- GitHub Pages, accessed November 15, 2025, [https://steinbergmedia.github.io/vst3\_dev\_portal/pages/Technical+Documentation/About+MIDI/Index.html](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/About+MIDI/Index.html)  
13. OT: Plugin Format Specific MIDI 2.0 conversion? \- JUCE Forum, accessed November 15, 2025, [https://forum.juce.com/t/ot-plugin-format-specific-midi-2-0-conversion/66200](https://forum.juce.com/t/ot-plugin-format-specific-midi-2-0-conversion/66200)  
14. Context Engineering Needs Domain Understanding | by Rod Johnson \- Medium, accessed November 15, 2025, [https://medium.com/@springrod/context-engineering-needs-domain-understanding-b4387e8e4bf8](https://medium.com/@springrod/context-engineering-needs-domain-understanding-b4387e8e4bf8)  
15. Why Context is Crucial for Effective Generative AI \- qBotica, accessed November 15, 2025, [https://qbotica.com/why-context-is-the-key-to-better-generative-ai/](https://qbotica.com/why-context-is-the-key-to-better-generative-ai/)  
16. MUSPY: A TOOLKIT FOR SYMBOLIC MUSIC GENERATION \- UCSD CSE, accessed November 15, 2025, [https://cseweb.ucsd.edu/\~jmcauley/pdfs/ismir20.pdf](https://cseweb.ucsd.edu/~jmcauley/pdfs/ismir20.pdf)  
17. A Survey of Music Generation in the Context of Interaction \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2402.15294v1](https://arxiv.org/html/2402.15294v1)  
18. Tquencer: A Tangible Musical Sequencer Using Overlays \- Jens Vetter, accessed November 15, 2025, [https://jensvetter.de/media/files/Tequencer-TEI'18.pdf](https://jensvetter.de/media/files/Tequencer-TEI'18.pdf)  
19. (PDF) Tquencer: A Tangible Musical Sequencer Using Overlays \- ResearchGate, accessed November 15, 2025, [https://www.researchgate.net/publication/323765508\_Tquencer\_A\_Tangible\_Musical\_Sequencer\_Using\_Overlays](https://www.researchgate.net/publication/323765508_Tquencer_A_Tangible_Musical_Sequencer_Using_Overlays)  
20. The State of Generative Music | by Christopher Landschoot | Whitebalance \- Medium, accessed November 15, 2025, [https://medium.com/whitebalance/the-state-of-generative-music-0fcb2745baf9](https://medium.com/whitebalance/the-state-of-generative-music-0fcb2745baf9)  
21. Orchestrate agent behavior with generative AI \- Microsoft Copilot Studio, accessed November 15, 2025, [https://learn.microsoft.com/en-us/microsoft-copilot-studio/advanced-generative-actions](https://learn.microsoft.com/en-us/microsoft-copilot-studio/advanced-generative-actions)  
22. Visualization for Human-AI Collaborative Music Composition, accessed November 15, 2025, [https://elib.uni-stuttgart.de/bitstreams/f9fab6be-b6bf-4732-8acd-f1445d5dcf87/download](https://elib.uni-stuttgart.de/bitstreams/f9fab6be-b6bf-4732-8acd-f1445d5dcf87/download)  
23. Human-AI Collaboration Insights from Music Composition, accessed November 15, 2025, [https://iris.unito.it/retrieve/fd062785-090c-4058-8098-17427854af31/2024-CHI.pdf](https://iris.unito.it/retrieve/fd062785-090c-4058-8098-17427854af31/2024-CHI.pdf)  
24. Absolute Memory for Tempo in Musicians and Non-Musicians \- PMC \- PubMed Central, accessed November 15, 2025, [https://pmc.ncbi.nlm.nih.gov/articles/PMC5070877/](https://pmc.ncbi.nlm.nih.gov/articles/PMC5070877/)  
25. Absolute vs relative pitch — my take \- Aaron Wolf, accessed November 15, 2025, [https://blog.wolftune.com/2012/02/absolute-vs-relative-pitch-my-take.html](https://blog.wolftune.com/2012/02/absolute-vs-relative-pitch-my-take.html)  
26. Perfect pitch, explained \- UChicago News \- The University of Chicago, accessed November 15, 2025, [https://news.uchicago.edu/explainer/what-is-perfect-pitch](https://news.uchicago.edu/explainer/what-is-perfect-pitch)  
27. MIDI 2.0 – MIDI.org, accessed November 15, 2025, [https://midi.org/midi-2-0](https://midi.org/midi-2-0)  
28. The ABC Of MPE \- Sound On Sound, accessed November 15, 2025, [https://www.soundonsound.com/sound-advice/mpe-midi-polyphonic-expression](https://www.soundonsound.com/sound-advice/mpe-midi-polyphonic-expression)  
29. VST3 and MIDI CC pitfall \- VST 3 SDK \- Steinberg Forums, accessed November 15, 2025, [https://forums.steinberg.net/t/vst3-and-midi-cc-pitfall/201879](https://forums.steinberg.net/t/vst3-and-midi-cc-pitfall/201879)  
30. Lets discuss MIDI 2.0 | VI-CONTROL, accessed November 15, 2025, [https://vi-control.net/community/threads/lets-discuss-midi-2-0.149244/](https://vi-control.net/community/threads/lets-discuss-midi-2-0.149244/)  
31. What Musicians & Artists need to know about MIDI 2.0, accessed November 15, 2025, [https://midi.org/what-musicians-artists-need-to-know-about-midi-2-0](https://midi.org/what-musicians-artists-need-to-know-about-midi-2-0)  
32. Profiles – MIDI.org, accessed November 15, 2025, [https://midi.org/profiles](https://midi.org/profiles)  
33. Modeling expressiveness in music performance \- dei.unipd, accessed November 15, 2025, [http://www.dei.unipd.it/\~musica/IM/espre.pdf](http://www.dei.unipd.it/~musica/IM/espre.pdf)  
34. The Dynamics of Dynamics: a Model of Musical Expression, accessed November 15, 2025, [http://iro.umontreal.ca/\~pift6080/H09/documents/papers/todd\_dyn.pdf](http://iro.umontreal.ca/~pift6080/H09/documents/papers/todd_dyn.pdf)  
35. Modeling Continuous Aspects of Music Performance: Vibrato and Portamento, accessed November 15, 2025, [https://repository.ubn.ru.nl/bitstream/handle/2066/74795/74795.pdf](https://repository.ubn.ru.nl/bitstream/handle/2066/74795/74795.pdf)  
36. 5 Important MIDI 2.0 Features To Be Aware of in 2023 \- AudioCipher, accessed November 15, 2025, [https://www.audiocipher.com/post/midi-2-0](https://www.audiocipher.com/post/midi-2-0)  
37. MPE, Polyphonic Aftertouch & MIDI 2.0: Are You Using the Correct Expression?, accessed November 15, 2025, [https://www.gearnews.com/mpe-polyphonic-aftertouch-midi-2-0-synth/](https://www.gearnews.com/mpe-polyphonic-aftertouch-midi-2-0-synth/)  
38. MPE: MIDI Polyphonic Expression Explained \- iZotope, accessed November 15, 2025, [https://www.izotope.com/en/learn/midi-polyphonic-expression-explained](https://www.izotope.com/en/learn/midi-polyphonic-expression-explained)  
39. Editing MPE — Ableton Reference Manual Version 11, accessed November 15, 2025, [https://www.ableton.com/en/live-manual/11/editing-mpe/](https://www.ableton.com/en/live-manual/11/editing-mpe/)  
40. MIDI specifications for Multidimensional Polyphonic Expression (MPE) \- Google Docs, accessed November 15, 2025, [https://docs.google.com/document/d/1-26r0pVtVBrZHM6VGA05hpF-ij5xT6aaXY9BfDzyTx8/edit](https://docs.google.com/document/d/1-26r0pVtVBrZHM6VGA05hpF-ij5xT6aaXY9BfDzyTx8/edit)  
41. M1-100-UM MIDI Polyphonic Expression v1.1 \- VCV Community, accessed November 15, 2025, [https://community.vcvrack.com/uploads/short-url/d96VkDiPYCVqyBTk2JOjzv82oAJ.pdf](https://community.vcvrack.com/uploads/short-url/d96VkDiPYCVqyBTk2JOjzv82oAJ.pdf)  
42. MIDI Polyphonic Expression \- midi mpe spec, accessed November 15, 2025, [https://midimpe.neocities.org/rp53spec.pdf](https://midimpe.neocities.org/rp53spec.pdf)  
43. Introducing MIDI 2.0, accessed November 15, 2025, [https://www.soundonsound.com/music-business/introducing-midi-20](https://www.soundonsound.com/music-business/introducing-midi-20)  
44. Universal MIDI Packet (UMP) Format and MIDI 2.0 Protocol v1.1.1, accessed November 15, 2025, [https://amei.or.jp/midistandardcommittee/MIDI2.0/MIDI2.0-DOCS/M2-104-UM\_v1-1-1\_UMP\_and\_MIDI\_2-0\_Protocol\_Specification.pdf](https://amei.or.jp/midistandardcommittee/MIDI2.0/MIDI2.0-DOCS/M2-104-UM_v1-1-1_UMP_and_MIDI_2-0_Protocol_Specification.pdf)  
45. Audio Developer Conference: Support of MIDI2 and MIDI-CI in VST3 ins... \- Sched, accessed November 15, 2025, [https://adc19.sched.com/event/T1Lu/support-of-midi2-and-midi-ci-in-vst3-instruments](https://adc19.sched.com/event/T1Lu/support-of-midi2-and-midi-ci-in-vst3-instruments)  
46. Use generative actions in cloud flows (preview) \- Power Automate \- Microsoft Learn, accessed November 15, 2025, [https://learn.microsoft.com/en-us/power-automate/generative-actions-overview](https://learn.microsoft.com/en-us/power-automate/generative-actions-overview)  
47. Large Language Models' Internal Perception of Symbolic Music \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2507.12808v1](https://arxiv.org/html/2507.12808v1)  
48. Generating Symbolic Music from Natural Language Prompts using an LLM-Enhanced Dataset \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2410.02084v1](https://arxiv.org/html/2410.02084v1)  
49. AI Music Generation Models 2025: Complete Guide to Music AI Tools \- Beatoven.ai, accessed November 15, 2025, [https://www.beatoven.ai/blog/ai-music-generation-models-the-only-guide-you-need/](https://www.beatoven.ai/blog/ai-music-generation-models-the-only-guide-you-need/)  
50. Using a Language Model to Generate Music in Its Symbolic Domain While Controlling Its Perceived Emotion \- IEEE Xplore, accessed November 15, 2025, [https://ieeexplore.ieee.org/iel7/6287639/10005208/10138187.pdf](https://ieeexplore.ieee.org/iel7/6287639/10005208/10138187.pdf)  
51. \[2507.12808\] Large Language Models' Internal Perception of Symbolic Music \- arXiv, accessed November 15, 2025, [https://arxiv.org/abs/2507.12808](https://arxiv.org/abs/2507.12808)  
52. Large Language Models' Internal Perception of Symbolic Music \- arXiv, accessed November 15, 2025, [https://arxiv.org/pdf/2507.12808](https://arxiv.org/pdf/2507.12808)  
53. On the evaluation of generative models in music \- Music Informatics Group \- Georgia Institute of Technology, accessed November 15, 2025, [https://musicinformatics.gatech.edu/wp-content\_nondefault/uploads/2018/11/postprint.pdf](https://musicinformatics.gatech.edu/wp-content_nondefault/uploads/2018/11/postprint.pdf)  
54. Structured Outputs | Gemini API \- Google AI for Developers, accessed November 15, 2025, [https://ai.google.dev/gemini-api/docs/structured-output](https://ai.google.dev/gemini-api/docs/structured-output)  
55. Learning to Generate Structured Output with Schema Reinforcement Learning \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2502.18878v1](https://arxiv.org/html/2502.18878v1)  
56. Practical Techniques to constraint LLM output in JSON format | by Minyang Chen \- Medium, accessed November 15, 2025, [https://mychen76.medium.com/practical-techniques-to-constraint-llm-output-in-json-format-e3e72396c670](https://mychen76.medium.com/practical-techniques-to-constraint-llm-output-in-json-format-e3e72396c670)  
57. Motifs, Phrases, and Beyond: The Modelling of Structure in Symbolic Music Generation, accessed November 15, 2025, [https://arxiv.org/html/2403.07995v1](https://arxiv.org/html/2403.07995v1)  
58. \[2511.07156\] Conditional Diffusion as Latent Constraints for Controllable Symbolic Music Generation \- arXiv, accessed November 15, 2025, [https://arxiv.org/abs/2511.07156](https://arxiv.org/abs/2511.07156)  
59. SymPAC: Scalable Symbolic Music Generation With Prompts And Constraints \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2409.03055v2](https://arxiv.org/html/2409.03055v2)  
60. Can LLMs "Reason" in Music? An Evaluation of LLMs' Capability of Music Understanding and Generation \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2407.21531v1](https://arxiv.org/html/2407.21531v1)  
61. Versatile Symbolic Music-for-Music Modeling via Function Alignment \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2506.15548v1](https://arxiv.org/html/2506.15548v1)  
62. Generating music in the waveform domain \- Sander Dieleman, accessed November 15, 2025, [https://sander.ai/2020/03/24/audio-generation.html](https://sander.ai/2020/03/24/audio-generation.html)  
63. A TEST OF SELECTED ASPECTS OF PETER WEBSTER'S CONCEPTUAL MODEL OF CREATIVE THINKING IN MUSIC Faculty of Music Submitted in parti, accessed November 15, 2025, [https://www.collectionscanada.gc.ca/obj/s4/f2/dsk2/ftp01/MQ28544.pdf](https://www.collectionscanada.gc.ca/obj/s4/f2/dsk2/ftp01/MQ28544.pdf)  
64. Centering Sound Artists in Generative Music | News \- Northwestern Engineering, accessed November 15, 2025, [https://www.mccormick.northwestern.edu/news/articles/2024/02/centering-sound-artists-in-generative-music/](https://www.mccormick.northwestern.edu/news/articles/2024/02/centering-sound-artists-in-generative-music/)  
65. Keystage \- POLY AT MIDI KEYBOARD | KORG (USA), accessed November 15, 2025, [https://www.korg.com/us/products/computergear/keystage/](https://www.korg.com/us/products/computergear/keystage/)  
66. New MIDI 2.0 Products Released-November, 2023, accessed November 15, 2025, [https://midi.org/new-midi-2-0-products-released-november-2023](https://midi.org/new-midi-2-0-products-released-november-2023)  
67. Apps and devices that support the higher resolution & MPE replacement side of MIDI 2.0, accessed November 15, 2025, [https://community.polyexpression.com/t/apps-and-devices-that-support-the-higher-resolution-mpe-replacement-side-of-midi-2-0/1728](https://community.polyexpression.com/t/apps-and-devices-that-support-the-higher-resolution-mpe-replacement-side-of-midi-2-0/1728)  
68. Revival: Collaborative Artistic Creation through Human-AI Interactions in Musical Creativity, accessed November 15, 2025, [https://arxiv.org/html/2503.15498v1](https://arxiv.org/html/2503.15498v1)  
69. MoMusic: A Motion-Driven Human-AI Collaborative Music Composition and Performing System \- AAAI Publications, accessed November 15, 2025, [https://ojs.aaai.org/index.php/AAAI/article/view/26907/26679](https://ojs.aaai.org/index.php/AAAI/article/view/26907/26679)  
70. ChatMusician: Understanding and Generating Music Intrinsically with LLMs \- ACL Anthology, accessed November 15, 2025, [https://aclanthology.org/2024.findings-acl.373.pdf](https://aclanthology.org/2024.findings-acl.373.pdf)  
71. \[2303.08385\] Generating symbolic music using diffusion models \- arXiv, accessed November 15, 2025, [https://arxiv.org/abs/2303.08385](https://arxiv.org/abs/2303.08385)  
72. Inside the Rise of AI as a Creative Partner in Music \- AMW Group, accessed November 15, 2025, [https://www.amworldgroup.com/blog/ai-music-co-production](https://www.amworldgroup.com/blog/ai-music-co-production)  
73. Jukebox | OpenAI, accessed November 15, 2025, [https://openai.com/index/jukebox/](https://openai.com/index/jukebox/)  
74. Generating Music with Data: Application of Deep Learning Models for Symbolic Music Composition \- MDPI, accessed November 15, 2025, [https://www.mdpi.com/2076-3417/13/7/4543](https://www.mdpi.com/2076-3417/13/7/4543)  
75. Autozap: AI Chord Analysis and Composition \- Songzap, accessed November 15, 2025, [https://songzap.app/autozap/](https://songzap.app/autozap/)  
76. MuPT: A Generative Symbolic Music Pretrained Transformer \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2404.06393v4](https://arxiv.org/html/2404.06393v4)  
77. XMusic: Towards a Generalized and Controllable Symbolic Music Generation Framework, accessed November 15, 2025, [https://arxiv.org/html/2501.08809v1](https://arxiv.org/html/2501.08809v1)  
78. AI FOR GOOD LIVE | Human collaboration with an AI Musician \- YouTube, accessed November 15, 2025, [https://www.youtube.com/watch?v=nkrUjge4LJA](https://www.youtube.com/watch?v=nkrUjge4LJA)  
79. Magenta RealTime: An Open-Weights Live Music Model, accessed November 15, 2025, [https://magenta.withgoogle.com/magenta-realtime](https://magenta.withgoogle.com/magenta-realtime)  
80. 'A Wish Worth Making' Chords: AI's Musical Analysis | ReelMind, accessed November 15, 2025, [https://reelmind.ai/blog/a-wish-worth-making-chords-ai-s-musical-analysis](https://reelmind.ai/blog/a-wish-worth-making-chords-ai-s-musical-analysis)  
81. SAGE-Music: Low-Latency Symbolic Music Generation via Attribute-Specialized Key-Value Head Sharing \- ResearchGate, accessed November 15, 2025, [https://www.researchgate.net/publication/396093891\_SAGE-Music\_Low-Latency\_Symbolic\_Music\_Generation\_via\_Attribute-Specialized\_Key-Value\_Head\_Sharing](https://www.researchgate.net/publication/396093891_SAGE-Music_Low-Latency_Symbolic_Music_Generation_via_Attribute-Specialized_Key-Value_Head_Sharing)  
82. An artificial intelligence-based classifier for musical emotion expression in media education, accessed November 15, 2025, [https://pmc.ncbi.nlm.nih.gov/articles/PMC10403192/](https://pmc.ncbi.nlm.nih.gov/articles/PMC10403192/)  
83. A Review of AI Music Generation Models, Datasets, and Evaluation Techniques Milind Uttam Nemade1, accessed November 15, 2025, [https://spast.org/techrep/article/download/5262/537/10498](https://spast.org/techrep/article/download/5262/537/10498)  
84. CRANE: Reasoning with constrained LLM generation \- arXiv, accessed November 15, 2025, [https://arxiv.org/html/2502.09061v3](https://arxiv.org/html/2502.09061v3)  
85. From a users point of view what does VST3 give that VST 2 could not deliver? \- Vi-Control, accessed November 15, 2025, [https://vi-control.net/community/threads/from-a-users-point-of-view-what-does-vst3-give-that-vst-2-could-not-deliver.127579/](https://vi-control.net/community/threads/from-a-users-point-of-view-what-does-vst3-give-that-vst-2-could-not-deliver.127579/)  
86. CLAP: The New CLever Audio Plug-in Format \- InSync \- Sweetwater, accessed November 15, 2025, [https://www.sweetwater.com/insync/clap-the-new-clever-audio-plug-in-format/](https://www.sweetwater.com/insync/clap-the-new-clever-audio-plug-in-format/)  
87. u-he and Bitwig announce CLAP 1.0 \- An open-source plugin standard : r/synthesizers, accessed November 15, 2025, [https://www.reddit.com/r/synthesizers/comments/vcwkit/uhe\_and\_bitwig\_announce\_clap\_10\_an\_opensource/](https://www.reddit.com/r/synthesizers/comments/vcwkit/uhe_and_bitwig_announce_clap_10_an_opensource/)  
88. How Musicians Can (and Should) Use AI—According to Berklee Experts, accessed November 15, 2025, [https://www.berklee.edu/berklee-now/news/how-musicians-can-and-should-use-ai-according-to-berklee-experts](https://www.berklee.edu/berklee-now/news/how-musicians-can-and-should-use-ai-according-to-berklee-experts)  
89. I tried 100 AI Music Tools… These are the ONLY ones worth using \- YouTube, accessed November 15, 2025, [https://www.youtube.com/watch?v=1oj0Usyy\_ds](https://www.youtube.com/watch?v=1oj0Usyy_ds)  
90. How AI is Changing the Music Industry in 2025 \- Yapsody, accessed November 15, 2025, [https://www.yapsody.com/ticketing/blog/ai-in-music-industry-2025/](https://www.yapsody.com/ticketing/blog/ai-in-music-industry-2025/)