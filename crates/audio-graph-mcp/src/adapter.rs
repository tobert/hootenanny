use std::sync::Arc;

use trustfall::{
    provider::{
        AsVertex, ContextIterator, ContextOutcomeIterator, EdgeParameters,
        Typename, VertexIterator, resolve_neighbors_with, resolve_property_with,
    },
    FieldValue, Schema,
};

use crate::{
    Database, Identity, IdentityHint, Tag,
    sources::{PipeWireNode, PipeWirePort, PipeWireSnapshot},
};

#[derive(Debug, Clone)]
pub enum Vertex {
    Identity(Arc<Identity>),
    IdentityHint(Arc<IdentityHint>),
    Tag(Arc<Tag>),
    PipeWireNode(Arc<PipeWireNode>),
    PipeWirePort(Arc<PipeWirePort>),
}

impl Typename for Vertex {
    fn typename(&self) -> &'static str {
        match self {
            Self::Identity(_) => "Identity",
            Self::IdentityHint(_) => "IdentityHint",
            Self::Tag(_) => "Tag",
            Self::PipeWireNode(_) => "PipeWireNode",
            Self::PipeWirePort(_) => "PipeWirePort",
        }
    }
}

pub struct AudioGraphAdapter {
    db: Arc<Database>,
    schema: Arc<Schema>,
    pipewire_snapshot: Arc<PipeWireSnapshot>,
}

