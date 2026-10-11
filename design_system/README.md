# Rhythm visual direction

A familiar Rhythia playfield, with original glassy feedback. Keep the approach geometry, cursor movement and hit area stable; decorate around them, never shake the camera or blur the aiming surface.

## Reference evidence

Inspected the installed Steam executable's embedded shader text, and the public Rhythia/Client checkout (October 4, 2026 revision). These are different clients: do not claim the Steam defaults are identical. No original art, sounds or shader source are bundled.

Reference concepts retained: scrolling grid space and hit-color interpolation (scripts/spaces/Grid.cs and BaseSpace.cs), 3.75 camera depth and 0.25 parallax (CameraLock.cs), 10-degree deep side panels (scenes/game.tscn), smoothly filled multiplier indicator (PanelLeft.cs). Original implementations live here in Rust.

## Custom layer

- src/postfx.rs: MSAA scene capture; quarter-resolution threshold + separable blur; eighth-resolution broad glow. Composition is additive, without blur on the underlying scene. HUD text and overlays render afterward. F2 toggles bloom and vignette; render targets rebuild on resize.
- src/skin.rs: procedural hollow note mesh, cursor, glow and wave masks. Fonts are bundled separately with their notice.
- src/render.rs: three selectable spaces (F3, shared by library and game): grid (default, Rhythia-style fog-faded floor/ceiling), tunnel (receding rings + lane road, colour follows the last hit) and void (soft haze + drifting dust). Every space ends in the fog colour, never black: do not reintroduce a dark backdrop sprite behind the far opening.
- src/render.rs: short impact rings and expanding halos behind notes, falling fragments, phase reveals. Particle lifetimes follow song time so pausing freezes them. No camera shake, chromatic splitting or screen flashes.
- src/main.rs: animated library indicator, hover selection, bounded list input and truncated long labels. Leave one frame between scenes so a key cannot activate both screens.
- src/sfx.rs: original FM-glass hit, soft descending miss, confirmation chime, multiplier level-up chime, descending failure and ascending clear motifs. Samples are generated once and reused by Kira.

## Controls and verification

F2 glow on/off. F12 writes a native framebuffer screenshot to screenshots/. Existing aiming, offset, retry and pause controls are unchanged.

Use cargo clippy -- -D warnings and cargo build --release. Visually inspect gameplay, successful completion, failure, pause/resume, library return and resizing. A centered 24-note fixture at 350ms intervals should finish with 13,600 score / 100.00% / 24 hits / 0 misses. Five uninterrupted corner misses should fail. Run fixtures through the actual executable, not copied game logic.

The reference feel is more important than an exact skin clone. Keep glow restrained, text crisp, colors connected across space and impact effects; do not let animation affect hit timing.

## Settings

src/settings.rs owns every tunable (sensitivity, offset, approach rate/distance, parallax, field of view, note and cursor size, volumes, glow, background). Open it from the song list with Tab or the SETTINGS label. Values save to ./settings.json on leaving the screen, leaving a song and quitting; F2/F3 and the in-game [ ] and - = keys also persist. Add a new tunable by adding a Field, a ROWS entry, and its get/set/show arms; the screen, file format and clamping follow from the table.

## Adding songs

Drop files or folders onto the window while the song list is open. src/import.rs validates each one by loading it, then copies it into ./maps (same-size duplicates are reused, other name clashes get a ' (2)' suffix). Accepts .sspm (v1 and v2), .phxm (zip or unpacked folder), .rhm and .txt; audio files (.mp3 .ogg .wav .flac) dropped alongside a .txt are copied as its companion. Bad files are reported in the footer and nothing is copied. File managers (Dolphin, GTK) offer drags as text/uri-list, which stock miniquad 0.4.11 refuses (it only accepts UTF8_STRING), so the drop was rejected. vendor/miniquad is a patched copy wired in through [patch.crates-io] in Cargo.toml: it prefers text/uri-list, percent-decodes file:// URIs (src/native/linux_x11/uri_list.rs), accepts folders and no longer reads dropped files into memory. X11 only; Wayland sessions are untouched. Tested with a scripted XDND source (same protocol messages a file manager sends) dropping a real .sspm.

