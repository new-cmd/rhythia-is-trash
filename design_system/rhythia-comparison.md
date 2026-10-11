# Menu resource use: Steam Rhythia vs this project

Measured 2026-10-11 on this PC (single 20 s window, idle menu, no input). Steam Rhythia =
Steam app 2250500 (`~/.local/share/Steam/steamapps/common/rhythia/rhythia`, window moved to
the right monitor with `xdotool windowmove <id> 1920 58` only). Ours = `perf/after.json`,
scenarios `play_vsync` and `menu_uncapped`. IMPORTANT: the "ours (vsync)" column is a GAMEPLAY
run (the game plays a real map at 144 Hz vsync), not a menu run, because no vsync menu scenario
exists in that file. It is not a like-for-like match with Steam Rhythia's idle menu, so do not
read it as a menu result; only the uncapped column is a true menu measurement. GPU utilisation is whole-GPU
(`nvidia-smi`), so it includes the desktop and other apps; per-process rows from
`nvidia-smi pmon` showed rhythia at sm 0-2%.

| metric                       | Steam Rhythia menu | ours (vsync) | ours (uncapped menu) |
|------------------------------|--------------------|--------------|----------------------|
| CPU (% of one core)          | 22.5               | 12.3         | 99.5                 |
| VmRSS (MB)                   | 536.5              | 218.8        | 278.4                |
| VmHWM (peak RSS, MB)         | 539.1              | 223.4        | 321.1                |
| Threads                      | 17                 | n/a          | n/a                  |
| GPU utilisation (%, whole GPU incl. desktop) | 12 (avg, 9-14, clock 2460 MHz) | 34.2 | 91.8 |
| Frame time avg (ms)          | ~66.7 (paced, see below) | 6.94 (vsync 144 Hz) | 0.40  |

Steam Rhythia frame pacing: during the 20 s idle window its own performance log wrote 48
`Frame spike ... inGameScene=False` lines, i.e. it treats every menu frame as a spike.
total= was 66.66 ms in 22 of 48 cases (median 66.72 ms, max 72.10 ms), and almost all of
that is `end=` (~66.5 ms) while start/update/draw sum to ~0.15 ms. The menu is paced at
about 15 fps (66.66 ms exactly = 1000/15), not 60 fps.

## What this does and does not prove

- On this machine our uncapped menu renders ~2500 fps at 0.40 ms/frame but pins one core
  (~99.5%) and the GPU (~92%), our vsync build sits at 6.94 ms/frame (144 Hz) with ~12% of
  one core, and Steam Rhythia's menu idles at 15 fps with ~22% of one core and ~12% GPU;
  our builds use roughly half the RSS of Steam Rhythia (219-278 vs 536 MB).
- Only the Steam Rhythia and uncapped-menu columns are menu numbers; the vsync column is gameplay, so it says nothing about menu cost, and the GPU utilisation
  figures include every other app on the machine, so they are not attributable to one process.
- Everything above is a single 20 s sample on one PC with one driver, so it shows what
  happened here, not what either game does in general.
