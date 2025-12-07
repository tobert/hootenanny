-- tool_call.lua - Example of calling upstream MCP tools
--
-- This script demonstrates how to call Hootenanny's MCP tools
-- from within a Lua script. The mcp.* namespace is automatically
-- populated with tools discovered from connected upstream servers.

function describe()
    return {
        name = "tool_call_demo",
        description = "Demonstrates calling upstream MCP tools from Lua",
        params = {
            content = { type = "string", required = true, description = "Content to store in CAS" }
        },
        returns = "Hash of stored content and job list from hootenanny"
    }
end

function main(params)
    -- Log the operation
    log.info("Starting tool call demo")

    -- Store content in Hootenanny's CAS (Content Addressable Storage)
    -- The mcp.hootenanny namespace contains all tools from the hootenanny server
    local store_result = mcp.hootenanny.cas_store {
        content_base64 = require("string").gsub(
            params.content,
            ".",
            function(c) return string.format("\\%02X", string.byte(c)) end
        ),  -- Simple base64-like encoding
        mime_type = "text/plain"
    }

    log.info("Stored content with hash: " .. (store_result.hash or "unknown"))

    -- List current jobs to show we can call multiple tools
    local jobs = mcp.hootenanny.job_list {}

    -- Return a summary
    return {
        stored_hash = store_result.hash,
        job_count = jobs.count or 0,
        success = true
    }
end