## Latency

Measured with RHYTHM_PERF=1 (prints frame, CPU, audio-callback and clock stats when a song ends or you leave it; spikes over 12 ms frame or 4 ms CPU are listed). The bottom-left line shows the same numbers live.

- Late input sampling: vendor/miniquad gains window::set_frame_delay, which sleeps at the top of the loop before events are read. src/perf.rs (Pacer) raises it 0.5 ms per 10 frames up to frame interval minus 2.5 ms, and caps it 1 ms below any delay that misses a refresh. Only active while a song is playing; Settings > Low latency mode turns it off.
- Audio buffer: Settings > Audio buffer asks the output device for a fixed buffer (default 256 frames, about 5 ms; the system default measured about 13 ms). 0 = system default. If it crackles, raise it.
- Song clock: src/audio.rs keeps a free-running wall clock and re-anchors it to the audio position with a max filter over the last 3 s (slew-limited), instead of nudging toward stale positions. Frame-to-frame jitter dropped from about 0.35 ms rms to under 0.01 ms.
- Text: every font size is warmed on first use (Skin::ensure) so no glyph is rasterized mid-song; a new glyph re-uploads the whole atlas and showed up as 4 to 25 ms hitches.
- Cursor trail is a setting and off by default.

## Latency, round two

Council verdict applied (hit feel first, measure, bypass the compositor, calibrate). Measured on an RTX 4060 (proprietary NVIDIA driver) with KWin compositing on, 144 Hz, a real 24 s map with No fail on.

- Hit flash: a filled rounded cell flash (about 90 ms, additive, tinted towards white) is drawn on the same frame as every hit, on top of the existing ring, halo and pieces (Skin::fill, draw_effects).
- Swap timing: the loop times next_frame().await; swap block = that minus the pacer delay. With the pacer off the thread idles about 6.15 ms of every 6.94 ms frame inside the swap (input is read right after vblank, one frame early). With it on it sleeps about 4.45 ms before reading input and blocks about 1.7 ms in the swap, with the same frame p50/p99. The HUD shows swap ms; RHYTHM_PERF=1 prints it plus await and delay averages.
- Compositor: KWin honours _NET_WM_BYPASS_COMPOSITOR (qdbus Compositing.active goes true to false). vendor/miniquad gained window::set_bypass_compositor, driven by Settings > Bypass compositor in song (default on): set while a song is loaded, cleared back in the song list. Fullscreen alone does NOT stop KWin compositing, so F11 is only a window mode. While it is set, other windows lose compositing (brief flash when entering and leaving a song).
- Short GL queue (__GL_MaxFramesAllowed=1, needs restart) is off by default: it produced more missed refreshes with the pacer on.
- Calibration: Settings > C. A click plays every 0.5 s and a box flashes; offset_ms delays the flash after the click. Press Right when the flash comes first, Left when the click comes first.
- No fail is a setting; also what makes long headless perf runs possible (the default health model fails an idle cursor after about 3 s).
- The window itself no longer asks for 4x MSAA (the scene already renders into its own 4x target).
- Known: with the pacer off, 1.6 percent of frames show a 3 to 5 ms stall inside a GL call (driver back-pressure); with it on that stall disappears but 1 to 3 vblank misses per 24 s still occur when the machine is busy, each lowering the pacer cap by 1 ms.

## Latency, round three

Council verdict applied: variance fixes first, no new latency features.

