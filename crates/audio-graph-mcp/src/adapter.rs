use std::sync::Arc;

use tokio::sync::RwLock;
use trustfall::{
    provider::{
        AsVertex, ContextIterator, ContextOutcomeIterator, EdgeParameters,
        Typename, VertexIterator, resolve_neighbors_with, resolve_property_with,
    },
    FieldValue, Schema,
};

use crate::{
    Database, Identity, IdentityHint, Tag,
    sources::{AnnotationData, ArtifactData, ArtifactSource, PipeWireNode, PipeWirePort, PipeWireSnapshot, DEFAULT_RECENT_WINDOW},
};

#[derive(Debug, Clone)]
pub enum Vertex {
    Identity(Arc<Identity>),
    IdentityHint(Arc<IdentityHint>),
    Tag(Arc<Tag>),
    PipeWireNode(Arc<PipeWireNode>),
    PipeWirePort(Arc<PipeWirePort>),
    Artifact(Arc<ArtifactData>),
    Annotation(Arc<AnnotationData>),
}

impl Typename for Vertex {
    fn typename(&self) -> &'static str {
        match self {
            Self::Identity(_) => "Identity",
            Self::IdentityHint(_) => "IdentityHint",
            Self::Tag(_) => "Tag",
            Self::PipeWireNode(_) => "PipeWireNode",
            Self::PipeWirePort(_) => "PipeWirePort",
            Self::Artifact(_) => "Artifact",
            Self::Annotation(_) => "Annotation",
        }
    }
}

pub struct AudioGraphAdapter {
    db: Arc<Database>,
    schema: Arc<Schema>,
    pipewire_snapshot: Arc<RwLock<PipeWireSnapshot>>,
    artifact_source: Option<Arc<dyn ArtifactSource>>,
}

