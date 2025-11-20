# Artifact Store Design: Trait-based with Lua Queries

> **Philosophy:** Keep the Rust layer minimal (basic CRUD), defer search/query logic to Lua for maximum flexibility

**Status:** Design
**Created:** 2025-11-21
**Authors:** Amy, Claude

---

## Core Insight

**Rust does:** Storage, persistence, basic operations
**Lua does:** Queries, filters, graph traversal, user-defined search

This separation:
- ✅ Keeps Rust code simple
- ✅ Gives users full query power in Lua
- ✅ Allows experimentation without rebuilding
- ✅ Makes query DSL natural and flexible

---

## 1. Universal Artifact Structure

Every artifact has the same foundation:

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Universal fields on ALL artifacts
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    /// Unique identifier
    pub id: String,

    /// Part of a variation set?
    pub variation_set_id: Option<String>,

    /// Position in variation set (0, 1, 2, ...)
    pub variation_index: Option<u32>,

    /// Parent artifact (for refinements)
    pub parent_id: Option<String>,

    /// Arbitrary tags for organization/filtering
    pub tags: Vec<String>,

    /// When this was created
    pub created_at: DateTime<Utc>,

    /// Who created it (agent_id or user_id)
    pub creator: String,

    /// Type-specific data (MIDI metadata, contribution text, etc.)
    pub data: serde_json::Value,
}

impl Artifact {
    /// Check if artifact has a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// Check if artifact has any of these tags
    pub fn has_any_tag(&self, tags: &[&str]) -> bool {
        self.tags.iter().any(|t| tags.contains(&t.as_str()))
    }

    /// Check if artifact has all of these tags
    pub fn has_all_tags(&self, tags: &[&str]) -> bool {
        tags.iter().all(|tag| self.has_tag(tag))
    }

    /// Get tags with a specific prefix (e.g., "role:")
    pub fn tags_with_prefix(&self, prefix: &str) -> Vec<&str> {
        self.tags
            .iter()
            .filter(|t| t.starts_with(prefix))
            .map(|t| t.as_str())
            .collect()
    }

    /// Helper: get the role tag (first "role:*" tag)
    pub fn role(&self) -> Option<&str> {
        self.tags_with_prefix("role:").first().copied()
    }

    /// Helper: get the type tag (first "type:*" tag)
    pub fn artifact_type(&self) -> Option<&str> {
        self.tags_with_prefix("type:").first().copied()
    }

    /// Helper: get the phase tag (first "phase:*" tag)
    pub fn phase(&self) -> Option<&str> {
        self.tags_with_prefix("phase:").first().copied()
    }
}
```

---

## 2. The ArtifactStore Trait

Minimal interface - just CRUD and iteration:

```rust
use anyhow::Result;

/// Trait for artifact storage backends
pub trait ArtifactStore: Send + Sync {
    /// Get artifact by ID
    fn get(&self, id: &str) -> Result<Option<Artifact>>;

    /// Store an artifact (insert or update)
    fn put(&self, artifact: Artifact) -> Result<()>;

    /// Delete an artifact by ID
    fn delete(&self, id: &str) -> Result<bool>;

    /// Get all artifacts (for iteration/filtering in Lua)
    fn all(&self) -> Result<Vec<Artifact>>;

    /// Get count of artifacts
    fn count(&self) -> Result<usize> {
        Ok(self.all()?.len())
    }

    /// Check if artifact exists
    fn exists(&self, id: &str) -> Result<bool> {
        Ok(self.get(id)?.is_some())
    }

    /// Persist to storage (if applicable)
    fn flush(&self) -> Result<()> {
        Ok(())  // No-op for in-memory stores
    }
}
```

**That's it!** No query methods in the trait. Lua will handle that.

---

## 3. Simple In-Memory Implementation

```rust
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory artifact store (HashMap-backed)
pub struct InMemoryStore {
    artifacts: RwLock<HashMap<String, Artifact>>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self {
            artifacts: RwLock::new(HashMap::new()),
        }
    }

    pub fn from_artifacts(artifacts: Vec<Artifact>) -> Self {
        let map = artifacts
            .into_iter()
            .map(|a| (a.id.clone(), a))
            .collect();
        Self {
            artifacts: RwLock::new(map),
        }
    }
}