- Swept hit test: src/game.rs judge_note() clips the cursor path between two frames against the 0.57 square (slab clip). A note counts as hit at the entry time, so the verdict no longer depends on where the frame boundary falls; entry before the note time still hits at the note time, entry after the 55 ms window is a miss. Unit tests cover frame phases 0 to 7 ms, window expiry and entry time. RHYTHM_PERF=1 prints "sweep-only hits" (hits the old point test would have missed).
- Raw motion: vendored xi_input.rs now reads raw_values by the valuator mask (the array is packed). Measured with xinput test-xi2 --root: one RawMotion per physical move with XIAllDevices (master copy, source = the mouse), so no doubled input and no sensitivity change.
- Pacer health: the RHYTHM_PERF song-end line also prints cap, low_latency, short_gl_queue and bypass. Healthy: pacing reached about 4 ms, swap block under 2 ms. A swap near 5.5 ms means the pacer was off or capped.
- Not done on purpose: audio buffer below 256 frames, custom audio callback, swap interval 0, glFinish, X timestamps for backdating, an input thread. Output-latency offset in the song clock: skipped, it is a constant the calibration offset already absorbs and would shift saved calibrations.

## Best scores

src/scores.rs keeps the best score per song (keyed by file name) in scores.json next to settings.json. A run replaces the best only with a higher score and only when not failed. The song list shows best accuracy and grade on the right; the ready and results screens show the best, or NEW BEST. Unit tests cover replacement and the file round trip.

## Song list details

src/main.rs request_meta() parses every map on a background thread into the METAS cache (kept across menu visits). Rows show the map title (file name until parsed), then notes, length (last note time) and mappers on the right, then the best accuracy and grade. Checked by loading a real .sspm: title, mapper and note count come out right. Covers and artist for .sspm are not read yet.

## Roadmap council (round 3)

Agreed order: instant retry, safe saves, hit-error stats, replays, difficulty stars, search, hover preview, speed practice last. Do not build: leaderboards, skin editor, tutorial, macOS target.

- Instant retry: R and click-to-retry start the music at first note minus approach minus 0.5 s (game.rs start_time), clamped to the normal 2 s lead-in. The first start of a map keeps the full lead-in.
- Safe saves: src/store.rs write_atomic writes a temp file then renames it over settings.json and scores.json. A scores.json that does not parse is moved to scores.json.bad instead of being overwritten.
- Dropped: "set offset to the mean hit error". A hit only registers at or after the note time, so there is no early side to average; the number would only measure how late the cursor arrived.
- Not done: pinning the data folder to an absolute path (would orphan the current settings, scores and maps), speed practice (needs an honest 55 ms window in real time first).

## Performance harness and round one

tools/bench.py plays a real map with a scripted cursor (RHYTHM_BENCH=play:12 or menu:6, RHYTHM_VSYNC=0 for uncapped) in a throwaway working directory, with the window moved to the second monitor, and writes perf/<label>.json (median of 3). tools/report.py builds perf/report.html with the regressions listed in red. Pin the binary with --bin (a rebuild mid-run mixes two builds); build experiments with CARGO_TARGET_DIR outside target/.

Baseline on an RTX 4060, 144 Hz, with the player's normal background apps: vsync play frame avg 6.94 ms, p99 7.4, 0 missed refreshes; CPU about 0.6 ms per frame (11 percent of one core); uncapped 720p 0.57 ms per frame, 1080p 0.95 ms (about 1000 fps); map load under 50 ms; start to first frame about 0.6 s (window creation is most of it).

The weak spot was CPU contention: with 12 busy processes the vsync run had 12 late frames, max 13.4 ms, and the pacer fell to zero delay (input sampled a full frame early). Fix: src/priority.rs asks rtkit for nice -15 for the main thread (from a background thread), and kira's cpal-realtime-dbus feature gives the audio callback thread real-time scheduling. After: 0 late frames, max 9.8 ms, pacer stays at 4.4 ms.

Open: the vsync scenarios for gpu stress and menu could not be rerun after the display lost vsync system-wide mid-session (glxgears reported 27000 fps); rerun them once vsync is back.

## Songs without maps, more formats, removing songs

