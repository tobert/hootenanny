"""
Minimal TMIDIX extraction for Orpheus tokenization.

This is a curated subset of the TMIDIX library (https://github.com/Tegridy-Code/Project-Los-Angeles)
containing only the functions required for Orpheus MIDI ↔ token conversion.

Original TMIDIX Copyright 2025 Project Los Angeles / Tegridy Code
Licensed under the Apache License, Version 2.0.

Original MIDI.py portions Copyright 2020 Peter Billam (pjb.com.au)
"""

from __future__ import annotations

import copy
import math
import struct
import sys
from collections import Counter, OrderedDict
from itertools import groupby

__version__ = "0.1.0"  # hootpy extraction


# =============================================================================
# MIDI Encoding/Decoding (from MIDI.py v6.7 by Peter Billam)
# =============================================================================

_previous_warning = ""
_previous_times = 0
_no_warning = True  # Suppress warnings by default


def _warn(s: str = "") -> None:
    """Print warning (suppressed by default)."""
    if _no_warning:
        return
    global _previous_times, _previous_warning
    if s == _previous_warning:
        _previous_times += 1
    else:
        _clean_up_warnings()
        sys.stderr.write(str(s) + "\n")
        _previous_warning = s


def _clean_up_warnings() -> None:
    """Clean up repeated warnings."""
    if _no_warning:
        return
    global _previous_times, _previous_warning
    if _previous_times > 1:
        sys.stderr.write(f"  previous message repeated {_previous_times} times\n")
    elif _previous_times > 0:
        sys.stderr.write("  previous message repeated\n")
    _previous_times = 0
    _previous_warning = ""


def _ber_compressed_int(integer: int) -> bytearray:
    """BER compressed integer encoding."""
    ber = bytearray(b"")
    seven_bits = 0x7F & integer
    ber.insert(0, seven_bits)
    integer >>= 7
    while integer > 0:
        seven_bits = 0x7F & integer
        ber.insert(0, 0x80 | seven_bits)
        integer >>= 7
    return ber


def _unshift_ber_int(ba: bytearray) -> tuple[int, bytearray]:
    """Extract BER-compressed integer from start of bytearray."""
    if not len(ba):
        _warn("_unshift_ber_int: no integer found")
        return (0, b"")
    byte = ba[0]
    ba = ba[1:]
    integer = 0
    while True:
        integer += byte & 0x7F
        if not (byte & 0x80):
            return (integer, ba)
        if not len(ba):
            _warn("_unshift_ber_int: no end-of-integer found")
            return (0, ba)
        byte = ba[0]
        ba = ba[1:]
        integer <<= 7


def _twobytes2int(byte_a: bytes) -> int:
    """Decode 16-bit quantity from two bytes."""
    return byte_a[1] | (byte_a[0] << 8)


def _int2twobytes(int_16bit: int) -> bytes:
    """Encode 16-bit quantity into two bytes."""
    return bytes([(int_16bit >> 8) & 0xFF, int_16bit & 0xFF])


def _read_14_bit(byte_a: bytes) -> int:
    """Decode 14-bit quantity from two bytes."""
    return byte_a[0] | (byte_a[1] << 7)


def _write_14_bit(int_14bit: int) -> bytes:
    """Encode 14-bit quantity into two bytes."""
    return bytes([int_14bit & 0x7F, (int_14bit >> 7) & 0x7F])


def _some_text_event(
    which_kind: int = 0x01, text: bytes = b"some_text", text_encoding: str = "ISO-8859-1"
) -> bytes:
    """Create a text meta-event."""
    if isinstance(text, str):
        data = bytes(text, encoding=text_encoding)
    else:
        data = bytes(text)
    return b"\xFF" + bytes((which_kind,)) + _ber_compressed_int(len(data)) + data


# MIDI event types
MIDI_events = (
    "note_off",
    "note_on",
    "key_after_touch",
    "control_change",
    "patch_change",
    "channel_after_touch",
    "pitch_wheel_change",
)

Text_events = (
    "text_event",
    "copyright_text_event",
    "track_name",
    "instrument_name",
    "lyric",
    "marker",
    "cue_point",
    "text_event_08",
    "text_event_09",
    "text_event_0a",
    "text_event_0b",
    "text_event_0c",
    "text_event_0d",
    "text_event_0e",
    "text_event_0f",
)

Nontext_meta_events = (
    "end_track",
    "set_tempo",
    "smpte_offset",
    "time_signature",
    "key_signature",
    "sequencer_specific",
    "raw_meta_event",
    "sysex_f0",
    "sysex_f7",
    "song_position",
    "song_select",
    "tune_request",
)

Meta_events = Text_events + Nontext_meta_events
All_events = MIDI_events + Meta_events

Event2channelindex = {
    "note": 3,
    "note_off": 2,
    "note_on": 2,
    "key_after_touch": 2,
    "control_change": 2,
    "patch_change": 2,
    "channel_after_touch": 2,
    "pitch_wheel_change": 2,
}


