-- midi_process.lua - MIDI processing example
--
-- This script demonstrates the midi.* standard library for
-- reading, manipulating, and writing MIDI files. Works with
-- the temp.* module for temporary file handling.

function describe()
    return {
        name = "midi_transpose",
        description = "Transpose MIDI notes by a given number of semitones",
        params = {
            input_hash = { type = "string", required = true, description = "CAS hash of input MIDI" },
            semitones = { type = "number", required = false, default = 0 },
            quantize_grid = { type = "number", required = false, description = "Quantize to grid (ticks)" }
        },
        returns = "Hash of processed MIDI and note count"
    }
end

function main(params)
    -- Add observability
    otel.set_attribute("input.hash", params.input_hash)
    otel.set_attribute("processing.semitones", params.semitones or 0)

    -- Fetch MIDI from CAS
    otel.event("fetching_midi", { hash = params.input_hash })
    local info = hootenanny.cas_inspect { hash = params.input_hash }

    if not info or not info.local_path then
        log.error("Failed to fetch MIDI from CAS")
        return { error = "Failed to fetch MIDI", hash = params.input_hash }
    end

    -- Read MIDI file
    local track = midi.read(info.local_path)
    local original_count = #track.events

    log.info("Loaded MIDI with " .. original_count .. " events")
    otel.record_metric("midi.input_events", original_count)

    -- Transpose if requested
    if params.semitones and params.semitones ~= 0 then
        otel.event("transposing", { semitones = params.semitones })
        midi.transpose(track.events, params.semitones)
        log.info("Transposed by " .. params.semitones .. " semitones")
    end

    -- Quantize if requested
    if params.quantize_grid and params.quantize_grid > 0 then
        otel.event("quantizing", { grid = params.quantize_grid })
        midi.quantize(track.events, params.quantize_grid)
        log.info("Quantized to grid: " .. params.quantize_grid)
    end

    -- Write to temp file
    local output_path = temp.path("processed.mid")
    midi.write(output_path, track)

    -- Upload to CAS
    otel.event("uploading_result")
    local result = hootenanny.cas_upload_file {
        file_path = output_path,
        mime_type = "audio/midi"
    }

    otel.set_attribute("output.hash", result.hash)
    otel.record_metric("midi.output_events", #track.events)

    return {
        hash = result.hash,
        input_events = original_count,
        output_events = #track.events,
        semitones = params.semitones or 0,
        trace_id = otel.trace_id()
    }
end