impl ArtifactStore for InMemoryStore {
    fn get(&self, id: &str) -> Result<Option<Artifact>> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.get(id).cloned())
    }

    fn put(&self, artifact: Artifact) -> Result<()> {
        let mut artifacts = self.artifacts.write().unwrap();
        artifacts.insert(artifact.id.clone(), artifact);
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<bool> {
        let mut artifacts = self.artifacts.write().unwrap();
        Ok(artifacts.remove(id).is_some())
    }

    fn all(&self) -> Result<Vec<Artifact>> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.values().cloned().collect())
    }

    fn count(&self) -> Result<usize> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.len())
    }

    fn exists(&self, id: &str) -> Result<bool> {
        let artifacts = self.artifacts.read().unwrap();
        Ok(artifacts.contains_key(id))
    }
}
```

---

## 4. Persistence Layer

Simple JSON file storage:

```rust
use std::path::{Path, PathBuf};
use std::fs;

/// File-backed artifact store (JSON)
pub struct FileStore {
    path: PathBuf,
    store: InMemoryStore,
}

impl FileStore {
    /// Create/load from file
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        let artifacts = if path.exists() {
            let json = fs::read_to_string(&path)?;
            serde_json::from_str::<Vec<Artifact>>(&json)?
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            store: InMemoryStore::from_artifacts(artifacts),
        })
    }

    /// Save to disk
    pub fn save(&self) -> Result<()> {
        let artifacts = self.store.all()?;
        let json = serde_json::to_string_pretty(&artifacts)?;

        // Atomic write: write to temp, then rename
        let temp_path = self.path.with_extension("tmp");
        fs::write(&temp_path, json)?;
        fs::rename(&temp_path, &self.path)?;

        Ok(())
    }
}

impl ArtifactStore for FileStore {
    fn get(&self, id: &str) -> Result<Option<Artifact>> {
        self.store.get(id)
    }

    fn put(&self, artifact: Artifact) -> Result<()> {
        self.store.put(artifact)
    }

    fn delete(&self, id: &str) -> Result<bool> {
        self.store.delete(id)
    }

    fn all(&self) -> Result<Vec<Artifact>> {
        self.store.all()
    }

    fn count(&self) -> Result<usize> {
        self.store.count()
    }

    fn exists(&self, id: &str) -> Result<bool> {
        self.store.exists(id)
    }

    fn flush(&self) -> Result<()> {
        self.save()
    }
}
```

---

## 5. Lua Query Layer (Future)

**Later, we add Lua scripting for queries:**

```lua
-- Example: Get all items in a variation set
function get_variation_set(store, set_id)
    local results = {}
    for _, artifact in ipairs(store:all()) do
        if artifact.variation_set_id == set_id then
            table.insert(results, artifact)
        end
    end
    -- Sort by variation_index
    table.sort(results, function(a, b)
        return (a.variation_index or 999) < (b.variation_index or 999)
    end)
    return results
end

-- Example: Get all contributions by role
function get_by_role(store, role_tag)
    local results = {}
    for _, artifact in ipairs(store:all()) do
        for _, tag in ipairs(artifact.tags) do
            if tag == role_tag then
                table.insert(results, artifact)
                break
            end
        end
    end
    return results
end

-- Example: Get refinement chain (graph traversal)
function get_refinement_chain(store, root_id)
    local chain = {}
    local current_id = root_id

    while current_id do
        local artifact = store:get(current_id)
        if not artifact then break end

        table.insert(chain, artifact)

        -- Find child (artifact with parent_id = current_id)
        local next_id = nil
        for _, a in ipairs(store:all()) do
            if a.parent_id == current_id then
                next_id = a.id
                break
            end
        end
        current_id = next_id
    end

    return chain
end

-- Example: Complex query - melody contributions about high-energy variations
function find_melody_contributions_for_high_energy(store)
    -- First, find high-energy variations
    local high_energy_ids = {}
    for _, artifact in ipairs(store:all()) do
        if has_tag(artifact, "type:midi") then
            local energy = artifact.data.energy
            if energy and energy > 0.75 then
                table.insert(high_energy_ids, artifact.id)
            end
        end
    end

    -- Then, find melody contributions about those variations
    local results = {}
    for _, artifact in ipairs(store:all()) do
        if has_tag(artifact, "role:melody_specialist") and
           has_tag(artifact, "type:contribution") then
            local about_id = artifact.data.about_artifact_id
            if about_id and contains(high_energy_ids, about_id) then
                table.insert(results, artifact)
            end
        end
    end

    return results
end