- Audio files (mp3 ogg oga opus wav flac m4a aac mp4 m4b aiff aif caf mka webm wma wv) dropped on the window become a playable map. src/chart.rs: spectral-flux onset detection (own radix-2 FFT, 10 ms hop, adaptive threshold, 0.12 s minimum gap), then a deterministic 3x3 layout whose step size depends on the gap to the previous note (1 cell under 0.22 s, 2 under 0.5 s, 3 beyond), never the same cell twice. Checked against the hand-made migraine.sspm: median offset 4.6 ms, about 72 percent of generated notes within 60 ms of a hand note at 5.5 notes per second (hand map 6.5). Mapper shows as auto-generated.
- Decoding goes through kira/symphonia (mp3 ogg vorbis flac wav aac alac aiff caf mp4 mkv adpcm); anything else (opus, wma, wavpack, webm) falls back to the ffmpeg binary if installed (src/audio.rs via_ffmpeg). All 15 formats above loaded in a probe.
- A song file next to a map with the same name (a .txt and its .mp3) stays a companion, not a second entry.
- osu: .osz and .osu import as maps (circles and slider heads, spinners dropped, osu!standard only, playfield mapped to the 3x3 grid, biggest difficulty in an .osz).
- Delete removes the selected song (files move to maps/.trash, a .txt takes its audio with it); Z undoes, newest first; dragging the file back in re-adds it, and the best score is kept because it is keyed by file name. Tests in src/import.rs cover remove, re-add by import, undo and same-name removals.

## Performance round two

- Pacer collapse (src/perf.rs, Pacer::update): the cap only ever dropped, so one burst of load left the pacer at 0 ms for the rest of the session (seen in 4 of 9 pinned runs under cpu/gpu stress, and once in plain play). Now the cap rises 1 ms after 4 s without a miss, never above frame interval minus 2.5 ms; a miss within 2 s of a raise doubles the wait (max 8x, 32 s). Unit tests in perf.rs cover climb, backoff, recovery, the ceiling and the doubled wait.
- Bench JSON gained pacer_delay_ms (summary of the per-frame pacer delay) and pacer_zero_pct (share of frames at 0 ms); report.py lists them plus swap block and window_ms. New scenarios play_vsync_cpu_stress_long and play_vsync_gpu_stress_long (30 s) show the recovery.
- Results (3 repeats, same method, pinned binaries, old policy vs new policy built from the same tree): zero-delay frames 100 percent to 0 in both 30 s stress runs, swap block 5.6 ms to 2.2 ms (cpu stress), slow frames 107 to 5, frame p99 12.1 to 9.6 ms. Counts, not milliseconds, are the trustworthy part on a shared machine.
- Startup: the earlier 0.6 s to first frame was mostly a harness artifact. bench.py asked xrandr for the monitor layout while the game was connecting to X, which stalls the X server about 450 ms (the game sat in poll() on its first X reply). With the monitor looked up before launch: window about 0.13 s, first frame about 0.16 s. Older JSON files still show the inflated numbers. Real breakdown with RHYTHM_PERF=1: libGL/GLX init about 25 ms, context create 20 ms, first make_current (NVIDIA context init) about 105 ms, macroquad init 11 ms, Skin::load 33 ms (two fonts 31 ms, parse plus ASCII bake), PostFx::new 0.3 to 14 ms (driver shader cache), Settings load under 0.1 ms.

## Looks, wallpapers and the tabbed settings screen

- src/look.rs holds 8 look presets as plain data (Hot Pink default, Acid, Ice, Amber, Violet, Clarity, Mint, Mono): two note colours, ground (fog) colour, background space, bloom strength, vignette. look::set_active(settings.look) runs every frame in main; palette(), fog(), bloom and vignette all read it, so a look recolours notes, background, fog and post effects together. Hit confetti, rings, tunnel and void dust only drift a little off the palette now (render.rs mixes prism() at 12 to 25 percent).
- Wallpapers: PNG and JPG in wallpapers/ (src/wallpaper.rs). Dropping an image on the song list copies it there and selects it. A selected wallpaper replaces the 3D background in draw_space (wallpaper::backdrop draws it into the scene target, dimmed by settings.wallpaper_dim); notes, grid, HUD and bloom stay on top. Images are decoded once, capped at 64 MB and 8192 px, and downscaled to 2560 px.
- Settings (src/settings.rs): four tabs Look, Play, Sound, Video, each with a More row for technical options. Q/E or click to switch tab; Backspace resets a row, Delete resets the tab, R reverts to what the screen opened with, Esc saves and leaves. A dot marks a row that differs from the default. The accent colour follows the look.
- Checked by screenshot: the Look and Play tabs, the Ice and Violet looks, a wallpaper (orientation verified by pixel colours). Not checked by eye: wallpaper picked inside the settings screen, mouse tab clicks, 1080p.

