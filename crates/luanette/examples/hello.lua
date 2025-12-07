-- hello.lua - Basic example script for Luanette
--
-- This script demonstrates the basic structure of a Luanette script
-- with describe() for introspection and main() for execution.

function describe()
    return {
        name = "hello",
        description = "A simple greeting script that says hello",
        params = {
            name = { type = "string", required = false, default = "World" }
        },
        returns = "A greeting message"
    }
end

function main(params)
    local name = params.name or "World"

    -- Use the log module for observability
    log.info("Greeting " .. name)

    return {
        message = "Hello, " .. name .. "!",
        timestamp = os.time()
    }
end
