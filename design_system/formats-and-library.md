# Formats and the library

Where maps and songs come from, what each file type gives the game, and how the maps/ folder is managed. All parsing lives in src/map.rs, importing in src/import.rs, decoding in src/audio.rs and auto-charting in src/chart.rs.

## Map types

Every map loads through Map::load (src/map.rs:67) into one shape: title, mapper string, a list of notes (time in seconds, x and y in -1..1) and optional embedded audio bytes. Anything the loader does not understand is dropped, not guessed.

### .sspm (Sound Space Plus map, v1 and v2)

Binary, magic "SS+m" plus a u16 version (src/map.rs:294).

- v1 (src/map.rs:306): reads title, mapper and note count; notes are millisecond times with positions either as two f32 (quantum) or two u8 grid bytes, remapped to -1..1 (x - 1.0, 1.0 - y). Reads embedded audio when the flag is set. Skips the cover image bytes (src/map.rs:314): covers are not shown anywhere. Difficulty and the song id lines are skipped.
- v2 (src/map.rs:339): reads title, a mapper list (joined with ", "), notes (same encoding as v1) and audio through the offset/length pair in the header. Skips the id, hash, difficulty and cover blocks.
- Any other version number is refused with "unsupported SSPM version" (src/map.rs:302).

### .phxm (SoundSpace Quantum / Pulsus map)

Two shapes, one loader branch each.

- Zip file (src/map.rs:456): reads metadata.json (title from Artist + Title, mappers from the Mappers array), objects.phxmo (binary note list, src/map.rs:379) and audio.<ext> when HasAudio is true (AudioExt, default mp3). Extra files in the zip are ignored.
- Unpacked folder (src/map.rs:419): the same three files read from the directory. A folder only counts as a map if metadata.json and objects.phxmo both exist (src/map.rs:101). Other files in the folder are ignored.
- In objects.phxmo, non-quantum positions are stored 1..3 on the grid and remapped; quantum floats are used as-is (src/map.rs:387).

### .rhm (Rhythm-plus map)

A zip (src/map.rs:582). Reads the "map" entry: JSON with Notes (Time in ms, X, Y remapped to -1..1), Title and Mappers. Audio comes from the "audio" entry first, then the file named by AudioFileName (src/map.rs:598). Anything else in the zip is ignored.

### .txt (plain text map)

Comma-separated x|y|time_ms triples after a title field (src/map.rs:165). x and y are remapped (1.0 - x, y - 1.0), time is divided by 1000. A BOM is tolerated, the mapper field stays empty, and one bad triple fails the whole file ("malformed note entry"). Audio is not inside the file: the loader looks next to it for any AUDIO_EXTS file with the same stem and reads the first one found (src/map.rs:188). That companion file is never a separate library entry (has_sibling_map, src/map.rs:109).

### .osu and .osz

Text parser at src/map.rs:477. osu!standard only: a Mode other than 0 is refused (src/map.rs:532). Reads AudioFilename, Title, Artist ("Artist - Title") and Creator as mapper. From HitObjects it keeps circles and slider heads; spinner lines (type bit 8) are dropped, and slider tails and repeats collapse to the head. Malformed lines are skipped, not fatal. Positions map from the 512x384 osu playfield into -1..1, clamped (src/map.rs:523). Timing points, hitsounds and everything in other sections are dropped: no BPM-based effects, no slider bodies.

- .osu file (src/map.rs:545): audio is the AudioFilename file next to the .osu, if present.
- .osz zip (src/map.rs:557): every .osu entry is parsed and the one with the most notes wins ("biggest difficulty"); its AudioFilename is looked up in the zip case-insensitively. If no difficulty parses, the last parser error is reported.

## Audio types

AUDIO_EXTS (src/map.rs:10): mp3, ogg, oga, opus, wav, flac, m4a, aac, mp4, m4b, aiff, aif, caf, mka, webm, wma, wv.

decode (src/audio.rs:19) first hands the bytes to kira/symphonia. That covers mp3, ogg/vorbis, flac, wav, aac, alac, aiff, caf, mp4/m4a/m4b, mka/mkv and adpcm. If symphonia refuses, via_ffmpeg (src/audio.rs:28) writes the bytes to a temp file and runs the ffmpeg binary with fixed arguments (the input path is passed as one argument, never interpolated into a command line), decoding to 48 kHz stereo s16 wav in a temp dir that is removed afterwards. That path covers opus, webm, wma and wv. If ffmpeg is missing or fails too, the error names both failures. Nothing is probed by extension: the bytes decide.

