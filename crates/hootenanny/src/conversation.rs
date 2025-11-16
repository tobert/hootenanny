//! Conversation tree structures for temporal forking and multi-agent collaboration.
//!
//! Musical conversations are like git repositories - branching, merging, evolving.
//! Each node represents a musical utterance in the ongoing dialogue.

use crate::domain::{EmotionalVector, Event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a conversation node.
pub type NodeId = u64;

/// Unique identifier for an agent participating in the conversation.
pub type AgentId = String;

/// Unique identifier for a conversation branch.
pub type BranchId = String;

/// A node in the conversation tree representing a single musical utterance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationNode {
    /// Unique identifier for this node
    pub id: NodeId,

    /// Parent node (None for root)
    pub parent: Option<NodeId>,

    /// The musical event at this node (Abstract or Concrete)
    pub event: Event,

    /// Which agent created this node
    pub author: AgentId,

    /// When this node was created (Unix timestamp in nanoseconds)
    pub timestamp: u64,

    /// Emotional context at the time of creation
    pub emotion: EmotionalVector,

    /// Optional human-readable description
    pub description: Option<String>,
}

/// A branch in the conversation tree - an alternative exploration path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationBranch {
    /// Unique identifier for this branch
    pub id: BranchId,

    /// Human-readable name
    pub name: String,

    /// The current head (latest node) of this branch
    pub head: NodeId,

    /// Where this branch diverged from
    pub base: NodeId,

    /// Why this branch was created
    pub fork_reason: ForkReason,

    /// Agents working on this branch
    pub participants: Vec<AgentId>,

    /// When this branch was created
    pub created_at: u64,
}

/// Reasons for creating a new branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForkReason {
    /// Exploring an alternative musical idea
    ExploreAlternative {
        description: String,
    },

    /// Agents disagree on direction
    AgentDisagreement {
        agents: Vec<AgentId>,
        description: String,
    },

    /// User explicitly requested a fork
    UserRequest {
        description: String,
    },

    /// Probabilistic exploration of possibilities
    ProbabilisticExploration {
        probability: f32,
        description: String,
    },

    /// Emotional state diverged significantly
    EmotionalDivergence {
        from_emotion: EmotionalVector,
        to_emotion: EmotionalVector,
        description: String,
    },
}

/// The complete conversation tree structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTree {
    /// All nodes in the tree, indexed by ID
    pub nodes: HashMap<NodeId, ConversationNode>,

    /// All branches, indexed by ID
    pub branches: HashMap<BranchId, ConversationBranch>,

    /// The main/default branch
    pub main_branch: BranchId,

    /// Next available node ID
    pub next_node_id: NodeId,
}

