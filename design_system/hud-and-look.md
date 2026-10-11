# HUD and look

What is on screen during a song, where it is drawn, and which constants control it. All gameplay drawing lives in src/render.rs (draw, render.rs:596), HUD state is animated in src/game.rs (animate_hud, game.rs:559), the note mesh and masks are baked in src/skin.rs, and bloom plus vignette live in src/postfx.rs. Text renders after post-processing so it stays crisp (render.rs:613-620).

## The two side panels

Two vertical panels flank the playfield, tilted 10 degrees toward it (panel(), render.rs:173; PANEL_W 4.0, PANEL_H 10.0, render.rs:34). Layout is authored in a 250x625 pixel space (PANEL_PX, render.rs:36) and converted by panel_px() (render.rs:182). The black panel bodies and the top/bottom accent trim strips are drawn in draw_hud_3d (render.rs:623-662); the accent colour is palette[0] (render.rs:635). Everything else is split between draw_hud_3d (world geometry) and draw_hud_text (render.rs:1026, world-anchored text). Font sizes come from em = 50 px and small = 30 px of the panel space (render.rs:1030-1031).

### Left panel: score and multiplier

- "SCORE" label at y 0.28, the animated score at y 0.35 (render.rs:1040-1047). The displayed value chases the real one at dt*14 per frame and snaps when under 0.5 away (game.rs:573-577).
- The multiplier ring, drawn as world geometry (render.rs:663-745). It is a polygon with multiplier_increment sides (clamped 3..32, render.rs:665); multiplier_increment is notes/200, minimum 2 (game.rs:212). The ring is centred at panel y 0.65 with radius 64 px and stroke 6 px (render.rs:667-668). Fill shows hud.multiplier_progress as an arc in degrees (render.rs:669); filled segments get two additive glow passes (render.rs:678-702) then a soft filled fan plus crisp strokes over a RING_GRAY base (render.rs:724-744). The ring pulses 12 percent on level-up via mult_pulse (render.rs:667, game.rs:578-582) and turns orange at the 8x cap (MAX_MULTIPLIER, game.rs:22; colour set in game.rs:564-568).
- The "8x" style number sits inside the ring at y 0.65 and grows 30 percent with the pulse (render.rs:1049-1055); "MULTIPLIER" label at y 0.79 (render.rs:1056-1062).

### Right panel: accuracy and notes

- "ACCURACY" label at y 0.28, the percentage at y 0.35 tinted 40 percent toward the grade colour, the grade letter at y 0.43 in full grade colour (render.rs:1078-1093). Grade thresholds: SS 100, S 95, A 90, B 80, C 70, else D, and F when failed (game.rs:49-57, 255-259). Colours: GRADE_S gold, GRADE_A mint, GRADE_B blue, C uses the first note colour, D and F use MISS red (render.rs:42-47, 1063-1069).
- Hit and miss icons at panel y 0.6175: a green square for hits, a red X for misses, drawn as geometry with additive flashes when their opacity spikes (render.rs:703-721, 746-757). Their counters sit beside them (render.rs:1097-1110) and the total judged count at y 0.7 (render.rs:1111-1117); "NOTES" label at y 0.79. Opacity idles at 0.62 (IDLE_OPACITY, game.rs:26), jumps to 1.0 on each hit or miss and decays at 1.5 per second (game.rs:570-571).
- A thin white line at panel y 0.65 separates the halves, drawn as two gradient quads fading at the middle and ends (render.rs:758-781).

### Bars and floating text

- Health bar: a 15-degree tilted bar above the playfield (tilted_bar, render.rs:189; render.rs:783-796). Track is accent colour at 18 percent alpha; the fill is HEALTH pink lerping to HEALTH_LOW red as health drops under 25 percent, pulsing with a 2 Hz sine in the danger zone, halved in alpha after a fail. HUD health chases real health at dt*60 (game.rs:572). BAR_HALF_W 5.12, BAR_HALF_H 0.08 (render.rs:37-38).
- Progress bar: the same shape mirrored below the playfield, PROGRESS_BG dark grey track, white fill by song position (render.rs:797-805).
- Combo number floats above the field at y 3.5 when combo > 0 (render.rs:1118-1126); song time and title at y 7.0 and 8.25 (render.rs:1127-1141); per-hit score popups rise from y -1.4 for POPUP_LIFE 0.25 s (render.rs:1142-1156); the lead-in countdown shows while song time is negative (render.rs:1157-1167). A dim perf line sits bottom-left (render.rs:1168-1184).

## Note rendering

Each visible note (draw_notes, render.rs:828) is three layers:

1. An additive glow sprite, note_size * NOTE_GLOW_SCALE (1.7, skin.rs:46) at NOTE_GLOW 0.3 alpha (render.rs:18, 847-858).
2. The squircle frame: a hollow extruded ring baked in skin.rs (squircle_frame, skin.rs:110; exponent SQUIRCLE_EXP 4.5, inner radius SQUIRCLE_INNER 0.714, depth SQUIRCLE_DEPTH 0.125, 48 segments), drawn per-face with its baked brightness (render.rs:859-873).
3. The white core: a rounded-square fill sprite at 0.8x note size, coloured 65 percent toward white, floated just in front of the frame (render.rs:874-884).

