-- multi_variation.lua - Generate multiple MIDI variations
--
-- This script demonstrates a more complex workflow that generates
-- multiple variations of MIDI using Orpheus with different parameters.

function describe()
    return {
        name = "multi_variation_generator",
        description = "Generate multiple MIDI variations with different temperatures",
        params = {
            count = { type = "number", required = false, default = 3 },
            base_temperature = { type = "number", required = false, default = 0.9 },
            temperature_step = { type = "number", required = false, default = 0.2 },
            model = { type = "string", required = false, default = "base" },
            max_tokens = { type = "number", required = false, default = 512 },
            tags = { type = "array", required = false }
        },
        returns = "List of job IDs for the generated variations"
    }
end

function main(params)
    local count = params.count or 3
    local base_temp = params.base_temperature or 0.9
    local temp_step = params.temperature_step or 0.2

    log.info("Generating " .. count .. " variations")
    otel.set_attribute("generation.count", count)
    otel.set_attribute("generation.base_temperature", base_temp)
    otel.set_attribute("generation.model", params.model or "base")

    -- Create a variation set ID to group these generations
    local variation_set_id = "var_" .. os.time()

    local jobs = {}
    local job_ids = {}

    for i = 1, count do
        local temperature = base_temp + ((i - 1) * temp_step)

        otel.event("spawning_variation", {
            index = i,
            temperature = temperature
        })

        -- Build tags for this variation
        local variation_tags = params.tags or {}
        table.insert(variation_tags, "temp:" .. string.format("%.2f", temperature))
        table.insert(variation_tags, "variation:" .. i)

        -- Generate MIDI with Orpheus
        local job = mcp.hootenanny.orpheus_generate {
            model = params.model or "base",
            temperature = temperature,
            max_tokens = params.max_tokens or 512,
            num_variations = 1,
            variation_set_id = variation_set_id,
            tags = variation_tags
        }

        table.insert(jobs, {
            index = i,
            temperature = temperature,
            job_id = job.job_id
        })
        table.insert(job_ids, job.job_id)

        log.info("Started variation " .. i .. " with temp " .. temperature .. " -> job " .. job.job_id)
    end

    -- Optionally poll for completion (commented out for async pattern)
    -- local results = mcp.hootenanny.job_poll {
    --     job_ids = job_ids,
    --     timeout_ms = 120000,
    --     mode = "all"
    -- }

    otel.record_metric("variations.spawned", count)

    return {
        variation_set_id = variation_set_id,
        count = count,
        jobs = jobs,
        job_ids = job_ids,
        trace_id = otel.trace_id()
    }
end