## Audio-only drops become maps

song() (src/map.rs:158): decode the bytes, run chart::generate, save the map with the file stem as title and mapper "auto-generated" (AUTO_MAPPER, src/map.rs:14). The algorithm (src/chart.rs):

1. Mix to mono and run spectral flux: a 1024-point radix-2 FFT (own implementation, chart.rs:29) with a Hann window, hop of sample_rate/100 (10 ms), log magnitudes, positive half-rectified difference between frames (chart.rs:84).
2. Pick onsets from the flux curve (chart.rs:105): a value must beat SENSITIVITY (1.0) times the local mean over SMOOTH_RADIUS (0.5 s), be a local maximum within PEAK_RADIUS (3 frames, 30 ms), and clear FLOOR (0.04) times the global peak. Peaks are then thinned strongest-first so no two onsets sit closer than MIN_GAP (0.12 s) (chart.rs:141).
3. Place notes on the 3x3 grid (layout, chart.rs:170): the step from the previous cell is limited by reach (chart.rs:160) to 1 cell for gaps under 0.22 s, 2 under 0.5 s, 3 beyond. The cell is picked from the allowed ring by a seeded hash mix (chart.rs:152) biased 60/40 between randomness and onset strength, never repeating the previous cell. The seed is length XOR sample rate (chart.rs:26), so the same file always charts the same.

Trade-offs: the 10 ms hop sets the time resolution, so chart times quantize to about 10 ms; MIN_GAP 0.12 s caps density at about 8 notes per second and drops fast double-hits; FLOOR 0.04 drops quiet sections entirely; reach ties hand travel distance to the gap, so dense parts stay playable but big jumps only happen on rests. On very loud or very dense input the strongest-first thinning keeps the loudest hits and discards the rest.

## maps/, trash, scores and settings

- maps/ (MAPS_DIR, src/main.rs:41) holds every library entry. Select::new creates it if missing and scans it with map::scan (src/map.rs:132): maps first-class, audio files only when no same-stem map sits beside them, sorted by lowercase name.
- Delete in the song list calls import::remove (src/import.rs:144): the map file moves into maps/.trash (created on demand), and a .txt drags every same-stem audio file with it. Name clashes in the trash get a " (2)" suffix so two removals of the same name both survive (src/import.rs:166). The demo row refuses removal (src/main.rs:291).
- Z calls import::restore (src/import.rs:173): files move back, newest removal first (a stack on Select, src/main.rs:242). If a same-name file has since been re-added, the restore lands beside it with a " (2)" suffix, never overwriting (tested at src/import.rs:200).
- unique() (src/import.rs:82) is the shared namer: "name (2).ext", "name (3).ext", and so on. copy_file (src/import.rs:100) reuses an existing destination with the same byte length instead of copying again; copy_dir (src/import.rs:110) does the same for folders by comparing objects.phxmo lengths.
- scores.json keeps the best run per song keyed by file name (design_system/README.md "Best scores"), so a remove/undo or remove/re-import cycle keeps the best, and a renamed copy starts fresh.
- settings.json holds the tunables from src/settings.rs; both files sit next to the executable and are written atomically through src/store.rs. Neither is touched by import or remove.
- request_meta (src/main.rs:46) parses each map once on a background thread into the METAS cache for the list rows (title, notes, length, mappers); a missing or broken map just shows its file name.

## Drag and drop, step by step

1. The OS delivers paths. On X11 the vendored miniquad prefers text/uri-list, percent-decodes file:// URIs and accepts folders (design_system/README.md "Adding songs"); other sessions use stock drops. get_dropped_files paths are collected in the main loop (src/main.rs:779).
2. Select::accept (src/main.rs:278) passes them to import::import (src/import.rs:39), which creates maps/ if needed and handles each path independently.
3. Per path (import_one, src/import.rs:55): an audio file with a same-stem map in the drop set or on disk is copied as a companion and counted as audio, not added as an entry. Anything that is neither map nor audio is refused with a message. Everything else is validated by fully loading it with Map::load before any copy; a parse failure is reported and nothing is copied.
4. A valid file is copied into maps/ (folders recursively), reusing a same-size destination or taking a " (2)" name on a clash.
5. The list rescans, selects the last added entry, and the footer shows either "added name (+N audio)" / "added N maps" or the first error with a count of the rest (src/import.rs:16).
6. Re-dragging a file that was deleted re-adds it; the old copy is still in maps/.trash and Z would restore it under a fresh name.
