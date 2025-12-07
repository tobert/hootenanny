-- otel_example.lua - OpenTelemetry observability example
--
-- This script demonstrates the otel.* namespace for distributed
-- tracing and observability. All operations are automatically
-- traced, and scripts can add custom events, attributes, and metrics.

function describe()
    return {
        name = "otel_demo",
        description = "Demonstrates OpenTelemetry features in Lua scripts",
        params = {
            items = { type = "array", required = true, description = "Items to process" },
            style = { type = "string", required = false, default = "default" }
        },
        returns = "Processing result with trace context"
    }
end

function main(params)
    -- Get trace context for correlation
    local trace_id = otel.trace_id()
    local span_id = otel.span_id()

    log.info("Processing with trace_id: " .. (trace_id or "none"))

    -- Set custom span attributes for filtering in trace UI
    otel.set_attribute("processing.style", params.style or "default")
    otel.set_attribute("processing.item_count", #params.items)

    -- Record custom event at start
    otel.event("processing_started", {
        item_count = #params.items,
        style = params.style
    })

    -- Process items with instrumentation
    local results = {}
    local total = 0

    for i, item in ipairs(params.items) do
        -- Record progress event
        if i % 10 == 0 then
            otel.event("processing_progress", {
                processed = i,
                remaining = #params.items - i
            })
        end

        -- Simulate processing
        local value = tonumber(item) or 0
        total = total + value
        table.insert(results, value * 2)
    end

    -- Record custom metrics
    otel.record_metric("items.processed", #params.items, {
        style = params.style
    })
    otel.record_metric("items.total_value", total)

    -- Final event
    otel.event("processing_complete", {
        result_count = #results,
        total_value = total
    })

    -- Include trace context in response for debugging
    return {
        results = results,
        total = total,
        trace_id = trace_id,
        span_id = span_id,
        traceparent = otel.traceparent()
    }
end