def _decode(
    trackdata: bytes = b"",
    exclude: list | None = None,
    include: list | None = None,
    no_eot_magic: bool = False,
) -> list:
    """Decode MIDI track data into opus-style events."""
    trackdata = bytearray(trackdata)
    if exclude is None:
        exclude = []
    if include is None:
        include = []
    if include and not exclude:
        exclude = list(All_events)
    include = set(include)
    exclude = set(exclude)

    event_code = -1
    events = []

    while len(trackdata):
        eot = False
        E = []

        time, trackdata = _unshift_ber_int(trackdata)
        first_byte = trackdata[0] & 0xFF
        trackdata = trackdata[1:]

        if first_byte < 0xF0:  # MIDI event
            if first_byte & 0x80:
                event_code = first_byte
            else:
                trackdata.insert(0, first_byte)
                if event_code == -1:
                    _warn("Running status not set; Aborting track.")
                    return []

            command = event_code & 0xF0
            channel = event_code & 0x0F

            if command == 0xF6:
                pass
            elif command in (0xC0, 0xD0):
                parameter = trackdata[0]
                trackdata = trackdata[1:]
            else:
                parameter = (trackdata[0], trackdata[1])
                trackdata = trackdata[2:]

            if command == 0x80:
                if "note_off" not in exclude:
                    E = ["note_off", time, channel, parameter[0], parameter[1]]
            elif command == 0x90:
                if "note_on" not in exclude:
                    E = ["note_on", time, channel, parameter[0], parameter[1]]
            elif command == 0xA0:
                if "key_after_touch" not in exclude:
                    E = ["key_after_touch", time, channel, parameter[0], parameter[1]]
            elif command == 0xB0:
                if "control_change" not in exclude:
                    E = ["control_change", time, channel, parameter[0], parameter[1]]
            elif command == 0xC0:
                if "patch_change" not in exclude:
                    E = ["patch_change", time, channel, parameter]
            elif command == 0xD0:
                if "channel_after_touch" not in exclude:
                    E = ["channel_after_touch", time, channel, parameter]
            elif command == 0xE0:
                if "pitch_wheel_change" not in exclude:
                    E = ["pitch_wheel_change", time, channel, _read_14_bit(parameter) - 0x2000]

        elif first_byte == 0xFF:  # Meta-Event
            command = trackdata[0] & 0xFF
            trackdata = trackdata[1:]
            length, trackdata = _unshift_ber_int(trackdata)

            if command == 0x00:
                if length == 2:
                    E = ["set_sequence_number", time, _twobytes2int(trackdata)]
                else:
                    E = ["set_sequence_number", time, 0]
            elif 0x01 <= command <= 0x0F:
                text_data = bytes(trackdata[0:length])
                event_names = [
                    "text_event",
                    "copyright_text_event",
                    "track_name",
                    "instrument_name",
                    "lyric",
                    "marker",
                    "cue_point",
                    "text_event_08",
                    "text_event_09",
                    "text_event_0a",
                    "text_event_0b",
                    "text_event_0c",
                    "text_event_0d",
                    "text_event_0e",
                    "text_event_0f",
                ]
                E = [event_names[command - 1], time, text_data]
            elif command == 0x2F:
                E = ["end_track", time]
            elif command == 0x51:
                E = ["set_tempo", time, struct.unpack(">I", b"\x00" + trackdata[0:3])[0]]
            elif command == 0x54:
                E = ["smpte_offset", time] + list(struct.unpack(">BBBBB", trackdata[0:5]))
            elif command == 0x58:
                E = ["time_signature", time] + list(trackdata[0:4])
            elif command == 0x59:
                E = ["key_signature", time] + list(struct.unpack(">bB", trackdata[0:2]))
            elif command == 0x7F:
                E = ["sequencer_specific", time, bytes(trackdata[0:length])]
            else:
                E = ["raw_meta_event", time, command, bytes(trackdata[0:length])]

            trackdata = trackdata[length:]

        elif first_byte in (0xF0, 0xF7):
            length, trackdata = _unshift_ber_int(trackdata)
            if first_byte == 0xF0:
                E = ["sysex_f0", time, bytes(trackdata[0:length])]
            else:
                E = ["sysex_f7", time, bytes(trackdata[0:length])]
            trackdata = trackdata[length:]

        elif first_byte == 0xF2:
            E = ["song_position", time, _read_14_bit(trackdata[:2])]
            trackdata = trackdata[2:]
        elif first_byte == 0xF3:
            E = ["song_select", time, trackdata[0]]
            trackdata = trackdata[1:]
        elif first_byte == 0xF6:
            E = ["tune_request", time]
        elif first_byte > 0xF0:
            E = ["raw_data", time, trackdata[0]]
            trackdata = trackdata[1:]
        else:
            _warn("Aborting track. Command-byte first_byte=" + hex(first_byte))
            break

        if E and E[0] == "end_track":
            eot = True
            if not no_eot_magic:
                if E[1] > 0:
                    E = ["text_event", E[1], ""]
                else:
                    E = []

        if E and E[0] not in exclude:
            events.append(E)
        if eot:
            break

    return events