## Dropping many files in a row

Imports now run on one background thread (Importer in src/main.rs) fed by a queue, so a slow import (a 6 minute mp3 needs decoding and beat finding) no longer freezes the window or drops the next drag. The list shows "importing songs..." while the queue is busy and refreshes as each drop finishes. Checked with real XDND drops 0.4 s apart: a 6 minute mp3, a .sspm, an .ogg and another mp3 all landed. Also accepted: about 45 audio and video extensions, folders (3 levels), and .zip packs (src/import.rs expand/unzip).

## Drag and drop finish message

The vendored miniquad X11 drop handler never told the drag source that a drop was done (XdndFinished, required by XDND 5). File managers such as Dolphin keep waiting on that, so a second drag soon after the first was refused. vendor/miniquad/src/native/linux_x11/drag_n_drop.rs on_finished() now sends it after the dropped data is read. Check: /tmp/xdnd_drop.py style drops report "XdndFinished received" for each of three drops in a row (before the fix: "NO XdndFinished").

## Competitive pack

- Look 9, "Competitive": white and cyan notes on pure black, Void background, no glow, no vignette, no hit confetti, rings, popups or cursor trail (src/look.rs `calm` flag, gated in render.rs draw_effects, the score popups and draw_cursor). postfx skips the bloom passes when the look has bloom 0 or glow is off, so it also costs less to draw.
- Results screen: average lateness, spread and the 95th percentile of hit lateness (src/stats.rs summarize). Note a hit only registers at or after its note time, so "late" here measures how long the cursor took to reach the note, not early versus late.
- Replay: after a run, V replays your cursor path against the same song (Game::watch_replay in src/game.rs). It never touches the saved best score and is reproduced by the same judging code, tested in game.rs. Esc or Space leaves the replay and restores the original results.
- design_system/windows-port.md: Windows feasibility (cargo check passes for both Windows targets; the blockers are runtime, not compile: frame delay ignored in miniquad's Windows backend, a drop-file panic on folders, no /proc telemetry). design_system/rhythia-comparison.md: Steam Rhythia menu numbers next to ours.

## Menu frame cap and review fixes

- Menus (song list and settings, not songs or calibration) sleep MENU_FRAME_DELAY = 11.5 ms before reading input (src/main.rs). Measured with the menu_vsync benchmark, 3 runs each, 144 Hz: before 16.7 percent of one core, 223.6 MB, 6.95 ms per frame; after 8.8 percent, 199.5 MB, 12.7 ms per frame (about 79 fps). Songs are unchanged: play_vsync still 6.94 ms avg, p99 7.9, pacer 4.4 ms, 0 slow frames. A first try at 7 ms only reached 13.3 percent because the GPU queue absorbed the sleep. GPU utilisation is whole-GPU and noisy (25 to 23 percent), so it is not a result. Steam Rhythia's menu on this PC used about 22.5 percent of a core at 15 fps.
- A Haiku 5.5 review of the replay, stats and settings code found four real issues, all fixed: NEW BEST badge lost after a replay, a replay failing mid-run ending twice, a slider drag moving a different row after Up or Down, and a NaN time panicking Replay::sample_at. Left as is: the replay stops recording after 400,000 samples (about 46 minutes), and changing the offset mid-run makes a replay differ slightly from the live judging.
- Checked by screenshot at 1280x720 and 1920x1022: the settings Look tab with a wallpaper picked inside the screen (dimmed, readable) and the Sound tab. Mouse clicks on tab names are still only covered by the layout tests, not by hand.