impl ConversationTree {
    /// Create a new conversation tree with a root node.
    pub fn new(root_event: Event, author: AgentId, emotion: EmotionalVector) -> Self {
        let root_node = ConversationNode {
            id: 0,
            parent: None,
            event: root_event,
            author: author.clone(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            emotion,
            description: Some("Root of conversation tree".to_string()),
        };

        let main_branch = ConversationBranch {
            id: "main".to_string(),
            name: "main".to_string(),
            head: 0,
            base: 0,
            fork_reason: ForkReason::UserRequest {
                description: "Initial branch".to_string(),
            },
            participants: vec![author],
            created_at: root_node.timestamp,
        };

        let mut nodes = HashMap::new();
        nodes.insert(0, root_node);

        let mut branches = HashMap::new();
        branches.insert("main".to_string(), main_branch);

        Self {
            nodes,
            branches,
            main_branch: "main".to_string(),
            next_node_id: 1,
        }
    }

    /// Add a new node to a branch.
    pub fn add_node(
        &mut self,
        branch_id: &BranchId,
        event: Event,
        author: AgentId,
        emotion: EmotionalVector,
        description: Option<String>,
    ) -> Result<NodeId, String> {
        let branch = self
            .branches
            .get(branch_id)
            .ok_or_else(|| format!("Branch {} not found", branch_id))?;

        let parent_id = branch.head;
        let node_id = self.next_node_id;
        self.next_node_id += 1;

        let node = ConversationNode {
            id: node_id,
            parent: Some(parent_id),
            event,
            author,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
            emotion,
            description,
        };

        self.nodes.insert(node_id, node);

        // Update branch head
        if let Some(branch) = self.branches.get_mut(branch_id) {
            branch.head = node_id;
        }

        Ok(node_id)
    }

    /// Create a new branch forking from a specific node.
    pub fn fork_branch(
        &mut self,
        from_node: NodeId,
        branch_name: String,
        reason: ForkReason,
        participants: Vec<AgentId>,
    ) -> Result<BranchId, String> {
        if !self.nodes.contains_key(&from_node) {
            return Err(format!("Node {} not found", from_node));
        }

        let branch_id = format!("branch_{}", self.branches.len());

        let branch = ConversationBranch {
            id: branch_id.clone(),
            name: branch_name,
            head: from_node,
            base: from_node,
            fork_reason: reason,
            participants,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        };

        self.branches.insert(branch_id.clone(), branch);

        Ok(branch_id)
    }

    /// Get the path from root to a specific node.
    pub fn get_path_to_node(&self, node_id: NodeId) -> Vec<NodeId> {
        let mut path = Vec::new();
        let mut current = node_id;

        while let Some(node) = self.nodes.get(&current) {
            path.push(current);
            match node.parent {
                Some(parent_id) => current = parent_id,
                None => break,
            }
        }

        path.reverse();
        path
    }

    /// Get all children of a node.
    pub fn get_children(&self, node_id: NodeId) -> Vec<NodeId> {
        self.nodes
            .values()
            .filter(|node| node.parent == Some(node_id))
            .map(|node| node.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Intention;

    #[test]
    fn create_conversation_tree() {
        let root_event = Event::Abstract(Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector::neutral(),
        });

        let tree = ConversationTree::new(
            root_event,
            "agent_1".to_string(),
            EmotionalVector::neutral(),
        );

        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.branches.len(), 1);
        assert!(tree.nodes.contains_key(&0));
    }

    #[test]
    fn add_node_to_branch() {
        let root_event = Event::Abstract(Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector::neutral(),
        });

        let mut tree = ConversationTree::new(
            root_event,
            "agent_1".to_string(),
            EmotionalVector::neutral(),
        );

        let second_event = Event::Abstract(Intention {
            what: "E".to_string(),
            how: "boldly".to_string(),
            emotion: EmotionalVector {
                valence: 0.7,
                arousal: 0.8,
                agency: 0.6,
            },
        });

        let node_id = tree
            .add_node(
                &"main".to_string(),
                second_event,
                "agent_2".to_string(),
                EmotionalVector::neutral(),
                Some("Second note in sequence".to_string()),
            )
            .unwrap();

        assert_eq!(node_id, 1);
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes.get(&1).unwrap().parent, Some(0));
    }

    #[test]
    fn fork_creates_new_branch() {
        let root_event = Event::Abstract(Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector::neutral(),
        });

        let mut tree = ConversationTree::new(
            root_event,
            "agent_1".to_string(),
            EmotionalVector::neutral(),
        );

        let branch_id = tree
            .fork_branch(
                0,
                "alternative".to_string(),
                ForkReason::ExploreAlternative {
                    description: "Try minor key".to_string(),
                },
                vec!["agent_3".to_string()],
            )
            .unwrap();

        assert_eq!(tree.branches.len(), 2);
        assert!(tree.branches.contains_key(&branch_id));

        let branch = tree.branches.get(&branch_id).unwrap();
        assert_eq!(branch.base, 0);
        assert_eq!(branch.head, 0);
    }

    #[test]
    fn get_path_to_node() {
        let root_event = Event::Abstract(Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector::neutral(),
        });

        let mut tree = ConversationTree::new(
            root_event,
            "agent_1".to_string(),
            EmotionalVector::neutral(),
        );

        // Add a chain: 0 -> 1 -> 2
        tree.add_node(
            &"main".to_string(),
            Event::Abstract(Intention {
                what: "E".to_string(),
                how: "normally".to_string(),
                emotion: EmotionalVector::neutral(),
            }),
            "agent_1".to_string(),
            EmotionalVector::neutral(),
            None,
        )
        .unwrap();

        tree.add_node(
            &"main".to_string(),
            Event::Abstract(Intention {
                what: "G".to_string(),
                how: "boldly".to_string(),
                emotion: EmotionalVector::neutral(),
            }),
            "agent_1".to_string(),
            EmotionalVector::neutral(),
            None,
        )
        .unwrap();

        let path = tree.get_path_to_node(2);
        assert_eq!(path, vec![0, 1, 2]);
    }

    #[test]
    fn get_children_of_node() {
        let root_event = Event::Abstract(Intention {
            what: "C".to_string(),
            how: "softly".to_string(),
            emotion: EmotionalVector::neutral(),
        });

        let mut tree = ConversationTree::new(
            root_event,
            "agent_1".to_string(),
            EmotionalVector::neutral(),
        );

        // Create two children of root
        tree.add_node(
            &"main".to_string(),
            Event::Abstract(Intention {
                what: "E".to_string(),
                how: "normally".to_string(),
                emotion: EmotionalVector::neutral(),
            }),
            "agent_1".to_string(),
            EmotionalVector::neutral(),
            None,
        )
        .unwrap();

        // Fork and add to new branch (also child of root)
        let branch_id = tree
            .fork_branch(
                0,
                "alternative".to_string(),
                ForkReason::ExploreAlternative {
                    description: "Different melody".to_string(),
                },
                vec!["agent_2".to_string()],
            )
            .unwrap();

        tree.add_node(
            &branch_id,
            Event::Abstract(Intention {
                what: "G".to_string(),
                how: "boldly".to_string(),
                emotion: EmotionalVector::neutral(),
            }),
            "agent_2".to_string(),
            EmotionalVector::neutral(),
            None,
        )
        .unwrap();

        let children = tree.get_children(0);
        assert_eq!(children.len(), 2);
        assert!(children.contains(&1));
        assert!(children.contains(&2));
    }
}