def _encode(
    events_lol: list,
    never_add_eot: bool = False,
    no_eot_magic: bool = False,
    no_running_status: bool = False,
    text_encoding: str = "ISO-8859-1",
) -> bytes:
    """Encode opus-style events into MIDI track data."""
    data = []
    events = copy.deepcopy(events_lol)

    if not never_add_eot:
        if events:
            last = events[-1]
            if last[0] != "end_track":
                if last[0] == "text_event" and len(last[2]) == 0:
                    if no_eot_magic:
                        events.append(["end_track", 0])
                    else:
                        last[0] = "end_track"
                else:
                    events.append(["end_track", 0])
        else:
            events = [["end_track", 0]]

    last_status = -1

    for event_r in events:
        E = copy.deepcopy(event_r)
        if not E:
            continue

        event = E.pop(0)
        if not len(event):
            continue

        dtime = int(E.pop(0))

        if event in (
            "note_on",
            "note_off",
            "control_change",
            "key_after_touch",
            "patch_change",
            "channel_after_touch",
            "pitch_wheel_change",
        ):
            if event == "note_off":
                status = 0x80 | (int(E[0]) & 0x0F)
                parameters = struct.pack(">BB", int(E[1]) & 0x7F, int(E[2]) & 0x7F)
            elif event == "note_on":
                status = 0x90 | (int(E[0]) & 0x0F)
                parameters = struct.pack(">BB", int(E[1]) & 0x7F, int(E[2]) & 0x7F)
            elif event == "key_after_touch":
                status = 0xA0 | (int(E[0]) & 0x0F)
                parameters = struct.pack(">BB", int(E[1]) & 0x7F, int(E[2]) & 0x7F)
            elif event == "control_change":
                status = 0xB0 | (int(E[0]) & 0x0F)
                parameters = struct.pack(">BB", int(E[1]) & 0xFF, int(E[2]) & 0xFF)
            elif event == "patch_change":
                status = 0xC0 | (int(E[0]) & 0x0F)
                parameters = struct.pack(">B", int(E[1]) & 0xFF)
            elif event == "channel_after_touch":
                status = 0xD0 | (int(E[0]) & 0x0F)
                parameters = struct.pack(">B", int(E[1]) & 0xFF)
            elif event == "pitch_wheel_change":
                status = 0xE0 | (int(E[0]) & 0x0F)
                parameters = _write_14_bit(int(E[1]) + 0x2000)

            data.append(_ber_compressed_int(dtime))
            if status != last_status or no_running_status:
                data.append(struct.pack(">B", status))
            data.append(parameters)
            last_status = status

        else:
            last_status = -1
            event_data = b""

            if event == "raw_meta_event":
                event_data = _some_text_event(int(E[0]), E[1], text_encoding)
            elif event == "set_sequence_number":
                event_data = b"\xFF\x00\x02" + _int2twobytes(E[0])
            elif event == "text_event":
                event_data = _some_text_event(0x01, E[0], text_encoding)
            elif event == "copyright_text_event":
                event_data = _some_text_event(0x02, E[0], text_encoding)
            elif event == "track_name":
                event_data = _some_text_event(0x03, E[0], text_encoding)
            elif event == "instrument_name":
                event_data = _some_text_event(0x04, E[0], text_encoding)
            elif event == "lyric":
                event_data = _some_text_event(0x05, E[0], text_encoding)
            elif event == "marker":
                event_data = _some_text_event(0x06, E[0], text_encoding)
            elif event == "cue_point":
                event_data = _some_text_event(0x07, E[0], text_encoding)
            elif event == "end_track":
                event_data = b"\xFF\x2F\x00"
            elif event == "set_tempo":
                event_data = b"\xFF\x51\x03" + struct.pack(">I", E[0])[1:]
            elif event == "smpte_offset":
                event_data = struct.pack(">BBBbBBBB", 0xFF, 0x54, 0x05, E[0], E[1], E[2], E[3], E[4])
            elif event == "time_signature":
                event_data = struct.pack(">BBBbBBB", 0xFF, 0x58, 0x04, E[0], E[1], E[2], E[3])
            elif event == "key_signature":
                event_data = struct.pack(">BBBbB", 0xFF, 0x59, 0x02, E[0], E[1])
            elif event == "sequencer_specific":
                event_data = _some_text_event(0x7F, E[0], text_encoding)
            elif event == "sysex_f0":
                event_data = bytearray(b"\xF0") + _ber_compressed_int(len(E[0])) + bytearray(E[0])
            elif event == "sysex_f7":
                event_data = bytearray(b"\xF7") + _ber_compressed_int(len(E[0])) + bytearray(E[0])
            elif event == "song_position":
                event_data = b"\xF2" + _write_14_bit(E[0])
            elif event == "song_select":
                event_data = struct.pack(">BB", 0xF3, E[0])
            elif event == "tune_request":
                event_data = b"\xF6"
            else:
                _warn("Unknown event: " + str(event))
                continue

            if isinstance(event_data, str):
                event_data = bytearray(event_data.encode("Latin1", "ignore"))
            if len(event_data):
                data.append(_ber_compressed_int(dtime) + event_data)

    return b"".join(data)


# =============================================================================
# Opus/Score Conversion
# =============================================================================


def opus2midi(opus: list | None = None, text_encoding: str = "ISO-8859-1") -> bytes:
    """Convert opus to MIDI bytes."""
    if opus is None or len(opus) < 2:
        opus = [1000, []]
    tracks = copy.deepcopy(opus)
    ticks = int(tracks.pop(0))
    ntracks = len(tracks)
    format = 0 if ntracks == 1 else 1

    my_midi = b"MThd\x00\x00\x00\x06" + struct.pack(">HHH", format, ntracks, ticks)
    for track in tracks:
        events = _encode(track, text_encoding=text_encoding)
        my_midi += b"MTrk" + struct.pack(">I", len(events)) + events
    _clean_up_warnings()
    return my_midi


def score2opus(score: list | None = None, text_encoding: str = "ISO-8859-1") -> list:
    """Convert score to opus."""
    if score is None or len(score) < 2:
        score = [1000, []]
    tracks = copy.deepcopy(score)
    ticks = int(tracks.pop(0))
    opus_tracks = []

    for scoretrack in tracks:
        time2events = {}
        for scoreevent in scoretrack:
            if scoreevent[0] == "note":
                note_on_event = ["note_on", scoreevent[1], scoreevent[3], scoreevent[4], scoreevent[5]]
                note_off_event = [
                    "note_off",
                    scoreevent[1] + scoreevent[2],
                    scoreevent[3],
                    scoreevent[4],
                    scoreevent[5],
                ]
                if note_on_event[1] in time2events:
                    time2events[note_on_event[1]].append(note_on_event)
                else:
                    time2events[note_on_event[1]] = [note_on_event]
                if note_off_event[1] in time2events:
                    time2events[note_off_event[1]].append(note_off_event)
                else:
                    time2events[note_off_event[1]] = [note_off_event]
            else:
                if scoreevent[1] in time2events:
                    time2events[scoreevent[1]].append(scoreevent)
                else:
                    time2events[scoreevent[1]] = [scoreevent]

        sorted_times = sorted(time2events.keys())
        sorted_events = []
        for time in sorted_times:
            sorted_events.extend(time2events[time])

        abs_time = 0
        for event in sorted_events:
            delta_time = event[1] - abs_time
            abs_time = event[1]
            event[1] = delta_time
        opus_tracks.append(sorted_events)

    opus_tracks.insert(0, ticks)
    _clean_up_warnings()
    return opus_tracks


def score2midi(score: list | None = None, text_encoding: str = "ISO-8859-1") -> bytes:
    """Convert score to MIDI bytes."""
    return opus2midi(score2opus(score, text_encoding), text_encoding)