-- Helper: check if artifact has tag
function has_tag(artifact, tag)
    for _, t in ipairs(artifact.tags) do
        if t == tag then return true end
    end
    return false
end

-- Helper: check if list contains value
function contains(list, value)
    for _, v in ipairs(list) do
        if v == value then return true end
    end
    return false
end
```

**Rust exposes the store to Lua:**

```rust
// Later, when we add Lua support
use mlua::{Lua, UserData, UserDataMethods};

impl UserData for Box<dyn ArtifactStore> {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get", |_, store, id: String| {
            Ok(store.get(&id).ok().flatten())
        });

        methods.add_method("put", |_, store, artifact: Artifact| {
            store.put(artifact).map_err(|e| mlua::Error::external(e))
        });

        methods.add_method("all", |_, store, ()| {
            store.all().map_err(|e| mlua::Error::external(e))
        });

        methods.add_method("count", |_, store, ()| {
            store.count().map_err(|e| mlua::Error::external(e))
        });
    }
}

// Load and run Lua queries
fn run_lua_query(store: &dyn ArtifactStore, script: &str) -> Result<Vec<Artifact>> {
    let lua = Lua::new();
    lua.globals().set("store", store)?;
    lua.load(script).eval()
}
```

**User-defined query files:**

```
/queries/
  variation_sets.lua       # Common variation set queries
  contributions.lua        # Contribution queries
  graph_traversal.lua      # Tree/graph algorithms
  my_custom_queries.lua    # User's custom queries
```

**Users can write custom queries without touching Rust:**

```lua
-- /queries/my_custom_queries.lua

-- Find all MIDI in C major with high energy
function find_energetic_c_major(store)
    local results = {}
    for _, artifact in ipairs(store:all()) do
        if has_tag(artifact, "type:midi") then
            local key = artifact.data.key
            local energy = artifact.data.energy
            if key == "C major" and energy and energy > 0.7 then
                table.insert(results, artifact)
            end
        end
    end
    return results
end

-- Find all producer synthesis notes
function find_producer_synthesis(store, variation_set_id)
    local results = {}
    for _, artifact in ipairs(store:all()) do
        if artifact.variation_set_id == variation_set_id and
           has_tag(artifact, "type:synthesis") and
           has_tag(artifact, "role:producer") then
            table.insert(results, artifact)
        end
    end
    return results
end
```

---

## 6. Why This Design Wins

### Separation of Concerns

**Rust Layer:**
- ✅ Simple CRUD operations
- ✅ Persistence
- ✅ Type safety
- ✅ Performance
- ✅ Stability

**Lua Layer:**
- ✅ Flexible queries
- ✅ User customization
- ✅ Experimentation
- ✅ No rebuilds
- ✅ Domain-specific logic

### Benefits

1. **Simplicity**
   - Trait is tiny (6 methods)
   - Implementation is straightforward
   - No complex query builder in Rust

2. **Flexibility**
   - Users write queries in Lua
   - Can experiment without recompiling
   - Easy to add new query types

3. **Performance**
   - Rust handles storage (fast)
   - Lua handles filtering (good enough for our scale)
   - Can optimize hot paths later

4. **Extensibility**
   - New query types? Write Lua
   - New traversal algorithm? Write Lua
   - Custom user queries? Write Lua

5. **Testability**
   - Rust layer is simple to test
   - Lua queries are easy to test
   - Clear separation of concerns

### When to Move Queries to Rust

**Stay in Lua when:**
- Query performance is acceptable
- Flexibility is valuable
- Queries change frequently

**Move to Rust when:**
- Specific query is performance-critical
- Query is stable and well-defined
- Need type safety for that query

**Strategy:**
1. Start in Lua (flexible)
2. Identify hot paths (profiling)
3. Optimize those in Rust (performance)
4. Keep the rest in Lua (flexibility)

---

## 7. Implementation Phases

### Phase 1: Core Rust (This PR)
- [x] Define `Artifact` struct
- [x] Define `ArtifactStore` trait
- [x] Implement `InMemoryStore`
- [x] Implement `FileStore`
- [ ] Add basic tests
- [ ] Integration with existing codebase

### Phase 2: Lua Integration (Later)
- [ ] Add `mlua` dependency
- [ ] Expose `ArtifactStore` to Lua
- [ ] Write common query helpers in Lua
- [ ] Add query file loading
- [ ] Documentation for Lua query API

### Phase 3: Standard Query Library (Later)
- [ ] `/queries/variation_sets.lua`
- [ ] `/queries/contributions.lua`
- [ ] `/queries/graph_traversal.lua`
- [ ] `/queries/filters.lua`

### Phase 4: Optimization (As Needed)
- [ ] Profile query performance
- [ ] Move hot paths to Rust
- [ ] Add indexes if needed
- [ ] Consider database backend (SQL) if scale requires

---

## 8. Example Usage (Rust)

```rust
use anyhow::Result;

