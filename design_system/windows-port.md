# Windows Port Investigation

## 1. Rust Targets Installed
Ran `rustup target list --installed`:
- `x86_64-pc-windows-gnu` (already installed)
- `x86_64-pc-windows-msvc` (already installed)
- `x86_64-unknown-linux-gnu` (host)

---

## 2. Cargo Check Results

### x86_64-pc-windows-gnu
Command executed:
```bash
nice -n 10 env CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=/tmp/rhythm-win-target cargo check --release --target x86_64-pc-windows-gnu
```

**Result**: Passed with zero compilation errors. Generated only 2 warnings in `vendor/miniquad`:
- `vendor/miniquad/src/native/windows.rs:253:13`: unused assignment to `new_rect`
- `vendor/miniquad/src/lib.rs:103:4`: dead code warning for `set_or_replace_display`

Finished in ~0.92s.

### x86_64-pc-windows-msvc
Command executed:
```bash
nice -n 10 env CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=/tmp/rhythm-win-target cargo check --release --target x86_64-pc-windows-msvc
```

**Result**: Passed with zero compilation errors (same 2 `miniquad` warnings). Finished in ~27.93s.

---

## 3. Analysis of Compile Blocks, Existing Gates, and Proposed Diffs

Neither target produced hard compile errors during `cargo check`, but deep inspection of code and dependencies reveals platform-specific assumptions and runtime bugs on Windows:

### 3.1 Linux-Only Dependencies & Features in Cargo.toml
- `dbus = "0.6.4"` and `libc = "0.2"` are already under `[target.'cfg(target_os = "linux")'.dependencies]` (lines 13–15). They are skipped cleanly on Windows.
- `kira` in `Cargo.toml:7`:
  ```toml
  kira = { version = "0.12.5", default-features = false, features = ["cpal", "cpal-realtime", "cpal-realtime-dbus", "mp3", "ogg", "vorbis", "flac", "wav", "pcm", "aac", "alac", "aiff", "caf", "isomp4", "mkv", "adpcm"] }
  ```
  `cpal-realtime-dbus` enables RealtimeKit integration via D-Bus inside `cpal`. On non-Linux targets, `cpal` / `audio_thread_priority` gracefully cfg-gates or ignores D-Bus handling without breaking compilation. However, to keep dependencies pure and avoid unnecessary feature evaluation, `cpal-realtime-dbus` should ideally be gated or separated.

### 3.2 Thread Priority (`src/priority.rs`)
`src/priority.rs` already has a fallback stub:
```rust
#[cfg(not(target_os = "linux"))]
pub fn raise_main_thread() {}
```
On Windows, this is a no-op. For parity with Linux (nice -15), Windows can call Win32 `SetPriorityClass` or `SetThreadPriority`.

**Proposed Diff (`src/priority.rs`)**:
```diff
--- a/src/priority.rs
+++ b/src/priority.rs
@@ -28,5 +28,15 @@ fn request_nice(tid: u64, nice: i32) -> Result<(), String> {
         .map_err(|e| e.to_string())
 }

-#[cfg(not(target_os = "linux"))]
+#[cfg(target_os = "windows")]
+pub fn raise_main_thread() {
+    unsafe {
+        use windows_sys::Win32::System::Threading::{
+            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
+        };
+        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
+    }
+}
+
+#[cfg(not(any(target_os = "linux", target_os = "windows")))]
 pub fn raise_main_thread() {}
```

### 3.3 Benchmarking & `/proc` Reads (`src/main.rs`)
`src/main.rs` lines 93–153 define:
- `proc_stat_fields()`: reads `/proc/self/stat`
- `realtime_threads()`: reads `/proc/self/task`
- `proc_cpu_seconds()`: parses stat ticks
- `process_age_ms()`: reads `/proc/uptime`
- `proc_status_mb()`: reads `/proc/self/status`

On Windows, all `/proc` reads fail gracefully returning defaults (`""`, `0.0`, or `0`), but the benchmark mode (`--bench`) produces meaningless/zero telemetry, and `realtime_threads()` always reports 0.