def midi2opus(midi: bytes = b"", do_not_check_MIDI_signature: bool = False) -> list:
    """Convert MIDI bytes to opus."""
    my_midi = bytearray(midi)
    if len(my_midi) < 4:
        _clean_up_warnings()
        return [1000, []]
    id = bytes(my_midi[0:4])
    if id != b"MThd":
        _warn("midi2opus: midi starts with " + str(id) + " instead of 'MThd'")
        _clean_up_warnings()
        if not do_not_check_MIDI_signature:
            return [1000, []]
    length, format, tracks_expected, ticks = struct.unpack(">IHHH", bytes(my_midi[4:14]))
    if length != 6:
        _warn("midi2opus: midi header length was " + str(length) + " instead of 6")
        _clean_up_warnings()
        return [1000, []]

    my_opus = [ticks]
    my_midi = my_midi[14:]
    track_num = 1

    while len(my_midi) >= 8:
        track_type = bytes(my_midi[0:4])
        track_length = struct.unpack(">I", my_midi[4:8])[0]
        my_midi = my_midi[8:]
        if track_length > len(my_midi):
            _warn(f"midi2opus: track #{track_num} length {track_length} is too large")
            _clean_up_warnings()
            return my_opus
        my_midi_track = my_midi[0:track_length]
        my_track = _decode(my_midi_track)
        my_opus.append(my_track)
        my_midi = my_midi[track_length:]
        track_num += 1

    _clean_up_warnings()
    return my_opus


def opus2score(opus: list | None = None) -> list:
    """Convert opus to score."""
    if opus is None or len(opus) < 2:
        _clean_up_warnings()
        return [1000, []]
    tracks = copy.deepcopy(opus)
    ticks = int(tracks.pop(0))
    score = [ticks]

    for opus_track in tracks:
        ticks_so_far = 0
        score_track = []
        chapitch2note_on_events = {}

        for opus_event in opus_track:
            ticks_so_far += opus_event[1]
            if opus_event[0] == "note_off" or (opus_event[0] == "note_on" and opus_event[4] == 0):
                cha = opus_event[2]
                pitch = opus_event[3]
                key = cha * 128 + pitch
                if key in chapitch2note_on_events and chapitch2note_on_events[key]:
                    new_event = chapitch2note_on_events[key].pop(0)
                    new_event[2] = ticks_so_far - new_event[1]
                    score_track.append(new_event)
            elif opus_event[0] == "note_on":
                cha = opus_event[2]
                pitch = opus_event[3]
                key = cha * 128 + pitch
                new_event = ["note", ticks_so_far, 0, cha, pitch, opus_event[4]]
                if key in chapitch2note_on_events:
                    chapitch2note_on_events[key].append(new_event)
                else:
                    chapitch2note_on_events[key] = [new_event]
            else:
                opus_event[1] = ticks_so_far
                score_track.append(opus_event)

        # Handle unterminated notes
        for chapitch in chapitch2note_on_events:
            note_on_events = chapitch2note_on_events[chapitch]
            for new_e in note_on_events:
                new_e[2] = ticks_so_far - new_e[1]
                score_track.append(new_e)

        score.append(score_track)

    _clean_up_warnings()
    return score


def midi2score(midi: bytes = b"", do_not_check_MIDI_signature: bool = False) -> list:
    """Convert MIDI bytes to score."""
    return opus2score(midi2opus(midi, do_not_check_MIDI_signature))


# =============================================================================
# Timing Conversion
# =============================================================================


def to_millisecs(
    old_opus: list | None = None, desired_time_in_ms: int = 1, pass_old_timings_events: bool = False
) -> list:
    """Convert opus timings to milliseconds."""
    if old_opus is None:
        return [1000 * desired_time_in_ms, []]
    try:
        old_tpq = int(old_opus[0])
    except IndexError:
        _warn("to_millisecs: the opus has no elements")
        return [1000 * desired_time_in_ms, []]

    new_opus = [1000 * desired_time_in_ms]

    # Build table of set_tempos by absolute tick
    ticks2tempo = {}
    itrack = 1
    while itrack < len(old_opus):
        ticks_so_far = 0
        for old_event in old_opus[itrack]:
            if old_event[0] == "note":
                raise TypeError("to_millisecs needs an opus, not a score")
            ticks_so_far += old_event[1]
            if old_event[0] == "set_tempo":
                ticks2tempo[ticks_so_far] = old_event[2]
        itrack += 1

    tempo_ticks = sorted(ticks2tempo.keys())

    itrack = 1
    while itrack < len(old_opus):
        ms_per_old_tick = 400 / old_tpq
        i_tempo_ticks = 0
        ticks_so_far = 0
        ms_so_far = 0.0
        previous_ms_so_far = 0.0

        if pass_old_timings_events:
            new_track = [["set_tempo", 0, 1000000 * desired_time_in_ms], ["old_tpq", 0, old_tpq]]
        else:
            new_track = [["set_tempo", 0, 1000000 * desired_time_in_ms]]

        for old_event in old_opus[itrack]:
            event_delta_ticks = old_event[1] * desired_time_in_ms
            if i_tempo_ticks < len(tempo_ticks) and tempo_ticks[i_tempo_ticks] < (
                ticks_so_far + old_event[1]
            ) * desired_time_in_ms:
                delta_ticks = tempo_ticks[i_tempo_ticks] - ticks_so_far
                ms_so_far += ms_per_old_tick * delta_ticks * desired_time_in_ms
                ticks_so_far = tempo_ticks[i_tempo_ticks]
                ms_per_old_tick = ticks2tempo[ticks_so_far] / (1000.0 * old_tpq * desired_time_in_ms)
                i_tempo_ticks += 1
                event_delta_ticks -= delta_ticks

            new_event = copy.deepcopy(old_event)
            ms_so_far += ms_per_old_tick * old_event[1] * desired_time_in_ms
            new_event[1] = round(ms_so_far - previous_ms_so_far)

            if pass_old_timings_events:
                if old_event[0] != "set_tempo":
                    previous_ms_so_far = ms_so_far
                    new_track.append(new_event)
                else:
                    new_event[0] = "old_set_tempo"
                    previous_ms_so_far = ms_so_far
                    new_track.append(new_event)
            else:
                if old_event[0] != "set_tempo":
                    previous_ms_so_far = ms_so_far
                    new_track.append(new_event)

            ticks_so_far += event_delta_ticks

        new_opus.append(new_track)
        itrack += 1

    _clean_up_warnings()
    return new_opus


