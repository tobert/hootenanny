"""
Known Hootenanny tools and their metadata.

This provides a static fallback when the server isn't available,
and serves as documentation of the tool interface.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum, auto


class ToolCategory(Enum):
    """Tool categories for organization."""
    CAS = auto()
    ARTIFACT = auto()
    ORPHEUS = auto()
    ABC = auto()
    AUDIO = auto()
    ANALYSIS = auto()
    GRAPH = auto()
    JOB = auto()
    GARDEN = auto()
    SCHEMA = auto()


@dataclass
class ToolDef:
    """Definition of a Hootenanny tool."""
    name: str
    category: ToolCategory
    description: str
    returns_job: bool = False
    required_params: list[str] = field(default_factory=list)
    optional_params: list[str] = field(default_factory=list)


# Known tools - update when adding new tools to hootenanny
TOOLS: dict[str, ToolDef] = {
    # CAS
    "cas_store": ToolDef(
        name="cas_store",
        category=ToolCategory.CAS,
        description="Store content in CAS",
        required_params=["data", "mime_type"],
    ),
    "cas_inspect": ToolDef(
        name="cas_inspect",
        category=ToolCategory.CAS,
        description="Inspect content in CAS",
        required_params=["hash"],
    ),
    "cas_get": ToolDef(
        name="cas_get",
        category=ToolCategory.CAS,
        description="Retrieve content from CAS",
        required_params=["hash"],
    ),
    "cas_upload_file": ToolDef(
        name="cas_upload_file",
        category=ToolCategory.CAS,
        description="Upload file to CAS",
        required_params=["file_path", "mime_type"],
    ),

    # Artifacts
    "artifact_upload": ToolDef(
        name="artifact_upload",
        category=ToolCategory.ARTIFACT,
        description="Upload file and create artifact",
        required_params=["file_path", "mime_type"],
        optional_params=["tags", "creator", "parent_id", "variation_set_id"],
    ),
    "artifact_get": ToolDef(
        name="artifact_get",
        category=ToolCategory.ARTIFACT,
        description="Get artifact by ID",
        required_params=["id"],
    ),
    "artifact_list": ToolDef(
        name="artifact_list",
        category=ToolCategory.ARTIFACT,
        description="List artifacts",
        optional_params=["tag", "creator"],
    ),

    # Orpheus
    "orpheus_generate": ToolDef(
        name="orpheus_generate",
        category=ToolCategory.ORPHEUS,
        description="Generate MIDI from scratch",
        returns_job=True,
        optional_params=["model", "temperature", "top_p", "max_tokens", "num_variations",
                        "tags", "creator", "parent_id", "variation_set_id"],
    ),
    "orpheus_continue": ToolDef(
        name="orpheus_continue",
        category=ToolCategory.ORPHEUS,
        description="Continue existing MIDI",
        returns_job=True,
        required_params=["input_hash"],
        optional_params=["model", "temperature", "top_p", "max_tokens", "num_variations",
                        "tags", "creator", "parent_id", "variation_set_id"],
    ),
    "orpheus_bridge": ToolDef(
        name="orpheus_bridge",
        category=ToolCategory.ORPHEUS,
        description="Create bridge between MIDI sections",
        returns_job=True,
        required_params=["section_a_hash"],
        optional_params=["section_b_hash", "model", "temperature", "top_p", "max_tokens",
                        "tags", "creator", "parent_id", "variation_set_id"],
    ),
    "orpheus_loops": ToolDef(
        name="orpheus_loops",
        category=ToolCategory.ORPHEUS,
        description="Generate loopable MIDI",
        returns_job=True,
        optional_params=["seed_hash", "temperature", "top_p", "max_tokens", "num_variations",
                        "tags", "creator", "parent_id", "variation_set_id"],
    ),
    "orpheus_classify": ToolDef(
        name="orpheus_classify",
        category=ToolCategory.ORPHEUS,
        description="Classify MIDI content",
        required_params=["midi_hash"],
    ),

    # ABC
    "abc_parse": ToolDef(
        name="abc_parse",
        category=ToolCategory.ABC,
        description="Parse ABC notation to AST",
        required_params=["abc"],
    ),
    "abc_to_midi": ToolDef(
        name="abc_to_midi",
        category=ToolCategory.ABC,
        description="Convert ABC notation to MIDI",
        required_params=["abc"],
        optional_params=["tempo_override", "transpose", "velocity", "channel",
                        "tags", "creator", "parent_id", "variation_set_id"],
    ),
    "abc_validate": ToolDef(
        name="abc_validate",
        category=ToolCategory.ABC,
        description="Validate ABC notation",
        required_params=["abc"],
    ),
    "abc_transpose": ToolDef(
        name="abc_transpose",
        category=ToolCategory.ABC,
        description="Transpose ABC notation",
        required_params=["abc"],
        optional_params=["semitones", "target_key"],
    ),

    # Audio
    "convert_midi_to_wav": ToolDef(
        name="convert_midi_to_wav",
        category=ToolCategory.AUDIO,
        description="Render MIDI to WAV using SoundFont",
        returns_job=True,
        required_params=["input_hash", "soundfont_hash"],
        optional_params=["sample_rate", "tags", "creator", "parent_id", "variation_set_id"],
    ),
    "soundfont_inspect": ToolDef(
        name="soundfont_inspect",
        category=ToolCategory.AUDIO,
        description="Inspect SoundFont presets",
        required_params=["soundfont_hash"],
        optional_params=["include_drum_map"],
    ),
    "soundfont_preset_inspect": ToolDef(
        name="soundfont_preset_inspect",
        category=ToolCategory.AUDIO,
        description="Inspect specific SoundFont preset",
        required_params=["soundfont_hash", "bank", "program"],
    ),
    "musicgen_generate": ToolDef(
        name="musicgen_generate",
        category=ToolCategory.AUDIO,
        description="Generate audio with MusicGen",
        returns_job=True,
        optional_params=["prompt", "duration", "temperature", "top_k", "top_p",
                        "guidance_scale", "do_sample", "tags", "creator"],
    ),
    "yue_generate": ToolDef(
        name="yue_generate",
        category=ToolCategory.AUDIO,
        description="Generate song with YuE",
        returns_job=True,
        required_params=["lyrics"],
        optional_params=["genre", "max_new_tokens", "run_n_segments", "seed",
                        "tags", "creator", "parent_id", "variation_set_id"],
    ),

    # Analysis
    "beatthis_analyze": ToolDef(
        name="beatthis_analyze",
        category=ToolCategory.ANALYSIS,
        description="Analyze beats in audio",
        optional_params=["audio_path", "audio_hash", "include_frames"],
    ),
    "clap_analyze": ToolDef(
        name="clap_analyze",
        category=ToolCategory.ANALYSIS,
        description="Analyze audio with CLAP",
        required_params=["audio_hash"],
        optional_params=["tasks", "audio_b_hash", "text_candidates", "creator"],
    ),

    # Graph
    "graph_query": ToolDef(
        name="graph_query",
        category=ToolCategory.GRAPH,
        description="Execute Trustfall query",
        required_params=["query"],
        optional_params=["variables", "limit"],
    ),
    "graph_bind": ToolDef(
        name="graph_bind",
        category=ToolCategory.GRAPH,
        description="Bind identity to device",
        required_params=["id", "name"],
        optional_params=["hints"],
    ),
    "graph_tag": ToolDef(
        name="graph_tag",
        category=ToolCategory.GRAPH,
        description="Tag an identity",
        required_params=["identity_id", "namespace", "value"],
    ),
    "graph_connect": ToolDef(
        name="graph_connect",
        category=ToolCategory.GRAPH,
        description="Connect two identities",
        required_params=["from_identity", "from_port", "to_identity", "to_port"],
        optional_params=["transport"],
    ),
    "graph_find": ToolDef(
        name="graph_find",
        category=ToolCategory.GRAPH,
        description="Find identities",
        optional_params=["name", "tag_namespace", "tag_value"],
    ),
    "graph_context": ToolDef(
        name="graph_context",
        category=ToolCategory.GRAPH,
        description="Get graph context for LLM",
        optional_params=["tag", "vibe_search", "creator", "limit",
                        "include_metadata", "include_annotations", "within_minutes"],
    ),
    "add_annotation": ToolDef(
        name="add_annotation",
        category=ToolCategory.GRAPH,
        description="Add annotation to artifact",
        required_params=["artifact_id", "message"],
        optional_params=["vibe", "source"],
    ),

    # Jobs
    "job_status": ToolDef(
        name="job_status",
        category=ToolCategory.JOB,
        description="Get job status",
        required_params=["job_id"],
    ),
    "job_poll": ToolDef(
        name="job_poll",
        category=ToolCategory.JOB,
        description="Poll for job completion",
        required_params=["job_ids", "timeout_ms"],
        optional_params=["mode"],
    ),
    "job_list": ToolDef(
        name="job_list",
        category=ToolCategory.JOB,
        description="List jobs",
        optional_params=["status"],
    ),
    "job_cancel": ToolDef(
        name="job_cancel",
        category=ToolCategory.JOB,
        description="Cancel a job",
        required_params=["job_id"],
    ),
    "job_sleep": ToolDef(
        name="job_sleep",
        category=ToolCategory.JOB,
        description="Sleep for a duration",
        required_params=["milliseconds"],
    ),

    # Garden
    "garden_status": ToolDef(
        name="garden_status",
        category=ToolCategory.GARDEN,
        description="Get chaosgarden status",
    ),
    "garden_play": ToolDef(
        name="garden_play",
        category=ToolCategory.GARDEN,
        description="Start playback",
    ),
    "garden_pause": ToolDef(
        name="garden_pause",
        category=ToolCategory.GARDEN,
        description="Pause playback",
    ),
    "garden_stop": ToolDef(
        name="garden_stop",
        category=ToolCategory.GARDEN,
        description="Stop playback",
    ),
    "garden_seek": ToolDef(
        name="garden_seek",
        category=ToolCategory.GARDEN,
        description="Seek to position",
        required_params=["beat"],
    ),
    "garden_set_tempo": ToolDef(
        name="garden_set_tempo",
        category=ToolCategory.GARDEN,
        description="Set tempo",
        required_params=["bpm"],
    ),
    "garden_query": ToolDef(
        name="garden_query",
        category=ToolCategory.GARDEN,
        description="Query garden state",
        required_params=["query"],
        optional_params=["variables"],
    ),
    "garden_emergency_pause": ToolDef(
        name="garden_emergency_pause",
        category=ToolCategory.GARDEN,
        description="Emergency pause",
    ),
    "garden_create_region": ToolDef(
        name="garden_create_region",
        category=ToolCategory.GARDEN,
        description="Create timeline region",
        required_params=["position", "duration", "behavior_type", "content_id"],
    ),
    "garden_delete_region": ToolDef(
        name="garden_delete_region",
        category=ToolCategory.GARDEN,
        description="Delete timeline region",
        required_params=["region_id"],
    ),
    "garden_move_region": ToolDef(
        name="garden_move_region",
        category=ToolCategory.GARDEN,
        description="Move timeline region",
        required_params=["region_id", "new_position"],
    ),
    "garden_get_regions": ToolDef(
        name="garden_get_regions",
        category=ToolCategory.GARDEN,
        description="Get timeline regions",
        optional_params=["start", "end"],
    ),
}


def get_tool(name: str) -> ToolDef | None:
    """Get tool definition by name."""
    return TOOLS.get(name)


def list_tools(category: ToolCategory | None = None) -> list[ToolDef]:
    """List tools, optionally filtered by category."""
    if category is None:
        return list(TOOLS.values())
    return [t for t in TOOLS.values() if t.category == category]


def tools_by_category() -> dict[ToolCategory, list[ToolDef]]:
    """Group tools by category."""
    result: dict[ToolCategory, list[ToolDef]] = {}
    for tool in TOOLS.values():
        result.setdefault(tool.category, []).append(tool)
    return result
