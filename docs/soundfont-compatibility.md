# SoundFont Compatibility with RustySynth

RustySynth is used for MIDI → WAV rendering via the `midi_to_wav` MCP tool.

## Compatibility Summary

**Tested:** 24 SoundFonts from `/home/atobey/midi/SF2/unzip/`
**Working:** 21 fonts (87.5%)
**Failed:** 3 fonts (12.5%)

## Working SoundFonts ✓

All of these load successfully and can be used for rendering:

### Drum Kits
- `Drums_TR-808.sf2` (397 KB)
- `HS_Acoustic_Percussion.sf2` (472 KB)
- `HS_African_Percussion.sf2` (226 KB)
- `HS_Boss_DR-550_Drums.sf2` (763 KB)
- `HS_Linn_Drums.sf2` (532 KB)
- `HS_M1_Drums.sf2` (762 KB)
- `HS_Magic_Techno_Drums.sf2` (1202 KB)
- `HS_R8_Drums.sf2` (542 KB)

### Synthesizers & Effects
- `HS_Pads_Textures_I.sf2` (1800 KB)
- `HS_Pads_Textures_II.sf2` (3224 KB)
- `HS_StarTrekFX.sf2` (788 KB)
- `HS_Strings.sf2` (983 KB)
- `HS_Synth_Collection_I.sf2` (3042 KB)
- `HS_Synthetic_Electronic.sf2` (222 KB)
- `HS_TB-303.sf2` (1091 KB)
- `HS_Vox.sf2` (456 KB)

### General Purpose
- `Setzer.sf2` (1169 KB)
- `Timber.sf2` (32934 KB) - Large general font
- `Unison.sf2` (28572 KB) - Large general font

### Game Soundtracks
- `ff4sf2.sf2` (323 KB) - Final Fantasy IV
- `ff6.SF2` (512 KB) - Final Fantasy VI

## Incompatible SoundFonts ✗

These fail with `SanityCheckFailed` error:

- `Square.SF2` - Reason unknown
- `Vintage_Dreams_Waves_v2.sf2` - Likely uses advanced SF2 features
- `ff7s.SF2` (532 KB) - Final Fantasy VII (interesting that FF4/FF6 work!)

## Error Messages

When an incompatible SoundFont is used, the tool now returns:

```
SoundFont failed compatibility check (RustySynth SanityCheckFailed).
This SF2 may use features not supported by RustySynth.
Try a simpler SoundFont like GeneralUser GS, FluidR3, or TR-808 drums.
```

## Recommendations

For reliable rendering:
1. **Drums:** TR-808, Boss DR-550, or any HS_* percussion
2. **Synths:** TB-303, Synth Collection I, Pads & Textures
3. **General:** Timber (32MB) or Unison (28MB) for full GM sets
4. **Retro games:** FF4 or FF6 fonts

## Technical Notes

RustySynth performs sanity checks on SF2 files during loading. The exact reasons for failures aren't exposed by the library, but incompatibility typically indicates:
- Non-standard SF2 extensions
- Corrupted or malformed data
- Features beyond SoundFont 2.01 spec

**Tested:** 2025-11-22
**RustySynth version:** 1.3.6