def midi2single_track_ms_score(
    midi_path_or_bytes,
    recalculate_channels: bool = False,
    pass_old_timings_events: bool = False,
    verbose: bool = False,
    do_not_check_MIDI_signature: bool = False,
) -> list:
    """Convert MIDI to single-track millisecond score."""
    if isinstance(midi_path_or_bytes, bytes):
        midi_data = midi_path_or_bytes
    elif isinstance(midi_path_or_bytes, str):
        midi_data = open(midi_path_or_bytes, "rb").read()
    else:
        raise TypeError("Expected bytes or str path")

    score = midi2score(midi_data, do_not_check_MIDI_signature)

    if recalculate_channels:
        events_matrixes = []
        itrack = 1
        events_matrixes_channels = []

        while itrack < len(score):
            events_matrix = []
            for event in score[itrack]:
                if event[0] == "note" and event[3] != 9:
                    event[3] = (16 * (itrack - 1)) + event[3]
                    if event[3] not in events_matrixes_channels:
                        events_matrixes_channels.append(event[3])
                events_matrix.append(event)
            events_matrixes.append(events_matrix)
            itrack += 1

        events_matrix1 = []
        for e in events_matrixes:
            events_matrix1.extend(e)

        if verbose and len(events_matrixes_channels) > 16:
            print(
                f"MIDI has {len(events_matrixes_channels)} instruments! "
                f"{len(events_matrixes_channels) - 16} instrument(s) will be removed!"
            )

        for e in events_matrix1:
            if e[0] == "note" and e[3] != 9:
                if e[3] in events_matrixes_channels[:15]:
                    idx = events_matrixes_channels[:15].index(e[3])
                    e[3] = idx if idx < 9 else idx + 1
                else:
                    events_matrix1.remove(e)

            if e[0] in [
                "patch_change",
                "control_change",
                "channel_after_touch",
                "key_after_touch",
                "pitch_wheel_change",
            ] and e[2] != 9:
                mod_channels = [c % 16 for c in events_matrixes_channels[:15]]
                if e[2] in mod_channels:
                    idx = mod_channels.index(e[2])
                    e[2] = idx if idx < 9 else idx + 1
                else:
                    events_matrix1.remove(e)
    else:
        events_matrix1 = []
        itrack = 1
        while itrack < len(score):
            for event in score[itrack]:
                events_matrix1.append(event)
            itrack += 1

    opus = score2opus([score[0], events_matrix1])
    ms_score = opus2score(to_millisecs(opus, pass_old_timings_events=pass_old_timings_events))

    return ms_score


# =============================================================================
# Score Processing Functions (TMIDIX)
# =============================================================================


def ordered_set(seq: list) -> list:
    """Return unique items preserving order."""
    dic = {}
    return [k for k, v in dic.fromkeys(seq).items()]


def ordered_groups(data: list, ptc_idx: int, pat_idx: int) -> list:
    """Group data by (pitch, patch) keys preserving order."""
    groups = OrderedDict()
    for sublist in data:
        key = tuple([sublist[ptc_idx], sublist[pat_idx]])
        if key not in groups:
            groups[key] = []
        groups[key].append(sublist)
    return list(groups.items())


def compute_sustain_intervals(events: list) -> list:
    """Compute sustain pedal intervals from control change events."""
    intervals = []
    pedal_on = False
    current_start = None

    for t, cc in events:
        if not pedal_on and cc >= 64:
            pedal_on = True
            current_start = t
        elif pedal_on and cc < 64:
            pedal_on = False
            intervals.append((current_start, t))
            current_start = None

    if pedal_on:
        intervals.append((current_start, float("inf")))

    # Merge overlapping intervals
    merged = []
    for interval in intervals:
        if merged and interval[0] <= merged[-1][1]:
            merged[-1] = (merged[-1][0], max(merged[-1][1], interval[1]))
        else:
            merged.append(interval)
    return merged


def apply_sustain_to_ms_score(score: list) -> list:
    """Apply sustain pedal events to note durations."""
    sustain_by_channel = {}

    for track in score[1:]:
        for event in track:
            if event[0] == "control_change" and event[3] == 64:
                channel = event[2]
                sustain_by_channel.setdefault(channel, []).append((event[1], event[4]))

    sustain_intervals_by_channel = {}

    for channel, events in sustain_by_channel.items():
        events.sort(key=lambda x: x[0])
        sustain_intervals_by_channel[channel] = compute_sustain_intervals(events)

    global_max_off = 0

    for track in score[1:]:
        for event in track:
            if event[0] == "note":
                global_max_off = max(global_max_off, event[1] + event[2])

    for channel, intervals in sustain_intervals_by_channel.items():
        updated_intervals = []
        for start, end in intervals:
            if end == float("inf"):
                end = global_max_off
            updated_intervals.append((start, end))
        sustain_intervals_by_channel[channel] = updated_intervals

    if sustain_intervals_by_channel:
        for track in score[1:]:
            for event in track:
                if event[0] == "note":
                    start = event[1]
                    nominal_dur = event[2]
                    nominal_off = start + nominal_dur
                    channel = event[3]

                    intervals = sustain_intervals_by_channel.get(channel, [])
                    effective_off = nominal_off

                    for intv_start, intv_end in intervals:
                        if intv_start < nominal_off < intv_end:
                            effective_off = intv_end
                            break

                    event[2] = effective_off - start

    return score