Notes travel down -z from approach_distance and fade in over FADE_IN 0.1 of the approach with opacity exponent 1.25 (render.rs:16-17, 841-845). Colour comes from note_hue (0.93 + 0.037 per note index, game.rs:605) through the prism palette (game.rs:590) and the per-map palette. Hit testing is unaffected by any of this (game.rs judge_note).

## Hit effects

draw_effects (render.rs:887), all timed off song time so pause freezes them:

- Hit flash: a filled cell at the note, white-shifted, HIT_FLASH_LIFE 0.09 s (render.rs:49, 903-918).
- Halo: an expanding disc behind the note, BREAK_LIFE 0.5 s, cubic falloff (render.rs:889-902).
- Two waves: expanding rings, the second delayed 0.05 s and hue-shifted +0.18 (render.rs:919-944).
- Fragments: 8 shards per hit with hashed directions, three lagged ghost copies each, prism-coloured, falling with gravity (render.rs:945-978).
- Miss: two crossed red bars at the bottom edge, POPUP_LIFE 0.25 s (render.rs:979-990). MISS_ICON pure red; MISS counter red is softer (render.rs:47-48).

## Bloom and vignette

src/postfx.rs. The scene renders into a 4x MSAA target (postfx.rs:73-80). Bloom: a threshold pass at quarter resolution (EXTRACT shader, peak over 0.48, soft knee 0.4, postfx.rs:14-23), two separable 9-tap Gaussian passes at quarter res, then two more at eighth res for a broad glow (postfx.rs:163-179). Composition (COMBINE, postfx.rs:39-55): base + (bloom * 0.5 + wide * 0.3) * Strength 0.8, glow saturation pushed to 1.3; vignette darkens 42 percent from smoothstep(0.45, 1.15) of the distance from centre. F2 toggles enabled, which zeroes Strength but keeps the vignette (postfx.rs:184-185). Targets rebuild when the window resizes (postfx.rs:140-142).

## How to change X

| Goal | Where |
| --- | --- |
| Panel width, height, tilt | PANEL_W, PANEL_H (src/render.rs:34-35); angle in panel() (src/render.rs:174) |
| Panel layout grid | PANEL_PX (src/render.rs:36); y positions are the 0.28/0.35/0.65/0.79 fractions in draw_hud_text (src/render.rs:1040-1117) |
| Panel text sizes | em and small (src/render.rs:1030-1031) |
| Multiplier ring radius, stroke, pulse | 64.0/6.0 in render.rs:667-668; pulse scale 0.12 same line; ring sides via multiplier_increment (src/game.rs:212) |
| Ring max multiplier and its orange | MAX_MULTIPLIER (src/game.rs:22); colour in animate_hud (src/game.rs:564-568) |
| Ring fill speed | multiplier_progress lerp rate 16.0 (src/game.rs:563) |
| Health and progress bar size, tilt | BAR_HALF_W, BAR_HALF_H (src/render.rs:37-38); tilted_bar() (src/render.rs:189); y offsets -6.3/6.3 (src/render.rs:783, 797) |
| Health colours and danger pulse | HEALTH, HEALTH_LOW (src/render.rs:40-41); beat and danger band (src/render.rs:788-795) |
| Grade thresholds | grade_for (src/game.rs:49) |
| Grade colours | GRADE_S, GRADE_A, GRADE_B (src/render.rs:42-44); C/D/F choice (src/render.rs:1063-1069) |
| Hit/miss icon colours | HITS_ICON, MISS, MISS_ICON (src/render.rs:46-48); idle opacity IDLE_OPACITY (src/game.rs:26) |
| Score count-up speed | 14.0 in animate_hud (src/game.rs:574) |
| Note shape | SQUIRCLE_EXP, SQUIRCLE_INNER, SQUIRCLE_DEPTH, SQUIRCLE_SEGMENTS (src/skin.rs:47-50) |
| Note size, glow size, glow strength | note_size setting; NOTE_GLOW_SCALE (src/skin.rs:46); NOTE_GLOW (src/render.rs:18) |
| Note white core | 0.8 size and 0.65 white lerp (src/render.rs:876-883); sprite shape Skin::fill (src/skin.rs) |
| Note fade-in | FADE_IN, OPACITY_EXPONENT (src/render.rs:16-17) |
| Note colour sequence | note_hue step and prism saturation (src/game.rs:590-607) |
| Hit flash length | HIT_FLASH_LIFE (src/render.rs:49) |
| Effect lifetimes | BREAK_LIFE, POPUP_LIFE (src/game.rs:19-20) |
| Bloom threshold and knee | EXTRACT shader 0.48/0.4 (src/postfx.rs:21) |
| Bloom strength and wide/near mix | Strength 0.8 (src/postfx.rs:185); 0.5/0.3 weights (src/postfx.rs:48) |
| Vignette depth and reach | 0.42 and smoothstep(0.45, 1.15) (src/postfx.rs:52-53) |
| Playfield corner marks | GRID_EDGE, GRID_THICK, GRID_ARM (src/render.rs:31-33); draw_grid (src/render.rs:808) |
| Camera distance | CAM_Z 3.75 (src/render.rs:15); parallax in camera() (src/render.rs) |
| Fog colour and range | FOG, FOG_BEGIN, FOG_END, FOG_CURVE (src/render.rs:19-22) |
