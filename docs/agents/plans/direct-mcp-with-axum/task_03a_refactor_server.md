# Task 3a: Refactor Server (Split server.rs)

**Objective**: Break up the monolithic `crates/hootenanny/src/server.rs` file (2500+ lines) into logical modules. This makes the codebase more maintainable and prepares for the manual dispatch implementation.

## Steps

1.  **Create Module Structure**
    *   Create `crates/hootenanny/src/api/mod.rs` (public API/Service layer).
    *   Create `crates/hootenanny/src/api/schema.rs` (Move all Request/DTO structs here: `AddNodeRequest`, `ForkRequest`, etc.).
    *   Create `crates/hootenanny/src/api/tools/mod.rs`.
    *   Create individual tool modules:
        *   `crates/hootenanny/src/api/tools/musical.rs` (`play`, `add_node`, `fork_branch`, etc.)
        *   `crates/hootenanny/src/api/tools/cas.rs` (`cas_store`, `cas_inspect`, `upload_file`)
        *   `crates/hootenanny/src/api/tools/orpheus.rs` (`orpheus_generate`, etc.)
        *   `crates/hootenanny/src/api/tools/jobs.rs` (`get_job_status`, `poll`, `cancel_job`)
        *   `crates/hootenanny/src/api/tools/graph.rs` (`graph_query`, `graph_bind`, etc.)

2.  **Refactor `server.rs`**
    *   Rename `server.rs` to `crates/hootenanny/src/api/service.rs` (The `EventDualityServer` struct definition).
    *   Keep `ConversationState` and `EventDualityServer` struct definitions here.
    *   Remove all `#[tool]` implementations from this file.

3.  **Implement Tool Methods in New Modules**
    *   Move the logic from `server.rs` to the respective modules.
    *   **Simplification**: Remove the `#[tool]` macro and `Parameters<T>` wrapper from method signatures.
    *   New signature style:
        ```rust
        impl EventDualityServer {
            pub async fn play(&self, request: AddNodeRequest) -> Result<CallToolResult, McpError> {
                // ... logic ...
            }
        }
        ```

4.  **Update `lib.rs`**
    *   Expose `pub mod api`.

## Success Criteria
*   `server.rs` is significantly smaller (<500 lines).
*   Tool logic is organized by domain in `src/api/tools/`.
*   Project compiles (fixing imports will be the main work here).