**Proposed Diff (`src/main.rs`)**:
```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -93,12 +93,16 @@ fn parse_bench_env() -> Option<(bool, f64)> {
 fn proc_stat_fields() -> Vec<String> {
+    #[cfg(target_os = "linux")]
     let text = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
+    #[cfg(not(target_os = "linux"))]
+    let text = String::new();
     text.rsplit_once(')')
         .map(|(_, rest)| rest.split_whitespace().map(String::from).collect())
         .unwrap_or_default()
 }

 fn realtime_threads() -> usize {
+    #[cfg(target_os = "linux")]
     std::fs::read_dir("/proc/self/task")
         .into_iter()
         .flatten()
@@ -109,6 +113,8 @@ fn realtime_threads() -> usize {
             matches!(policy, Some(1 | 2))
         })
         .count()
+    #[cfg(not(target_os = "linux"))]
+    0
 }
```

### 3.4 Miniquad Display Controls (`vendor/miniquad`)
- `window::set_bypass_compositor(on)`:
  Implemented in `vendor/miniquad/src/lib.rs:336`.
  Linux/X11 sends `_NET_WM_BYPASS_COMPOSITOR`.
  On non-Linux systems, it executes `let _ = on;` (silent no-op). Windows DWM does not support X11 compositor bypassing; true fullscreen exclusive or borderless windowed mode is required instead.
- `window::set_frame_delay(seconds)`:
  Implemented in `vendor/miniquad/src/lib.rs:121` and stores into atomic `FRAME_DELAY_US`.
  **CRITICAL ISSUE**: In `vendor/miniquad/src/native/linux_x11.rs:518, 634`, `FRAME_DELAY_US` is actively read and passed to `std::thread::sleep(Duration::from_micros(delay as u64))` before swapping buffers.
  In `vendor/miniquad/src/native/windows.rs`, `FRAME_DELAY_US` is **never read**! Frame delay limiting is completely ignored on Windows.

**Proposed Diff (`vendor/miniquad/src/native/windows.rs`)**:
```diff
--- a/vendor/miniquad/src/native/windows.rs
+++ b/vendor/miniquad/src/native/windows.rs
@@ -1285,6 +1285,10 @@ pub unsafe fn run<T>(conf: conf::Conf, f: T)
             (d.event_handler).as_mut().unwrap().draw();
         }
+        let delay = crate::FRAME_DELAY_US.load(std::sync::atomic::Ordering::Relaxed);
+        if delay > 0 {
+            std::thread::sleep(std::time::Duration::from_micros(delay as u64));
+        }
         SwapBuffers(display.dc);
     }
```

---

## 4. Runtime Checklist for Windows

- [ ] **Audio Device Latency on WASAPI (cpal / kira)**:
  - `src/audio.rs:100`: `config.buffer_size = BufferSize::Fixed(frames)`. On Windows WASAPI shared mode, `cpal` requests buffer sizes in 100ns units, and many drivers/audio endpoints reject arbitrary buffer sizes or clamp to the shared mix period (often 10ms / ~441–480 samples).
  - Kira's fallback logic catches failure and logs `audio buffer {frames}: {e}; using the default`, falling back to `AudioManagerSettings::default()`.
  - Check whether WASAPI exclusive mode or high audio latency affects rhythm gameplay timing.
- [ ] **Kira Realtime Audio Thread**:
  - `cpal-realtime` uses `audio_thread_priority`. On Windows, this relies on `AvSetMmThreadCharacteristicsW` (Pro Audio task). Does not use D-Bus.
- [ ] **Compositor Bypass & DWM vs X11**:
  - Windows DWM always enforces desktop composition in windowed modes. Fullscreen exclusive via OpenGL / WGL requires explicit mode change or borderless matching display resolution. `miniquad::window::set_bypass_compositor` is a no-op on Windows.