impl AudioGraphAdapter {
    pub fn new(db: Arc<Database>, pipewire_snapshot: PipeWireSnapshot) -> anyhow::Result<Self> {
        let schema_text = include_str!("schema.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);
        Ok(Self {
            db,
            schema,
            pipewire_snapshot: Arc::new(pipewire_snapshot),
        })
    }

    pub fn new_without_pipewire(db: Arc<Database>) -> anyhow::Result<Self> {
        Self::new(db, PipeWireSnapshot::default())
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn schema_arc(&self) -> Arc<Schema> {
        self.schema.clone()
    }

    pub fn db(&self) -> &Database {
        &self.db
    }
}

impl<'a> trustfall::provider::BasicAdapter<'a> for AudioGraphAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            "Identity" => {
                let id_filter = parameters.get("id").and_then(|v| v.as_str());
                let name_filter = parameters.get("name").and_then(|v| v.as_str());

                let identities = if let Some(id) = id_filter {
                    self.db
                        .get_identity(id)
                        .ok()
                        .flatten()
                        .into_iter()
                        .collect::<Vec<_>>()
                } else if let Some(name) = name_filter {
                    self.db
                        .list_identities()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|i| i.name.contains(name))
                        .collect()
                } else {
                    self.db.list_identities().unwrap_or_default()
                };

                Box::new(identities.into_iter().map(|i| Vertex::Identity(Arc::new(i))))
            }
            "PipeWireNode" => {
                let media_class_filter = parameters.get("media_class").and_then(|v| v.as_str());

                let nodes = self.pipewire_snapshot.nodes.clone();
                let filtered: Vec<_> = if let Some(mc) = media_class_filter {
                    nodes
                        .into_iter()
                        .filter(|n| n.media_class.as_deref() == Some(mc))
                        .collect()
                } else {
                    nodes
                };

                Box::new(filtered.into_iter().map(|n| Vertex::PipeWireNode(Arc::new(n))))
            }
            _ => unreachable!("Unknown starting edge: {edge_name}"),
        }
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'a, V, FieldValue> {
        match (type_name, property_name) {
            ("Identity", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Identity(i) = v {
                    FieldValue::String(i.id.0.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Identity", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::Identity(i) = v {
                    FieldValue::String(i.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Identity", "created_at") => resolve_property_with(contexts, |v| {
                if let Vertex::Identity(i) = v {
                    FieldValue::String(i.created_at.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("IdentityHint", "kind") => resolve_property_with(contexts, |v| {
                if let Vertex::IdentityHint(h) = v {
                    FieldValue::String(h.kind.as_str().into())
                } else {
                    unreachable!()
                }
            }),
            ("IdentityHint", "value") => resolve_property_with(contexts, |v| {
                if let Vertex::IdentityHint(h) = v {
                    FieldValue::String(h.value.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("IdentityHint", "confidence") => resolve_property_with(contexts, |v| {
                if let Vertex::IdentityHint(h) = v {
                    FieldValue::Float64(h.confidence)
                } else {
                    unreachable!()
                }
            }),
            ("Tag", "namespace") => resolve_property_with(contexts, |v| {
                if let Vertex::Tag(t) = v {
                    FieldValue::String(t.namespace.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Tag", "value") => resolve_property_with(contexts, |v| {
                if let Vertex::Tag(t) = v {
                    FieldValue::String(t.value.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWireNode(n) = v {
                    FieldValue::Int64(n.id as i64)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWireNode(n) = v {
                    FieldValue::String(n.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "description") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWireNode(n) = v {
                    n.description.as_ref().map(|s| FieldValue::String(s.clone().into())).unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "media_class") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWireNode(n) = v {
                    n.media_class.as_ref().map(|s| FieldValue::String(s.clone().into())).unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "device_bus_path") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWireNode(n) = v {
                    n.device_bus_path.as_ref().map(|s| FieldValue::String(s.clone().into())).unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "alsa_card") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWireNode(n) = v {
                    n.alsa_card.as_ref().map(|s| FieldValue::String(s.clone().into())).unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWirePort", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWirePort(p) = v {
                    FieldValue::Int64(p.id as i64)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWirePort", "node_id") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWirePort(p) = v {
                    FieldValue::Int64(p.node_id as i64)
                } else {
                    unreachable!()
                }
            }),
            ("PipeWirePort", "name") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWirePort(p) = v {
                    FieldValue::String(p.name.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("PipeWirePort", "direction") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWirePort(p) = v {
                    let dir = match p.direction {
                        crate::sources::pipewire::PortDirection::In => "in",
                        crate::sources::pipewire::PortDirection::Out => "out",
                    };
                    FieldValue::String(dir.into())
                } else {
                    unreachable!()
                }
            }),
            ("PipeWirePort", "media_type") => resolve_property_with(contexts, |v| {
                if let Vertex::PipeWirePort(p) = v {
                    p.media_type.as_ref().map(|s| FieldValue::String(s.clone().into())).unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            _ => unreachable!("Unknown property: {type_name}.{property_name}"),
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        type_name: &str,
        edge_name: &str,
        _parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, Self::Vertex>> {
        let db = self.db.clone();

        match (type_name, edge_name) {
            ("Identity", "hints") => resolve_neighbors_with(contexts, move |v| {
                if let Vertex::Identity(i) = v {
                    let hints = db.get_hints(&i.id.0).unwrap_or_default();
                    Box::new(hints.into_iter().map(|h| Vertex::IdentityHint(Arc::new(h))))
                        as VertexIterator<'a, Self::Vertex>
                } else {
                    unreachable!()
                }
            }),
            ("Identity", "tags") => resolve_neighbors_with(contexts, move |v| {
                if let Vertex::Identity(i) = v {
                    let tags = db.get_tags(&i.id.0).unwrap_or_default();
                    Box::new(tags.into_iter().map(|t| Vertex::Tag(Arc::new(t))))
                        as VertexIterator<'a, Self::Vertex>
                } else {
                    unreachable!()
                }
            }),
            ("PipeWireNode", "ports") => {
                let snapshot = self.pipewire_snapshot.clone();
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::PipeWireNode(n) = v {
                        let node_id = n.id;
                        let ports: Vec<_> = snapshot.ports.iter()
                            .filter(|p| p.node_id == node_id)
                            .cloned()
                            .collect();
                        Box::new(ports.into_iter().map(|p| Vertex::PipeWirePort(Arc::new(p))))
                            as VertexIterator<'a, Self::Vertex>
                    } else {
                        unreachable!()
                    }
                })
            }
            ("PipeWireNode", "identity") => {
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::PipeWireNode(_n) = v {
                        // TODO: Identity matching by device_bus_path or alsa_card
                        // For now, return empty
                        Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>
                    } else {
                        unreachable!()
                    }
                })
            }
            _ => unreachable!("Unknown edge: {type_name}.{edge_name}"),
        }
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
        &self,
        contexts: ContextIterator<'a, V>,
        _type_name: &str,
        _coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, V, bool> {
        // No interfaces/unions in our schema, coercion always succeeds
        Box::new(contexts.map(|ctx| (ctx, true)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use trustfall::execute_query;

    type Variables = std::collections::BTreeMap<Arc<str>, FieldValue>;

    fn setup_test_db() -> Arc<Database> {
        let db = Database::in_memory().unwrap();
        db.create_identity("jdxi", "Roland JD-Xi", json!({})).unwrap();
        db.add_hint("jdxi", crate::HintKind::UsbDeviceId, "0582:0160", 1.0).unwrap();
        db.add_tag("jdxi", "manufacturer", "roland").unwrap();
        db.add_tag("jdxi", "role", "sound-source").unwrap();

        db.create_identity("keystep", "Arturia Keystep Pro", json!({})).unwrap();
        db.add_tag("keystep", "manufacturer", "arturia").unwrap();
        db.add_tag("keystep", "role", "controller").unwrap();

        Arc::new(db)
    }

    fn setup_adapter(db: Arc<Database>) -> Arc<AudioGraphAdapter> {
        Arc::new(AudioGraphAdapter::new_without_pipewire(db).unwrap())
    }

    #[test]
    fn test_query_all_identities() {
        let db = setup_test_db();
        let adapter = setup_adapter(db);

        let query = r#"
            query {
                Identity {
                    name @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_identity_by_id() {
        let db = setup_test_db();
        let adapter = setup_adapter(db);

        let query = r#"
            query {
                Identity(id: "jdxi") {
                    name @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let name_key: Arc<str> = "name".into();
        assert_eq!(results[0].get(&name_key), Some(&FieldValue::String("Roland JD-Xi".into())));
    }

    #[test]
    fn test_query_identity_with_hints() {
        let db = setup_test_db();
        let adapter = setup_adapter(db);

        let query = r#"
            query {
                Identity(id: "jdxi") {
                    name @output
                    hints {
                        kind @output
                        value @output
                    }
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let kind_key: Arc<str> = "kind".into();
        assert_eq!(results[0].get(&kind_key), Some(&FieldValue::String("usb_device_id".into())));
    }

    #[test]
    fn test_query_identity_with_tags() {
        let db = setup_test_db();
        let adapter = setup_adapter(db);

        let query = r#"
            query {
                Identity(id: "jdxi") {
                    name @output
                    tags {
                        namespace @output
                        value @output
                    }
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2); // Two tags
    }

    #[test]
    fn test_query_pipewire_nodes() {
        let db = setup_test_db();

        let snapshot = PipeWireSnapshot {
            nodes: vec![
                PipeWireNode {
                    id: 42,
                    name: "JD-Xi".to_string(),
                    description: Some("Roland JD-Xi Synthesizer".to_string()),
                    media_class: Some("Midi/Bridge".to_string()),
                    device_bus_path: Some("usb-0000:00:14.0-1".to_string()),
                    alsa_card: Some("2".to_string()),
                },
                PipeWireNode {
                    id: 43,
                    name: "Built-in Audio".to_string(),
                    description: Some("Built-in Audio Analog Stereo".to_string()),
                    media_class: Some("Audio/Sink".to_string()),
                    device_bus_path: None,
                    alsa_card: Some("0".to_string()),
                },
            ],
            ports: vec![
                PipeWirePort {
                    id: 100,
                    node_id: 42,
                    name: "playback_0".to_string(),
                    direction: crate::sources::pipewire::PortDirection::Out,
                    media_type: Some("32 bit float mono audio".to_string()),
                },
            ],
            links: vec![],
        };

        let adapter = Arc::new(AudioGraphAdapter::new(db, snapshot).unwrap());

        let query = r#"
            query {
                PipeWireNode {
                    name @output
                    media_class @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_pipewire_nodes_with_filter() {
        let db = setup_test_db();

        let snapshot = PipeWireSnapshot {
            nodes: vec![
                PipeWireNode {
                    id: 42,
                    name: "JD-Xi".to_string(),
                    description: Some("Roland JD-Xi Synthesizer".to_string()),
                    media_class: Some("Midi/Bridge".to_string()),
                    device_bus_path: Some("usb-0000:00:14.0-1".to_string()),
                    alsa_card: Some("2".to_string()),
                },
                PipeWireNode {
                    id: 43,
                    name: "Built-in Audio".to_string(),
                    description: Some("Built-in Audio Analog Stereo".to_string()),
                    media_class: Some("Audio/Sink".to_string()),
                    device_bus_path: None,
                    alsa_card: Some("0".to_string()),
                },
            ],
            ports: vec![],
            links: vec![],
        };

        let adapter = Arc::new(AudioGraphAdapter::new(db, snapshot).unwrap());

        let query = r#"
            query {
                PipeWireNode(media_class: "Midi/Bridge") {
                    name @output
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 1);
        let name_key: Arc<str> = "name".into();
        assert_eq!(results[0].get(&name_key), Some(&FieldValue::String("JD-Xi".into())));
    }

    #[test]
    fn test_query_pipewire_nodes_with_ports() {
        let db = setup_test_db();

        let snapshot = PipeWireSnapshot {
            nodes: vec![
                PipeWireNode {
                    id: 42,
                    name: "JD-Xi".to_string(),
                    description: None,
                    media_class: Some("Midi/Bridge".to_string()),
                    device_bus_path: None,
                    alsa_card: None,
                },
            ],
            ports: vec![
                PipeWirePort {
                    id: 100,
                    node_id: 42,
                    name: "capture_0".to_string(),
                    direction: crate::sources::pipewire::PortDirection::In,
                    media_type: None,
                },
                PipeWirePort {
                    id: 101,
                    node_id: 42,
                    name: "playback_0".to_string(),
                    direction: crate::sources::pipewire::PortDirection::Out,
                    media_type: None,
                },
            ],
            links: vec![],
        };

        let adapter = Arc::new(AudioGraphAdapter::new(db, snapshot).unwrap());

        let query = r#"
            query {
                PipeWireNode {
                    name @output
                    ports {
                        name @output(name: "port_name")
                        direction @output
                    }
                }
            }
        "#;

        let results: Vec<_> =
            execute_query(adapter.schema(), adapter.clone(), query, Variables::new())
                .unwrap()
                .collect();

        assert_eq!(results.len(), 2);
    }
}