def chordify_score(
    score: list,
    return_chordified_score: bool = True,
    return_detected_score_information: bool = False,
) -> list | None:
    """Group score notes into chords by timing."""
    if not score:
        return None

    num_tracks = 1
    single_track_score = []
    score_num_ticks = 0

    if isinstance(score[0], int) and len(score) > 1:
        score_type = "MIDI_PY"
        score_num_ticks = score[0]

        while num_tracks < len(score):
            for event in score[num_tracks]:
                single_track_score.append(event)
            num_tracks += 1
    else:
        score_type = "CUSTOM"
        single_track_score = score

    if not single_track_score or not single_track_score[0]:
        return None

    try:
        if isinstance(single_track_score[0][0], str) or single_track_score[0][0] == "note":
            single_track_score.sort(key=lambda x: x[1])
            score_timings = [s[1] for s in single_track_score]
        else:
            score_timings = [s[0] for s in single_track_score]

        is_absolute = all(x <= y for x, y in zip(score_timings, score_timings[1:]))

        if is_absolute:
            score_timings_type = "ABS"
            chords = []
            cho = []

            pe = single_track_score[0]

            for e in single_track_score:
                if score_type == "MIDI_PY":
                    time, ptime = e[1], pe[1]
                else:
                    time, ptime = e[0], pe[0]

                if time == ptime:
                    cho.append(e)
                else:
                    if cho:
                        chords.append(cho)
                    cho = [e]

                pe = e

            if cho:
                chords.append(cho)
        else:
            score_timings_type = "REL"
            chords = []
            cho = []

            for e in single_track_score:
                time = e[1] if score_type == "MIDI_PY" else e[0]

                if time == 0:
                    cho.append(e)
                else:
                    if cho:
                        chords.append(cho)
                    cho = [e]

            if cho:
                chords.append(cho)

        requested_data = []

        if return_detected_score_information:
            detected_score_information = [
                ["Score type", score_type],
                ["Score timings type", score_timings_type],
                ["Score tpq", score_num_ticks],
                ["Score number of tracks", num_tracks],
            ]
            requested_data.append(detected_score_information)

        if return_chordified_score and return_detected_score_information:
            requested_data.append(chords)
        elif return_chordified_score and not return_detected_score_information:
            requested_data.extend(chords)

        return requested_data

    except Exception as e:
        print("Error! Check score for consistency and compatibility!")
        print("Exception detected:", e)
        return None


def advanced_score_processor(
    raw_score: list,
    patches_to_analyze: list | None = None,
    return_score_analysis: bool = False,
    return_enhanced_score: bool = False,
    return_enhanced_score_notes: bool = False,
    return_enhanced_monophonic_melody: bool = False,
    return_chordified_enhanced_score: bool = False,
    return_chordified_enhanced_score_with_lyrics: bool = False,
    return_score_tones_chords: bool = False,
    return_text_and_lyric_events: bool = False,
    apply_sustain: bool = False,
) -> list:
    """Process raw MIDI score with various enhancements."""
    if patches_to_analyze is None:
        patches_to_analyze = list(range(129))

    if raw_score and isinstance(raw_score, list):
        num_ticks = 0
        num_tracks = 1
        basic_single_track_score = []

        if not isinstance(raw_score[0], int):
            if len(raw_score[0]) < 5 and not isinstance(raw_score[0][0], str):
                return ["Check score for errors and compatibility!"]
            else:
                basic_single_track_score = copy.deepcopy(raw_score)
        else:
            num_ticks = raw_score[0]
            while num_tracks < len(raw_score):
                for event in raw_score[num_tracks]:
                    ev = copy.deepcopy(event)
                    basic_single_track_score.append(ev)
                num_tracks += 1

        for e in basic_single_track_score:
            if e[0] == "note":
                e[3] = e[3] % 16
                e[4] = e[4] % 128
                e[5] = e[5] % 128
            if e[0] == "patch_change":
                e[2] = e[2] % 16
                e[3] = e[3] % 128

        if apply_sustain:
            apply_sustain_to_ms_score([1000, basic_single_track_score])

        basic_single_track_score.sort(key=lambda x: x[4] if x[0] == "note" else 128, reverse=True)
        basic_single_track_score.sort(key=lambda x: x[1])

        enhanced_single_track_score = []
        patches = [0] * 16
        all_score_patches = []
        num_patch_changes = 0

        for event in basic_single_track_score:
            if event[0] == "patch_change":
                patches[event[2]] = event[3]
                enhanced_single_track_score.append(event)
                num_patch_changes += 1

            if event[0] == "note":
                if event[3] != 9:
                    event.extend([patches[event[3]]])
                    all_score_patches.extend([patches[event[3]]])
                else:
                    event.extend([128])
                    all_score_patches.extend([128])

                if enhanced_single_track_score:
                    if event[1] == enhanced_single_track_score[-1][1]:
                        if [event[3], event[4]] != enhanced_single_track_score[-1][3:5]:
                            enhanced_single_track_score.append(event)
                    else:
                        enhanced_single_track_score.append(event)
                else:
                    enhanced_single_track_score.append(event)

            if event[0] not in ["note", "patch_change"]:
                enhanced_single_track_score.append(event)

        enhanced_single_track_score.sort(key=lambda x: x[6] if x[0] == "note" else -1)
        enhanced_single_track_score.sort(key=lambda x: x[4] if x[0] == "note" else 128, reverse=True)
        enhanced_single_track_score.sort(key=lambda x: x[1])

        score_notes = [
            s for s in enhanced_single_track_score if s[0] == "note" and s[6] in patches_to_analyze
        ]

        requested_data = []

        if return_enhanced_score_notes and score_notes:
            requested_data.append(score_notes)

        return requested_data
    else:
        return ["Check score for errors and compatibility!"]


def augment_enhanced_score_notes(
    enhanced_score_notes: list,
    timings_divider: int = 16,
    full_sorting: bool = True,
    timings_shift: int = 0,
    pitch_shift: int = 0,
    ceil_timings: bool = False,
    round_timings: bool = False,
    legacy_timings: bool = True,
    sort_drums_last: bool = False,
    even_timings: bool = False,
) -> list:
    """Augment enhanced score notes with timing adjustments."""
    esn = copy.deepcopy(enhanced_score_notes)
    pe = enhanced_score_notes[0]
    abs_time = max(0, int(enhanced_score_notes[0][1] / timings_divider))

    for i, e in enumerate(esn):
        dtime = (e[1] / timings_divider) - (pe[1] / timings_divider)

        if round_timings:
            dtime = round(dtime)
        elif ceil_timings:
            dtime = math.ceil(dtime)
        else:
            dtime = int(dtime)

        if legacy_timings:
            abs_time = int(e[1] / timings_divider) + timings_shift
        else:
            abs_time += dtime

        e[1] = max(0, abs_time + timings_shift)

        if round_timings:
            e[2] = max(1, round(e[2] / timings_divider)) + timings_shift
        elif ceil_timings:
            e[2] = max(1, math.ceil(e[2] / timings_divider)) + timings_shift
        else:
            e[2] = max(1, int(e[2] / timings_divider)) + timings_shift

        e[4] = max(1, min(127, e[4] + pitch_shift))
        pe = enhanced_score_notes[i]

    if even_timings:
        for e in esn:
            if e[1] % 2 != 0:
                e[1] += 1
            if e[2] % 2 != 0:
                e[2] += 1

    if full_sorting:
        esn.sort(key=lambda x: x[6])
        esn.sort(key=lambda x: x[4], reverse=True)
        esn.sort(key=lambda x: x[1])

    if sort_drums_last:
        esn.sort(key=lambda x: (x[1], -x[4], x[6]) if x[6] != 128 else (x[1], x[6], -x[4]))

    return esn


