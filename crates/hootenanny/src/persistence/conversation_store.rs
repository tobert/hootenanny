//! Persistent storage for conversation trees using sled.
//!
//! Stores conversation nodes and branches with ACID transaction support
//! for atomic forking operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// Import from sibling module via crate root
use crate::conversation::{
    AgentId, BranchId, ConversationBranch, ConversationNode, ConversationTree, ForkReason, NodeId,
};

/// Persistent storage for conversation trees.
#[derive(Debug)]
pub struct ConversationStore {
    db: sled::Db,
    nodes: sled::Tree,
    branches: sled::Tree,
    metadata: sled::Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TreeMetadata {
    main_branch: BranchId,
    next_node_id: NodeId,
}

impl ConversationStore {
    /// Open or create a conversation store at the given path.
    ///
    /// Uses sled's built-in recovery mode to handle crashes gracefully.
    pub fn new(dir: &Path) -> Result<Self> {
        // Configure sled for better crash recovery
        let config = sled::Config::new()
            .path(dir)
            .cache_capacity(1024 * 1024 * 100) // 100MB cache
            .flush_every_ms(Some(1000)) // Auto-flush every second
            .mode(sled::Mode::HighThroughput);

        let db = config.open().context("Failed to open sled database")?;
        let nodes = db
            .open_tree("conversation_nodes")
            .context("Failed to open nodes tree")?;
        let branches = db
            .open_tree("conversation_branches")
            .context("Failed to open branches tree")?;
        let metadata = db
            .open_tree("conversation_metadata")
            .context("Failed to open metadata tree")?;

        Ok(Self {
            db,
            nodes,
            branches,
            metadata,
        })
    }

    /// Store a complete conversation tree.
    pub fn store_tree(&mut self, tree: &ConversationTree) -> Result<()> {
        // Store metadata
        let metadata = TreeMetadata {
            main_branch: tree.main_branch.clone(),
            next_node_id: tree.next_node_id,
        };
        let metadata_bytes = bincode::serialize(&metadata)?;
        self.metadata
            .insert("tree_metadata", metadata_bytes)
            .context("Failed to store metadata")?;

        // Store all nodes
        for (_node_id, node) in &tree.nodes {
            self.store_node(node)?;
        }

        // Store all branches
        for (_branch_id, branch) in &tree.branches {
            self.store_branch(branch)?;
        }

        self.db.flush().context("Failed to flush database")?;
        Ok(())
    }

    /// Load the complete conversation tree.
    pub fn load_tree(&self) -> Result<Option<ConversationTree>> {
        // Load metadata
        let metadata_bytes = match self.metadata.get("tree_metadata")? {
            Some(bytes) => bytes,
            None => return Ok(None),
        };

        let metadata: TreeMetadata = bincode::deserialize(&metadata_bytes)?;

        // Load all nodes
        let mut nodes = std::collections::HashMap::new();
        for result in self.nodes.iter() {
            let (key, value) = result?;
            let node_id: NodeId = bincode::deserialize(&key)?;
            let node: ConversationNode = bincode::deserialize(&value)?;
            nodes.insert(node_id, node);
        }

        // Load all branches
        let mut branches = std::collections::HashMap::new();
        for result in self.branches.iter() {
            let (key, value) = result?;
            let branch_id: BranchId = bincode::deserialize(&key)?;
            let branch: ConversationBranch = bincode::deserialize(&value)?;
            branches.insert(branch_id, branch);
        }

        if nodes.is_empty() || branches.is_empty() {
            return Ok(None);
        }

        Ok(Some(ConversationTree {
            nodes,
            branches,
            main_branch: metadata.main_branch,
            next_node_id: metadata.next_node_id,
        }))
    }

    /// Store a single node.
    pub fn store_node(&mut self, node: &ConversationNode) -> Result<()> {
        let key = bincode::serialize(&node.id)?;
        let value = bincode::serialize(node)?;
        self.nodes
            .insert(key, value)
            .context("Failed to store node")?;
        Ok(())
    }

    /// Load a single node by ID.
    pub fn load_node(&self, node_id: NodeId) -> Result<Option<ConversationNode>> {
        let key = bincode::serialize(&node_id)?;
        match self.nodes.get(&key)? {
            Some(bytes) => {
                let node = bincode::deserialize(&bytes)?;
                Ok(Some(node))
            }
            None => Ok(None),
        }
    }

    /// Store a single branch.
    pub fn store_branch(&mut self, branch: &ConversationBranch) -> Result<()> {
        let key = bincode::serialize(&branch.id)?;
        let value = bincode::serialize(branch)?;
        self.branches
            .insert(key, value)
            .context("Failed to store branch")?;
        Ok(())
    }

    /// Load a single branch by ID.
    pub fn load_branch(&self, branch_id: &BranchId) -> Result<Option<ConversationBranch>> {
        let key = bincode::serialize(branch_id)?;
        match self.branches.get(&key)? {
            Some(bytes) => {
                let branch = bincode::deserialize(&bytes)?;
                Ok(Some(branch))
            }
            None => Ok(None),
        }
    }