impl AudioGraphAdapter {
    pub fn new(db: Arc<Database>, pipewire_snapshot: PipeWireSnapshot) -> anyhow::Result<Self> {
        let schema_text = include_str!("schema.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);
        Ok(Self {
            db,
            schema,
            pipewire_snapshot: Arc::new(RwLock::new(pipewire_snapshot)),
            artifact_source: None,
        })
    }

    pub fn new_without_pipewire(db: Arc<Database>) -> anyhow::Result<Self> {
        Self::new(db, PipeWireSnapshot::default())
    }

    /// Create an adapter with artifact source for full artifact queries.
    pub fn new_with_artifacts(
        db: Arc<Database>,
        pipewire_snapshot: PipeWireSnapshot,
        artifact_source: Arc<dyn ArtifactSource>,
    ) -> anyhow::Result<Self> {
        let schema_text = include_str!("schema.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);
        Ok(Self {
            db,
            schema,
            pipewire_snapshot: Arc::new(RwLock::new(pipewire_snapshot)),
            artifact_source: Some(artifact_source),
        })
    }

    /// Create an adapter with a live, shared PipeWire snapshot.
    ///
    /// The snapshot is shared with PipeWireListener which updates it
    /// as devices are added/removed. Trustfall queries always see
    /// the current device state.
    pub fn new_with_live_snapshot(
        db: Arc<Database>,
        pipewire_snapshot: Arc<RwLock<PipeWireSnapshot>>,
        artifact_source: Arc<dyn ArtifactSource>,
    ) -> anyhow::Result<Self> {
        let schema_text = include_str!("schema.graphql");
        let schema = Arc::new(Schema::parse(schema_text)?);
        Ok(Self {
            db,
            schema,
            pipewire_snapshot,
            artifact_source: Some(artifact_source),
        })
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

    /// Get a reference to the shared PipeWire snapshot.
    pub fn pipewire_snapshot(&self) -> &Arc<RwLock<PipeWireSnapshot>> {
        &self.pipewire_snapshot
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

                let snap = self.pipewire_snapshot.blocking_read();
                let nodes = snap.nodes.clone();
                drop(snap);

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
            "Artifact" => {
                let source = match &self.artifact_source {
                    Some(s) => s.clone(),
                    None => return Box::new(std::iter::empty()),
                };

                let id_filter = parameters.get("id").and_then(|v| v.as_str());
                let tag_filter = parameters.get("tag").and_then(|v| v.as_str());
                let creator_filter = parameters.get("creator").and_then(|v| v.as_str());
                let within_minutes = parameters.get("within_minutes").and_then(|v| v.as_i64());

                // ID lookup is always exclusive - return just that artifact
                if let Some(id) = id_filter {
                    let artifacts: Vec<_> = source.get(id).unwrap_or(None).into_iter().collect();
                    return Box::new(artifacts.into_iter().map(|a| Vertex::Artifact(Arc::new(a))));
                }

                // Start with time-filtered or all artifacts
                let window = within_minutes
                    .map(|m| std::time::Duration::from_secs(m.max(1) as u64 * 60));

                let base_artifacts = if window.is_some() || (tag_filter.is_none() && creator_filter.is_none()) {
                    // Use recent() if within_minutes specified OR no other filters
                    source.recent(window.unwrap_or(DEFAULT_RECENT_WINDOW)).unwrap_or_default()
                } else {
                    // Tag or creator specified without within_minutes - get all
                    source.all().unwrap_or_default()
                };

                // Apply tag and creator filters
                let artifacts: Vec<_> = base_artifacts
                    .into_iter()
                    .filter(|a| tag_filter.is_none_or(|t| a.tags.iter().any(|at| at == t)))
                    .filter(|a| creator_filter.is_none_or(|c| a.creator == c))
                    .collect();

                Box::new(artifacts.into_iter().map(|a| Vertex::Artifact(Arc::new(a))))
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
            // Artifact properties
            ("Artifact", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    FieldValue::String(a.id.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "content_hash") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    FieldValue::String(a.content_hash.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "created_at") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    FieldValue::String(a.created_at.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "creator") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    FieldValue::String(a.creator.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "tags") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    let tags: Vec<FieldValue> = a.tags.iter()
                        .map(|t| FieldValue::String(t.clone().into()))
                        .collect();
                    FieldValue::List(tags.into())
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "variation_set_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    a.variation_set_id.as_ref()
                        .map(|s| FieldValue::String(s.clone().into()))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "variation_index") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    a.variation_index
                        .map(|i| FieldValue::Int64(i as i64))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("Artifact", "metadata") => resolve_property_with(contexts, |v| {
                if let Vertex::Artifact(a) = v {
                    FieldValue::String(a.metadata.to_string().into())
                } else {
                    unreachable!()
                }
            }),
            // Annotation properties
            ("Annotation", "id") => resolve_property_with(contexts, |v| {
                if let Vertex::Annotation(a) = v {
                    FieldValue::String(a.id.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Annotation", "artifact_id") => resolve_property_with(contexts, |v| {
                if let Vertex::Annotation(a) = v {
                    FieldValue::String(a.artifact_id.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Annotation", "message") => resolve_property_with(contexts, |v| {
                if let Vertex::Annotation(a) = v {
                    FieldValue::String(a.message.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Annotation", "vibe") => resolve_property_with(contexts, |v| {
                if let Vertex::Annotation(a) = v {
                    a.vibe.as_ref()
                        .map(|s| FieldValue::String(s.clone().into()))
                        .unwrap_or(FieldValue::Null)
                } else {
                    unreachable!()
                }
            }),
            ("Annotation", "source") => resolve_property_with(contexts, |v| {
                if let Vertex::Annotation(a) = v {
                    FieldValue::String(a.source.clone().into())
                } else {
                    unreachable!()
                }
            }),
            ("Annotation", "created_at") => resolve_property_with(contexts, |v| {
                if let Vertex::Annotation(a) = v {
                    FieldValue::String(a.created_at.clone().into())
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
                        let snap = snapshot.blocking_read();
                        let ports: Vec<_> = snap.ports.iter()
                            .filter(|p| p.node_id == node_id)
                            .cloned()
                            .collect();
                        drop(snap);
                        Box::new(ports.into_iter().map(|p| Vertex::PipeWirePort(Arc::new(p))))
                            as VertexIterator<'a, Self::Vertex>
                    } else {
                        unreachable!()
                    }
                })
            }
            ("PipeWireNode", "identity") => {
                let db = self.db.clone();
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::PipeWireNode(n) = v {
                        // Extract fingerprints from the PipeWire node
                        let pw_source = crate::sources::pipewire::PipeWireSource::new();
                        let fingerprints = pw_source.extract_fingerprints(&n);

                        // Try to match against known identities
                        let matcher = crate::matcher::IdentityMatcher::new(&db);
                        match matcher.best_match(&fingerprints) {
                            Ok(Some(result)) => {
                                Box::new(std::iter::once(Vertex::Identity(Arc::new(result.identity))))
                                    as VertexIterator<'a, Self::Vertex>
                            }
                            Ok(None) | Err(_) => {
                                Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>
                            }
                        }
                    } else {
                        unreachable!()
                    }
                })
            }
            // Artifact relationships
            ("Artifact", "parent") => {
                let source = self.artifact_source.clone();
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::Artifact(a) = v {
                        if let (Some(source), Some(parent_id)) = (&source, &a.parent_id) {
                            let parent = source.get(parent_id).ok().flatten();
                            Box::new(parent.into_iter().map(|p| Vertex::Artifact(Arc::new(p))))
                                as VertexIterator<'a, Self::Vertex>
                        } else {
                            Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>
                        }
                    } else {
                        unreachable!()
                    }
                })
            }
            ("Artifact", "children") => {
                let source = self.artifact_source.clone();
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::Artifact(a) = v {
                        if let Some(source) = &source {
                            let children = source.by_parent(&a.id).unwrap_or_default();
                            Box::new(children.into_iter().map(|c| Vertex::Artifact(Arc::new(c))))
                                as VertexIterator<'a, Self::Vertex>
                        } else {
                            Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>
                        }
                    } else {
                        unreachable!()
                    }
                })
            }
            ("Artifact", "variations") => {
                let source = self.artifact_source.clone();
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::Artifact(a) = v {
                        if let (Some(source), Some(set_id)) = (&source, &a.variation_set_id) {
                            let variations = source.by_variation_set(set_id).unwrap_or_default();
                            // Exclude self from variations
                            let filtered: Vec<_> = variations.into_iter()
                                .filter(|sib| sib.id != a.id)
                                .collect();
                            Box::new(filtered.into_iter().map(|s| Vertex::Artifact(Arc::new(s))))
                                as VertexIterator<'a, Self::Vertex>
                        } else {
                            Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>
                        }
                    } else {
                        unreachable!()
                    }
                })
            }
            ("Artifact", "annotations") => {
                let source = self.artifact_source.clone();
                resolve_neighbors_with(contexts, move |v| {
                    if let Vertex::Artifact(a) = v {
                        if let Some(source) = &source {
                            let annotations = source.annotations_for(&a.id).unwrap_or_default();
                            Box::new(annotations.into_iter().map(|ann| Vertex::Annotation(Arc::new(ann))))
                                as VertexIterator<'a, Self::Vertex>
                        } else {
                            Box::new(std::iter::empty()) as VertexIterator<'a, Self::Vertex>
                        }
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