def remove_duplicate_pitches_from_escore_notes(
    escore_notes: list,
    pitches_idx: int = 4,
    patches_idx: int = 6,
    return_dupes_count: bool = False,
) -> list | int:
    """Remove duplicate pitch/patch combinations from each chord."""
    cscore = chordify_score([1000, escore_notes])
    new_escore = []
    bp_count = 0

    for c in cscore:
        cho = []
        seen = []

        for cc in c:
            if [cc[pitches_idx], cc[patches_idx]] not in seen:
                cho.append(cc)
                seen.append([cc[pitches_idx], cc[patches_idx]])
            else:
                bp_count += 1

        new_escore.extend(cho)

    if return_dupes_count:
        return bp_count
    else:
        return new_escore


def fix_monophonic_score_durations(
    monophonic_score: list,
    min_notes_gap: int = 1,
    min_notes_dur: int = 1,
    extend_durs: bool = False,
) -> list:
    """Fix overlapping durations in a monophonic score."""
    fixed_score = []

    if monophonic_score[0][0] == "note":
        for i in range(len(monophonic_score) - 1):
            note = monophonic_score[i]
            nmt = monophonic_score[i + 1][1]

            if note[1] + note[2] >= nmt:
                note_dur = max(1, nmt - note[1] - min_notes_gap)
            elif extend_durs:
                note_dur = max(1, nmt - note[1] - min_notes_gap)
            else:
                note_dur = note[2]

            new_note = [note[0], note[1], note_dur] + note[3:]

            if new_note[2] >= min_notes_dur:
                fixed_score.append(new_note)

        if monophonic_score[-1][2] >= min_notes_dur:
            fixed_score.append(monophonic_score[-1])

    elif isinstance(monophonic_score[0][0], int):
        for i in range(len(monophonic_score) - 1):
            note = monophonic_score[i]
            nmt = monophonic_score[i + 1][0]

            if note[0] + note[1] >= nmt:
                note_dur = max(1, nmt - note[0] - min_notes_gap)
            elif extend_durs:
                note_dur = max(1, nmt - note[0] - min_notes_gap)
            else:
                note_dur = note[1]

            new_note = [note[0], note_dur] + note[2:]

            if new_note[1] >= min_notes_dur:
                fixed_score.append(new_note)

        if monophonic_score[-1][1] >= min_notes_dur:
            fixed_score.append(monophonic_score[-1])

    return fixed_score


def fix_escore_notes_durations(
    escore_notes: list,
    min_notes_gap: int = 1,
    min_notes_dur: int = 1,
    times_idx: int = 1,
    durs_idx: int = 2,
    channels_idx: int = 3,
    pitches_idx: int = 4,
    patches_idx: int = 6,
) -> list:
    """Fix overlapping durations grouped by pitch/patch."""
    notes = [e for e in escore_notes if e[channels_idx] != 9]
    drums = [e for e in escore_notes if e[channels_idx] == 9]

    escore_groups = ordered_groups(notes, pitches_idx, patches_idx)

    merged_score = []

    for k, g in escore_groups:
        if len(g) > 2:
            fg = fix_monophonic_score_durations(
                g, min_notes_gap=min_notes_gap, min_notes_dur=min_notes_dur
            )
            merged_score.extend(fg)
        elif len(g) == 2:
            if g[0][times_idx] + g[0][durs_idx] >= g[1][times_idx]:
                g[0][durs_idx] = max(1, g[1][times_idx] - g[0][times_idx] - min_notes_gap)
            merged_score.extend(g)
        else:
            merged_score.extend(g)

    return sorted(merged_score + drums, key=lambda x: x[times_idx])


def delta_score_notes(
    score_notes: list,
    timings_clip_value: int = 255,
    even_timings: bool = False,
    compress_timings: bool = False,
) -> list:
    """Convert absolute timings to delta (relative) timings."""
    delta_score = []
    pe = score_notes[0]

    for n in score_notes:
        note = copy.deepcopy(n)
        time = n[1] - pe[1]
        dur = n[2]

        if even_timings:
            if time != 0 and time % 2 != 0:
                time += 1
            if dur % 2 != 0:
                dur += 1

        time = max(0, min(timings_clip_value, time))
        dur = max(0, min(timings_clip_value, dur))

        if compress_timings:
            time /= 2
            dur /= 2

        note[1] = int(time)
        note[2] = int(dur)

        delta_score.append(note)
        pe = n

    return delta_score