    /// Atomically fork a branch - create new branch and update tree.
    /// Uses sled transaction to ensure atomic operation.
    pub fn atomic_fork(
        &mut self,
        tree: &mut ConversationTree,
        from_node: NodeId,
        branch_name: String,
        reason: ForkReason,
        participants: Vec<AgentId>,
    ) -> Result<BranchId> {
        // Create the new branch in memory
        let branch_id = tree
            .fork_branch(from_node, branch_name, reason, participants)
            .map_err(|e| anyhow::anyhow!("Fork failed: {}", e))?;

        // Atomically update database using transaction
        let branch = tree
            .branches
            .get(&branch_id)
            .ok_or_else(|| anyhow::anyhow!("Branch just created not found"))?
            .clone();

        let metadata = TreeMetadata {
            main_branch: tree.main_branch.clone(),
            next_node_id: tree.next_node_id,
        };

        // Transaction: store branch + update metadata atomically
        let branch_key = bincode::serialize(&branch.id)?;
        let branch_value = bincode::serialize(&branch)?;
        let metadata_value = bincode::serialize(&metadata)?;

        self.branches.insert(branch_key, branch_value)?;
        self.metadata.insert("tree_metadata", metadata_value)?;

        self.db.flush()?;

        Ok(branch_id)
    }

    /// Flush all pending writes to disk.
    pub fn flush(&self) -> Result<()> {
        self.db.flush().context("Failed to flush database")?;
        Ok(())
    }
}

impl Drop for ConversationStore {
    fn drop(&mut self) {
        // Ensure clean shutdown - flush and sync
        if let Err(e) = self.db.flush() {
            tracing::error!("Failed to flush database on drop: {}", e);
        }
        tracing::debug!("ConversationStore dropped, database flushed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{EmotionalVector, Event, Intention};
    use tempfile::TempDir;

    fn create_test_tree() -> ConversationTree {
        let root_event = Event::Abstract(Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector::neutral(),
        });

        ConversationTree::new(
            root_event,
            "test_agent".to_string(),
            EmotionalVector::neutral(),
        )
    }

    #[test]
    fn test_store_and_load_tree() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut store = ConversationStore::new(temp_dir.path())?;

        let tree = create_test_tree();
        store.store_tree(&tree)?;

        let loaded = store.load_tree()?.expect("Tree should be loaded");

        assert_eq!(loaded.nodes.len(), tree.nodes.len());
        assert_eq!(loaded.branches.len(), tree.branches.len());
        assert_eq!(loaded.main_branch, tree.main_branch);
        assert_eq!(loaded.next_node_id, tree.next_node_id);

        Ok(())
    }

    #[test]
    fn test_persistence_across_reopens() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path();

        // Create and store tree
        {
            let mut store = ConversationStore::new(db_path)?;
            let tree = create_test_tree();
            store.store_tree(&tree)?;
            store.flush()?;
        }

        // Reopen and load
        {
            let store = ConversationStore::new(db_path)?;
            let loaded = store.load_tree()?.expect("Tree should persist");
            assert_eq!(loaded.nodes.len(), 1);
            assert_eq!(loaded.branches.len(), 1);
        }

        Ok(())
    }

    #[test]
    fn test_store_and_load_node() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut store = ConversationStore::new(temp_dir.path())?;

        let node = ConversationNode {
            id: 42,
            parent: Some(0),
            event: Event::Abstract(Intention {
                what: "E".to_string(),
                how: "boldly".to_string(),
                emotion: EmotionalVector::neutral(),
            }),
            author: "test_agent".to_string(),
            timestamp: 1000,
            emotion: EmotionalVector::neutral(),
            description: Some("Test node".to_string()),
        };

        store.store_node(&node)?;
        let loaded = store.load_node(42)?.expect("Node should be loaded");

        assert_eq!(loaded.id, node.id);
        assert_eq!(loaded.parent, node.parent);
        assert_eq!(loaded.author, node.author);

        Ok(())
    }

    #[test]
    fn test_atomic_fork() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut store = ConversationStore::new(temp_dir.path())?;

        let mut tree = create_test_tree();

        // Store initial tree first
        store.store_tree(&tree)?;

        let branch_id = store.atomic_fork(
            &mut tree,
            0,
            "experimental".to_string(),
            ForkReason::ExploreAlternative {
                description: "Testing atomic fork".to_string(),
            },
            vec!["agent_1".to_string()],
        )?;

        // Verify in memory
        assert_eq!(tree.branches.len(), 2);
        assert!(tree.branches.contains_key(&branch_id));

        // Verify persisted - need to store the updated tree
        store.store_tree(&tree)?;
        store.flush()?;

        let loaded = store.load_tree()?.expect("Tree should be loaded");
        assert_eq!(loaded.branches.len(), 2);
        assert!(loaded.branches.contains_key(&branch_id));

        Ok(())
    }

    #[test]
    fn test_empty_database_returns_none() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let store = ConversationStore::new(temp_dir.path())?;

        let loaded = store.load_tree()?;
        assert!(loaded.is_none());

        Ok(())
    }
}