- [ ] **Drag and Drop (`WM_DROPFILES`)**:
  - `vendor/miniquad/src/native/windows.rs:741` handles `WM_DROPFILES`, calling `DragQueryFileW`.
  - Notice line 758: `d.dropped_files.bytes.push(std::fs::read(&path).unwrap());`. If a dropped item is a directory or an unreadable path, `unwrap()` will crash the process immediately!
  - Must ensure miniquad does not panic on folder drops.
- [ ] **High-DPI Scaling**:
  - `vendor/miniquad/src/native/windows.rs:1093` handles DPI via `GetDpiForMonitor` / `SetProcessDPIAware`.
  - Notice line 300 comment: `// this is probably not correct: with high dpi content_width and window_width are actually different..`.
  - On non-100% display scaling (125%, 150%), check if macroquad mouse coordinates match the rendered viewport.
- [ ] **Settings, Scores, and Maps Paths**:
  - Currently saves to current working directory (`settings.json`, `scores.json`, `maps/`).
  - If installed in `C:\Program Files\`, writing to cwd will fail with Access Denied (permission errors). Should migrate to `dirs::config_dir()` / `%APPDATA%` on Windows.
- [ ] **Windows File Name Rules**:
  - Map unpacking and file imports (`src/import.rs`, `src/map.rs`) must sanitize reserved characters: `< > : " / \ | ? *` and reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`).
  - Windows paths use `\` or `/`, but trailing dots and spaces in directory names cause OS file errors.

---

## 5. Ranked List: What Blocks a First Windows Build

1. **Linker Availability for Cross-Compilation (`x86_64-pc-windows-gnu`)**
   - **Status**: Blocks binary linking when compiling from Linux.
   - **Effort**: Low (install package + cargo config).
   - **Details**: `cargo check` passes, but `cargo build --target x86_64-pc-windows-gnu` requires `x86_64-w64-mingw32-gcc` (or `lld`).
   - **Linker Package Needed**: On Arch Linux: `mingw-w64-gcc` (`pacman -S mingw-w64-gcc`).
   - `.cargo/config.toml`:
     ```toml
     [target.x86_64-pc-windows-gnu]
     linker = "x86_64-w64-mingw32-gcc"
     ar = "x86_64-w64-mingw32-gcc-ar"
     ```
   - Alternatively, install `lld` and configure `rust-lld` as linker.

2. **Miniquad Windows Frame Delay Bug**
   - **Status**: Runtime blocker for frame pacing / delay features.
   - **Effort**: 10 minutes (add atomic load and sleep in `vendor/miniquad/src/native/windows.rs`).

3. **Miniquad Windows `WM_DROPFILES` Crash Risk**
   - **Status**: Runtime stability issue (drag-dropping a folder panics on `std::fs::read(&path).unwrap()`).
   - **Effort**: 15 minutes (safely handle directory paths or ignore read errors).

4. **Benchmarking Telemetry (`/proc` on Windows)**
   - **Status**: Feature limitation (non-blocking for gameplay).
   - **Effort**: 30 minutes (cfg-gate or add Windows performance counter / memory status fallback).

5. **Audio Buffer Size Compatibility (WASAPI)**
   - **Status**: Runtime audio latency tuning.
   - **Effort**: 1 hour (test supported buffer sizes; kira fallback already prevents hard crash).

---

## Cross-Build Feasibility Verdict

The project **can be cross-built from Linux** immediately using `cargo build --target x86_64-pc-windows-gnu`. The Rust code has zero compilation errors for Windows. Only a MinGW-w64 C cross-compiler/linker (`mingw-w64-gcc` on Arch Linux) is needed to produce the `.exe`.

## Review notes (added by the owner)

- The proposed `realtime_threads` diff above puts `#[cfg]` on a block tail expression, which rustc rejects. Write two separate `fn realtime_threads` items (one per cfg) instead.
- The proposed `priority.rs` Windows branch uses `windows_sys`; that crate is not in Cargo.toml and would have to be added as a Windows-only dependency.
- `cargo check` for both Windows targets was reported clean by the agent but I did not re-run it myself.