def patch_enhanced_score_notes(
    escore_notes: list,
    default_patch: int = 0,
    reserved_patch: int = -1,
    reserved_patch_channel: int = -1,
    drums_patch: int = 9,
    verbose: bool = False,
) -> tuple[list, list, list]:
    """Assign patches to channels with overflow handling."""
    enhanced_score_notes = copy.deepcopy(escore_notes)
    enhanced_score_notes_with_patch_changes = []
    patches = [-1] * 16

    if -1 < reserved_patch < 128 and -1 < reserved_patch_channel < 128:
        patches[reserved_patch_channel] = reserved_patch

    overflow_idx = -1

    for idx, e in enumerate(enhanced_score_notes):
        if e[0] == "note":
            if e[3] != 9:
                if -1 < reserved_patch < 128 and -1 < reserved_patch_channel < 128:
                    if e[6] == reserved_patch:
                        e[3] = reserved_patch_channel

                if patches[e[3]] == -1:
                    patches[e[3]] = e[6]
                else:
                    if patches[e[3]] != e[6]:
                        if e[6] in patches:
                            e[3] = patches.index(e[6])
                        else:
                            if -1 in patches:
                                patches[patches.index(-1)] = e[6]
                            else:
                                overflow_idx = idx
                                break

        enhanced_score_notes_with_patch_changes.append(e)

    overflow_patches = []
    overflow_channels = [-1] * 16
    overflow_channels[9] = drums_patch

    if -1 < reserved_patch < 128 and -1 < reserved_patch_channel < 128:
        overflow_channels[reserved_patch_channel] = reserved_patch

    if overflow_idx != -1:
        for idx, e in enumerate(enhanced_score_notes[overflow_idx:]):
            if e[0] == "note":
                if e[3] != 9:
                    if e[6] not in overflow_channels:
                        if -1 in overflow_channels:
                            free_chan = overflow_channels.index(-1)
                            overflow_channels[free_chan] = e[6]
                            e[3] = free_chan
                            enhanced_score_notes_with_patch_changes.append(
                                ["patch_change", e[1], e[3], e[6]]
                            )
                            overflow_patches.append(e[6])
                        else:
                            overflow_channels = [-1] * 16
                            overflow_channels[9] = drums_patch

                            if -1 < reserved_patch < 128 and -1 < reserved_patch_channel < 128:
                                overflow_channels[reserved_patch_channel] = reserved_patch
                                e[3] = reserved_patch_channel

                            if e[6] != reserved_patch:
                                free_chan = overflow_channels.index(-1)
                                e[3] = free_chan

                            overflow_channels[e[3]] = e[6]
                            enhanced_score_notes_with_patch_changes.append(
                                ["patch_change", e[1], e[3], e[6]]
                            )
                            overflow_patches.append(e[6])
                    else:
                        e[3] = overflow_channels.index(e[6])

            enhanced_score_notes_with_patch_changes.append(e)

    patches = [p if p != -1 else default_patch for p in patches]
    patches[9] = drums_patch
    overflow_patches = ordered_set(overflow_patches)

    return enhanced_score_notes_with_patch_changes, patches, overflow_patches


# General MIDI patch names (for reference)
Number2patch = {
    0: "Acoustic Grand",
    1: "Bright Acoustic",
    2: "Electric Grand",
    3: "Honky-Tonk",
    4: "Electric Piano 1",
    5: "Electric Piano 2",
    6: "Harpsichord",
    7: "Clav",
    8: "Celesta",
    9: "Glockenspiel",
    10: "Music Box",
    11: "Vibraphone",
    12: "Marimba",
    13: "Xylophone",
    14: "Tubular Bells",
    15: "Dulcimer",
    16: "Drawbar Organ",
    17: "Percussive Organ",
    18: "Rock Organ",
    19: "Church Organ",
    20: "Reed Organ",
    21: "Accordion",
    22: "Harmonica",
    23: "Tango Accordion",
    24: "Acoustic Guitar(nylon)",
    25: "Acoustic Guitar(steel)",
    26: "Electric Guitar(jazz)",
    27: "Electric Guitar(clean)",
    28: "Electric Guitar(muted)",
    29: "Overdriven Guitar",
    30: "Distortion Guitar",
    31: "Guitar Harmonics",
    32: "Acoustic Bass",
    33: "Electric Bass(finger)",
    34: "Electric Bass(pick)",
    35: "Fretless Bass",
    36: "Slap Bass 1",
    37: "Slap Bass 2",
    38: "Synth Bass 1",
    39: "Synth Bass 2",
    40: "Violin",
    41: "Viola",
    42: "Cello",
    43: "Contrabass",
    44: "Tremolo Strings",
    45: "Pizzicato Strings",
    46: "Orchestral Harp",
    47: "Timpani",
    48: "String Ensemble 1",
    49: "String Ensemble 2",
    50: "SynthStrings 1",
    51: "SynthStrings 2",
    52: "Choir Aahs",
    53: "Voice Oohs",
    54: "Synth Voice",
    55: "Orchestra Hit",
    56: "Trumpet",
    57: "Trombone",
    58: "Tuba",
    59: "Muted Trumpet",
    60: "French Horn",
    61: "Brass Section",
    62: "SynthBrass 1",
    63: "SynthBrass 2",
    64: "Soprano Sax",
    65: "Alto Sax",
    66: "Tenor Sax",
    67: "Baritone Sax",
    68: "Oboe",
    69: "English Horn",
    70: "Bassoon",
    71: "Clarinet",
    72: "Piccolo",
    73: "Flute",
    74: "Recorder",
    75: "Pan Flute",
    76: "Blown Bottle",
    77: "Shakuhachi",
    78: "Whistle",
    79: "Ocarina",
    80: "Lead 1 (square)",
    81: "Lead 2 (sawtooth)",
    82: "Lead 3 (calliope)",
    83: "Lead 4 (chiff)",
    84: "Lead 5 (charang)",
    85: "Lead 6 (voice)",
    86: "Lead 7 (fifths)",
    87: "Lead 8 (bass+lead)",
    88: "Pad 1 (new age)",
    89: "Pad 2 (warm)",
    90: "Pad 3 (polysynth)",
    91: "Pad 4 (choir)",
    92: "Pad 5 (bowed)",
    93: "Pad 6 (metallic)",
    94: "Pad 7 (halo)",
    95: "Pad 8 (sweep)",
    96: "FX 1 (rain)",
    97: "FX 2 (soundtrack)",
    98: "FX 3 (crystal)",
    99: "FX 4 (atmosphere)",
    100: "FX 5 (brightness)",
    101: "FX 6 (goblins)",
    102: "FX 7 (echoes)",
    103: "FX 8 (sci-fi)",
    104: "Sitar",
    105: "Banjo",
    106: "Shamisen",
    107: "Koto",
    108: "Kalimba",
    109: "Bagpipe",
    110: "Fiddle",
    111: "Shanai",
    112: "Tinkle Bell",
    113: "Agogo",
    114: "Steel Drums",
    115: "Woodblock",
    116: "Taiko Drum",
    117: "Melodic Tom",
    118: "Synth Drum",
    119: "Reverse Cymbal",
    120: "Guitar Fret Noise",
    121: "Breath Noise",
    122: "Seashore",
    123: "Bird Tweet",
    124: "Telephone Ring",
    125: "Helicopter",
    126: "Applause",
    127: "Gunshot",
}
