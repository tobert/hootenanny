//! OpenTelemetry bridge for Lua scripts.
//!
//! Provides the `otel.*` Lua namespace for observability:
//! - `otel.trace_id()` - Current trace ID as hex string
//! - `otel.span_id()` - Current span ID as hex string
//! - `otel.traceparent()` - W3C traceparent header value
//! - `otel.event(name, attrs?)` - Add event to current span
//! - `otel.set_attribute(key, value)` - Set span attribute
//! - `otel.record_metric(name, value, attrs?)` - Record a metric

use anyhow::Result;
use mlua::{Lua, Table, Value as LuaValue};

/// Stored span context for use in blocking Lua execution.
///
/// Since Lua runs in spawn_blocking, we capture the span context
/// before entering the blocking context and store it here.
#[derive(Clone, Debug)]
pub struct StoredSpanContext {
    pub trace_id: String,
    pub span_id: String,
    pub sampled: bool,
}

impl StoredSpanContext {
    /// Capture the current span context from the tracing system.
    pub fn capture() -> Option<Self> {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let context = tracing::Span::current().context();
        let span = context.span();
        let span_context = span.span_context();

        if span_context.is_valid() {
            Some(Self {
                trace_id: span_context.trace_id().to_string(),
                span_id: span_context.span_id().to_string(),
                sampled: span_context.is_sampled(),
            })
        } else {
            None
        }
    }

}

/// Lua registry key for stored span context.
const SPAN_CONTEXT_KEY: &str = "otel_span_context";

/// Store span context in Lua registry for access by otel.* functions.
pub fn store_span_context(lua: &Lua, ctx: Option<StoredSpanContext>) -> Result<()> {
    if let Some(ctx) = ctx {
        let table = lua.create_table()?;
        table.set("trace_id", ctx.trace_id)?;
        table.set("span_id", ctx.span_id)?;
        table.set("sampled", ctx.sampled)?;
        lua.set_named_registry_value(SPAN_CONTEXT_KEY, table)?;
    }
    Ok(())
}

/// Get stored span context from Lua registry.
fn get_stored_context(lua: &Lua) -> Option<(String, String, bool)> {
    let table: Option<Table> = lua.named_registry_value(SPAN_CONTEXT_KEY).ok();
    table.and_then(|t| {
        let trace_id: String = t.get("trace_id").ok()?;
        let span_id: String = t.get("span_id").ok()?;
        let sampled: bool = t.get("sampled").ok()?;
        Some((trace_id, span_id, sampled))
    })
}

