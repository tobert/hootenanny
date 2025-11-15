# Research Prompt: The Infinite Canvas

**Recommended Agent:** Gemini
**Justification:** This is a complex, systems-architecture problem requiring deep technical knowledge of Rust, memory layout, and high-performance computing. Gemini's strengths in this area make it the ideal candidate.
**Output Document:** `docs/research/infinite-canvas-storage.md`

---

You are a world-class systems architect designing a storage engine for HalfRemembered, a system for creating music through infinitely branching conversations. Our goal is nothing less than building an infinite canvas for musical exploration, where no idea is ever truly lost and countless parallel futures can be explored simultaneously.

## The Vision: An Infinite Musical Memory

Our central data structure is the `ConversationTree`, a git-like repository for music containing nodes, branches, and events. We want these trees to grow to enormous sizes, holding terabytes of musical possibilities. Our hardware is ready: we have 32GB of main RAM, but also 4TB of fast NVMe SSD storage that we want to treat as a seamless extension of our working memory.

## The Core Challenge: Escaping the Heap

Our current plan involves `sled` for persistence, but we are concerned about the overhead of constantly serializing, deserializing, and copying data between disk and the main RAM heap. For our vision to work, we need near-zero-copy access to our data structures, no matter how large they get.

## Your Task: Design the Storage Architecture

Your mission is to research and propose a high-performance storage architecture in Rust that uses **memory-mapping (`mmap`)** to treat our SSDs as our primary data store.

Please address the following critical questions:

1.  **The `mmap` Feasibility Study:**
    *   How can we effectively use `mmap` in Rust for complex, pointer-heavy data structures like our `ConversationTree` (which contains `HashMap`s and `Vec`s)? Standard `mmap` works on raw bytes; how do we bridge this gap safely?

2.  **The Zero-Copy Serialization Landscape:**
    *   Investigate and compare Rust libraries designed for this purpose. Specifically, analyze:
        *   **`rkyv`**: A pure Rust, zero-copy deserialization framework. How would our `Event` enum and `ConversationNode` struct look in `rkyv`? What are the pros and cons?
        *   **`flatbuffers` / `capnproto`**: How do these compare to `rkyv` for our use case?
        *   Other relevant libraries.

3.  **A Concrete Data Layout Strategy:**
    *   Propose a tangible strategy for laying out our `ConversationTree` on disk. Should we use a single large memory-mapped file? Multiple files? An arena allocation scheme within the mapped region?
    *   How do we handle dynamic allocations (e.g., adding a new node or forking a branch) in this architecture?

4.  **Integration with `sled`:**
    *   Is `sled` still useful in this model? Should it be used to store offsets into the memory-mapped file? Or does this new architecture replace it entirely?

5.  **Safety and Ergonomics:**
    *   What are the `unsafe` implications, and how can we minimize them to present a safe, ergonomic API to the rest of the application?

Your final output should be a detailed technical document that not only explains the chosen approach but provides clear code examples and a strategic rationale, giving our implementation agents a clear path to building a truly infinite musical memory.
