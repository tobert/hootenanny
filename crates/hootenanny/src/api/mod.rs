pub mod garden_adapter; // Trustfall adapter for cached garden state
pub mod manual_schemas; // Hand-written schemas for llama.cpp compatibility
pub mod native;
pub mod schema;
pub mod service;
mod service_typed; // Typed method implementations for service
pub mod tools;
pub mod tools_registry; // Tool metadata for discovery
pub mod typed_dispatcher; // Typed dispatcher (Protocol v2)