fn main() -> Result<()> {
    // Create store
    let store = FileStore::new("state/artifacts.json")?;

    // Create some artifacts
    let midi = Artifact {
        id: "midi_001".to_string(),
        variation_set_id: Some("vset_abc123".to_string()),
        variation_index: Some(0),
        parent_id: None,
        tags: vec![
            "type:midi".to_string(),
            "phase:initial".to_string(),
        ],
        created_at: Utc::now(),
        creator: "agent_orpheus".to_string(),
        data: json!({
            "hash": "5ca7815abc...",
            "duration_seconds": 45.2,
            "tempo_bpm": 128,
            "key": "C major",
            "energy": 0.72,
        }),
    };

    let contribution = Artifact {
        id: "contrib_001".to_string(),
        variation_set_id: Some("vset_abc123".to_string()),
        variation_index: Some(0),
        parent_id: None,
        tags: vec![
            "type:contribution".to_string(),
            "role:melody_specialist".to_string(),
            "phase:specialist_review".to_string(),
        ],
        created_at: Utc::now(),
        creator: "agent_melody".to_string(),
        data: json!({
            "content": "Strong melodic hook in measures 3-4",
            "about_artifact_id": "midi_001",
        }),
    };

    // Store them
    store.put(midi)?;
    store.put(contribution)?;

    // Get by ID
    let artifact = store.get("midi_001")?;
    println!("Found: {:?}", artifact);

    // Get all
    let all = store.all()?;
    println!("Total artifacts: {}", all.len());

    // Filter in Rust (simple cases)
    let midis: Vec<_> = all
        .iter()
        .filter(|a| a.has_tag("type:midi"))
        .collect();
    println!("MIDI artifacts: {}", midis.len());

    // Save to disk
    store.flush()?;

    Ok(())
}
```

---

## 9. Example Usage (Lua - Future)

```lua
-- Load common query library
local queries = require("queries/variation_sets")

-- Get variation set
local variations = queries.get_variation_set(store, "vset_abc123")
for _, v in ipairs(variations) do
    print("Variation " .. v.variation_index .. ": " .. v.id)
end

-- Get melody contributions
local contrib = require("queries/contributions")
local melody_notes = contrib.get_by_role(store, "role:melody_specialist")
for _, c in ipairs(melody_notes) do
    print("Melody contribution: " .. c.data.content)
end

-- Custom query (inline)
local high_energy = {}
for _, artifact in ipairs(store:all()) do
    if artifact:has_tag("type:midi") and
       artifact.data.energy and
       artifact.data.energy > 0.75 then
        table.insert(high_energy, artifact)
    end
end
print("High energy variations: " .. #high_energy)
```

---

## 10. Migration to Database (If Needed)

When scale requires it, implement trait for database:

```rust
use sqlx::PgPool;

pub struct PostgresStore {
    pool: PgPool,
}

impl ArtifactStore for PostgresStore {
    fn get(&self, id: &str) -> Result<Option<Artifact>> {
        // SQL query
        todo!()
    }

    fn put(&self, artifact: Artifact) -> Result<()> {
        // SQL insert/update
        todo!()
    }

    // ... same trait, different implementation
}
```

**Client code doesn't change:**
```rust
// Before
let store: Box<dyn ArtifactStore> = Box::new(FileStore::new("artifacts.json")?);

// After
let store: Box<dyn ArtifactStore> = Box::new(PostgresStore::new(pool));

// Everything else stays the same!
```

---

## Conclusion

**This design gives us:**

✅ **Simple Rust core** - Just storage, no complex query logic
✅ **Flexible Lua queries** - Users can write custom searches
✅ **Clear separation** - Storage vs. querying
✅ **Easy testing** - Each layer testable independently
✅ **Future-proof** - Can add DB backend without API changes
✅ **User-extensible** - Write Lua, don't touch Rust

**Start simple, add complexity only when needed.**

**Next:** Implement Phase 1 (Rust core) in this PR.