/// Register the `otel` global table with observability functions.
pub fn register_otel_globals(lua: &Lua) -> Result<()> {
    let globals = lua.globals();

    let otel_table = lua.create_table()?;

    // otel.trace_id() -> string or nil
    let trace_id_fn = lua.create_function(|lua, ()| {
        Ok(get_stored_context(lua).map(|(trace_id, _, _)| trace_id))
    })?;
    otel_table.set("trace_id", trace_id_fn)?;

    // otel.span_id() -> string or nil
    let span_id_fn = lua.create_function(|lua, ()| {
        Ok(get_stored_context(lua).map(|(_, span_id, _)| span_id))
    })?;
    otel_table.set("span_id", span_id_fn)?;

    // otel.traceparent() -> string or nil
    let traceparent_fn = lua.create_function(|lua, ()| {
        Ok(get_stored_context(lua).map(|(trace_id, span_id, sampled)| {
            let flags = if sampled { "01" } else { "00" };
            format!("00-{}-{}-{}", trace_id, span_id, flags)
        }))
    })?;
    otel_table.set("traceparent", traceparent_fn)?;

    // otel.event(name, attrs?) - Add event to current span
    let event_fn = lua.create_function(|_, (name, attrs): (String, Option<Table>)| {
        if let Some(attr_table) = attrs {
            // Build key-value pairs from Lua table
            let mut fields: Vec<(String, String)> = Vec::new();
            for (k, v) in attr_table.pairs::<String, LuaValue>().flatten() {
                let value_str = match v {
                    LuaValue::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                    LuaValue::Integer(i) => i.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    LuaValue::Boolean(b) => b.to_string(),
                    _ => continue,
                };
                fields.push((k, value_str));
            }

            // Use tracing event with dynamic fields
            // Note: tracing macro doesn't support dynamic fields easily,
            // so we serialize to a message
            let attrs_str: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            tracing::info!(target: "luanette.script.event", name = %name, attrs = %attrs_str.join(", "), "Script event");
        } else {
            tracing::info!(target: "luanette.script.event", name = %name, "Script event");
        }
        Ok(())
    })?;
    otel_table.set("event", event_fn)?;

    // otel.set_attribute(key, value) - Set span attribute
    // Note: This logs the attribute since we can't easily modify the current span
    let set_attr_fn = lua.create_function(|_, (key, value): (String, LuaValue)| {
        let value_str = match value {
            LuaValue::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            LuaValue::Integer(i) => i.to_string(),
            LuaValue::Number(n) => n.to_string(),
            LuaValue::Boolean(b) => b.to_string(),
            LuaValue::Nil => "nil".to_string(),
            _ => "[complex]".to_string(),
        };
        tracing::info!(target: "luanette.script.attribute", key = %key, value = %value_str, "Script attribute");
        Ok(())
    })?;
    otel_table.set("set_attribute", set_attr_fn)?;

    // otel.record_metric(name, value, attrs?) - Record a metric
    let metric_fn = lua.create_function(|_, (name, value, attrs): (String, f64, Option<Table>)| {
        if let Some(attr_table) = attrs {
            let mut fields: Vec<(String, String)> = Vec::new();
            for (k, v) in attr_table.pairs::<String, LuaValue>().flatten() {
                let value_str = match v {
                    LuaValue::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                    LuaValue::Integer(i) => i.to_string(),
                    LuaValue::Number(n) => n.to_string(),
                    LuaValue::Boolean(b) => b.to_string(),
                    _ => continue,
                };
                fields.push((k, value_str));
            }
            let attrs_str: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            tracing::info!(
                target: "luanette.script.metric",
                metric_name = %name,
                metric_value = %value,
                attrs = %attrs_str.join(", "),
                "Script metric"
            );
        } else {
            tracing::info!(
                target: "luanette.script.metric",
                metric_name = %name,
                metric_value = %value,
                "Script metric"
            );
        }
        Ok(())
    })?;
    otel_table.set("record_metric", metric_fn)?;

    globals.set("otel", otel_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_otel_globals() {
        let lua = Lua::new();
        register_otel_globals(&lua).unwrap();

        // Verify otel table exists
        let globals = lua.globals();
        let otel: Table = globals.get("otel").unwrap();

        // Verify functions exist
        assert!(otel.contains_key("trace_id").unwrap());
        assert!(otel.contains_key("span_id").unwrap());
        assert!(otel.contains_key("traceparent").unwrap());
        assert!(otel.contains_key("event").unwrap());
        assert!(otel.contains_key("set_attribute").unwrap());
        assert!(otel.contains_key("record_metric").unwrap());
    }

    #[test]
    fn test_trace_id_outside_span() {
        let lua = Lua::new();
        register_otel_globals(&lua).unwrap();

        // Outside a span, trace_id should return nil
        let result: Option<String> = lua.load("return otel.trace_id()").eval().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_event_with_attrs() {
        let lua = Lua::new();
        register_otel_globals(&lua).unwrap();

        // Should not panic
        lua.load(r#"otel.event("test_event", {foo = "bar", count = 42})"#)
            .exec()
            .unwrap();
    }

    #[test]
    fn test_set_attribute() {
        let lua = Lua::new();
        register_otel_globals(&lua).unwrap();

        // Should not panic
        lua.load(r#"otel.set_attribute("my_key", "my_value")"#)
            .exec()
            .unwrap();
    }

    #[test]
    fn test_record_metric() {
        let lua = Lua::new();
        register_otel_globals(&lua).unwrap();

        // Should not panic
        lua.load(r#"otel.record_metric("notes.count", 128, {instrument = "piano"})"#)
            .exec()
            .unwrap();
    }
}
