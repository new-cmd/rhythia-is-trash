use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use serde_json::Value;
use zip::ZipArchive;

pub const MAP_EXTS: [&str; 6] = ["sspm", "phxm", "rhm", "txt", "osz", "osu"];
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "ogg", "oga", "opus", "wav", "flac", "m4a", "aac", "mp4", "m4b", "aiff", "aif", "caf",
    "mka", "webm", "wma", "wv", "avi", "mov", "mkv", "flv", "wmv", "3gp", "mts", "m2ts", "mpg",
    "mpeg", "m4v", "ogv", "weba", "ac3", "eac3", "dts", "amr", "au", "mp2", "mp1", "ape", "tta",
    "spx", "aifc", "wave", "mpc", "tak", "oma", "ra",
];
pub const AUTO_MAPPER: &str = "auto-generated";

/// Picked after a song. Easier levels drop notes that follow the last kept one too closely, so streams thin out into steady beats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub const ALL: [Self; 3] = [Self::Easy, Self::Medium, Self::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Self::Easy => "Easy",
            Self::Medium => "Medium",
            Self::Hard => "Hard",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Self::Easy => "main beats only",
            Self::Medium => "most of the notes",
            Self::Hard => "every note",
        }
    }

    fn min_gap(self) -> f64 {
        match self {
            Self::Easy => 0.4,
            Self::Medium => 0.25,
            Self::Hard => 0.0,
        }
    }

    /// Scores on Hard keep the plain song key, so bests from before difficulties existed still count.
    pub fn key(self, label: &str) -> String {
        match self {
            Self::Hard => label.to_string(),
            _ => format!("{label} ({})", self.name()),
        }
    }

    pub fn apply(self, notes: &mut Vec<Note>) {
        let gap = self.min_gap();
        if gap <= 0.0 {
            return;
        }
        let mut last = f64::NEG_INFINITY;
        notes.retain(|n| {
            let keep = n.time - last >= gap - 1e-9;
            if keep {
                last = n.time;
            }
            keep
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Note {
    pub time: f64,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone)]
pub struct Meta {
    pub title: String,
    pub mappers: String,
    pub notes: usize,
    pub length: f64,
}

pub struct Map {
    pub title: String,
    pub mappers: String,
    pub notes: Vec<Note>,
    pub audio: Option<Vec<u8>>,
}

impl Map {
    pub fn new(
        title: impl Into<String>,
        mappers: impl Into<String>,
        mut notes: Vec<Note>,
        audio: Option<Vec<u8>>,
    ) -> Self {
        notes.retain(|n| n.time.is_finite() && n.time >= 0.0 && n.x.is_finite() && n.y.is_finite());
        for n in &mut notes {
            n.x = n.x.clamp(-2.0, 2.0);
            n.y = n.y.clamp(-2.0, 2.0);
        }
        notes.sort_by(|a, b| a.time.total_cmp(&b.time));
        Self {
            title: title.into(),
            mappers: mappers.into(),
            notes,
            audio,
        }
    }

    pub fn end_time(&self) -> f64 {
        self.notes.last().map_or(0.0, |n| n.time)
    }

    pub fn meta(&self) -> Meta {
        Meta {
            title: self.title.clone(),
            mappers: self.mappers.clone(),
            notes: self.notes.len(),
            length: self.end_time(),
        }
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let map = if path.is_dir() {
            phxm_dir(path)
        } else {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "txt" => txt(path),
                "sspm" => sspm(&read(path)?),
                "phxm" => phxm_zip(&read(path)?),
                "rhm" => rhm(&read(path)?),
                "osz" => osz(&read(path)?),
                "osu" => osu_file(path),
                e if AUDIO_EXTS.contains(&e) => song(path),
                _ => Err(format!("unsupported map format .{ext}")),
            }
        }
        .map_err(|e| format!("{}: {e}", path.display()))?;
        if map.notes.is_empty() {
            let why = if map.mappers == AUTO_MAPPER {
                "no beats found in this song"
            } else {
                "map has no notes"
            };
            return Err(format!("{}: {why}", path.display()));
        }
        Ok(map)
    }
}

pub fn is_map(path: &Path) -> bool {
    if path.is_dir() {
        return path.join("metadata.json").is_file() && path.join("objects.phxmo").is_file();
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MAP_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn has_sibling_map(path: &Path, others: &[PathBuf]) -> bool {
    let key = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().to_lowercase());
    let dir = path.parent().unwrap_or(Path::new("."));
    let stem = key(path);
    let beside = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .chain(others.iter().cloned());
    beside
        .into_iter()
        .any(|p| p != path && is_map(&p) && key(&p) == stem)
}

pub fn is_audio(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| AUDIO_EXTS.contains(&e.to_ascii_lowercase().as_str()))
}

pub fn scan(dir: &Path) -> Vec<PathBuf> {
    let all: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    let mut maps: Vec<PathBuf> = all
        .iter()
        .filter(|p| is_map(p) || (is_audio(p) && !has_sibling_map(p, &[])))
        .cloned()
        .collect();
    maps.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    maps
}

const MAX_FILE: u64 = 1 << 30;
const MAX_TEXT: u64 = 64 << 20;

fn read(path: &Path) -> Result<Vec<u8>, String> {
    check_size(path, MAX_FILE)?;
    std::fs::read(path).map_err(|e| e.to_string())
}

fn read_text(path: &Path) -> Result<String, String> {
    check_size(path, MAX_TEXT)?;
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

fn check_size(path: &Path, limit: u64) -> Result<(), String> {
    let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if len > limit {
        return Err(format!("file is too large ({} MB)", len >> 20));
    }
    Ok(())
}

fn audio_ext(meta: &Value) -> &'static str {
    let wanted = meta["AudioExt"].as_str().unwrap_or("").to_ascii_lowercase();
    AUDIO_EXTS
        .iter()
        .find(|e| **e == wanted)
        .copied()
        .unwrap_or("mp3")
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "untitled".into(), |s| s.to_string_lossy().into_owned())
}

fn ms(v: u32) -> f64 {
    v as f64 / 1000.0
}

fn song(path: &Path) -> Result<Map, String> {
    let bytes = read(path)?;
    let data = crate::audio::decode(bytes.clone())?;
    let notes = crate::chart::generate(&data);
    Ok(Map::new(stem(path), AUTO_MAPPER, notes, Some(bytes)))
}

fn txt(path: &Path) -> Result<Map, String> {
    let src = read_text(path)?;
    let mut fields = src.trim().trim_start_matches('\u{feff}').split(',');
    fields
        .next()
        .filter(|s| !s.trim().is_empty())
        .ok_or("empty map")?;
    let notes = fields
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|f| {
            let mut p = f.split('|').map(str::trim);
            let x: f32 = p.next()?.parse().ok()?;
            let y: f32 = p.next()?.parse().ok()?;
            let t: f64 = p.next()?.parse().ok()?;
            Some(Note {
                time: t / 1000.0,
                x: 1.0 - x,
                y: y - 1.0,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("malformed note entry")?;
    let audio = AUDIO_EXTS
        .iter()
        .map(|e| path.with_extension(e))
        .find(|p| p.is_file())
        .map(|p| read(&p))
        .transpose()?;
    Ok(Map::new(stem(path), "", notes, audio))
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.buf.len())
            .ok_or("unexpected end of file")?;
        let out = &self.buf[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        Ok(self.bytes(N)?.try_into().expect("slice has length N"))
    }

    fn skip(&mut self, n: usize) -> Result<(), String> {
        self.bytes(n).map(drop)
    }

    fn seek(&mut self, pos: u64) -> Result<(), String> {
        let pos = usize::try_from(pos).map_err(|e| e.to_string())?;
        if pos > self.buf.len() {
            return Err("offset out of range".into());
        }
        self.pos = pos;
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.array::<1>()?[0])
    }

    fn bool(&mut self) -> Result<bool, String> {
        Ok(self.u8()? != 0)
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    fn string(&mut self, n: usize) -> Result<String, String> {
        Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }

    fn short_string(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        self.string(n)
    }

    fn line(&mut self) -> Result<String, String> {
        let rest = &self.buf[self.pos..];
        let n = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("unterminated line")?;
        let s = self.string(n)?;
        self.pos += 1;
        Ok(s)
    }

    fn sized(&mut self, len: u64) -> Result<Vec<u8>, String> {
        Ok(self
            .bytes(usize::try_from(len).map_err(|e| e.to_string())?)?
            .to_vec())
    }

    fn position(&mut self, quantum: bool) -> Result<(f32, f32), String> {
        Ok(if quantum {
            (self.f32()?, self.f32()?)
        } else {
            (self.u8()? as f32, self.u8()? as f32)
        })
    }
}

fn sspm(buf: &[u8]) -> Result<Map, String> {
    let mut r = Reader::new(buf);
    if r.bytes(4)? != b"SS+m" {
        return Err("not an SSPM file".into());
    }
    match r.u16()? {
        1 => sspm_v1(&mut r),
        2 => sspm_v2(&mut r),
        v => Err(format!("unsupported SSPM version {v}")),
    }
}

fn sspm_v1(r: &mut Reader) -> Result<Map, String> {
    r.skip(2)?;
    r.line()?;
    let title = r.line()?;
    let mappers = r.line()?;
    r.skip(4)?;
    let count = r.u32()?;
    r.skip(1)?;
    if r.u8()? == 2 {
        let len = r.u64()?;
        r.sized(len)?;
    }
    let audio = if r.bool()? {
        let len = r.u64()?;
        Some(r.sized(len)?)
    } else {
        None
    };
    let notes = (0..count)
        .map(|_| {
            let t = r.u32()?;
            let quantum = r.bool()?;
            let (x, y) = r.position(quantum)?;
            Ok(Note {
                time: ms(t),
                x: x - 1.0,
                y: 1.0 - y,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Map::new(title.trim(), mappers.trim(), notes, audio))
}

fn sspm_v2(r: &mut Reader) -> Result<Map, String> {
    r.skip(4 + 20 + 4)?;
    let count = r.u32()?;
    r.skip(4 + 1 + 2)?;
    let has_audio = r.bool()?;
    r.skip(1 + 1)?;
    r.skip(16)?;
    let (audio_offset, audio_len) = (r.u64()?, r.u64()?);
    r.skip(16 + 16)?;
    let marker_offset = r.u64()?;
    r.skip(8)?;
    r.short_string()?;
    let title = r.short_string()?;
    r.short_string()?;
    let mappers = (0..r.u16()?)
        .map(|_| r.short_string())
        .collect::<Result<Vec<_>, _>>()?;
    let audio = if has_audio {
        r.seek(audio_offset)?;
        Some(r.sized(audio_len)?)
    } else {
        None
    };
    r.seek(marker_offset)?;
    let notes = (0..count)
        .map(|_| {
            let t = r.u32()?;
            r.skip(1)?;
            let quantum = r.bool()?;
            let (x, y) = r.position(quantum)?;
            Ok(Note {
                time: ms(t),
                x: x - 1.0,
                y: 1.0 - y,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Map::new(title.trim(), mappers.join(", "), notes, audio))
}

fn phxmo(buf: &[u8]) -> Result<Vec<Note>, String> {
    let mut r = Reader::new(buf);
    r.skip(4)?;
    (0..r.u32()?)
        .map(|_| {
            let t = r.u32()?;
            let quantum = r.bool()?;
            let (x, y) = r.position(quantum)?;
            let (x, y) = if quantum { (x, y) } else { (x - 1.0, y - 1.0) };
            Ok(Note { time: ms(t), x, y })
        })
        .collect()
}

fn json(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("bad json: {e}"))
}

fn title_of(meta: &Value) -> String {
    let artist = meta["Artist"].as_str().unwrap_or("").trim();
    let title = meta["Title"].as_str().unwrap_or("").trim();
    if artist.is_empty() {
        title.to_string()
    } else {
        format!("{artist} - {title}")
    }
}

fn mappers_of(meta: &Value) -> String {
    meta["Mappers"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn phxm_dir(dir: &Path) -> Result<Map, String> {
    let meta = json(&read(&dir.join("metadata.json"))?)?;
    let notes = phxmo(&read(&dir.join("objects.phxmo"))?)?;
    let audio = if meta["HasAudio"].as_bool().unwrap_or(false) {
        Some(read(&dir.join(format!("audio.{}", audio_ext(&meta))))?)
    } else {
        None
    };
    Ok(Map::new(title_of(&meta), mappers_of(&meta), notes, audio))
}

struct Zip(ZipArchive<Cursor<Vec<u8>>>);

impl Zip {
    fn open(buf: &[u8]) -> Result<Self, String> {
        ZipArchive::new(Cursor::new(buf.to_vec()))
            .map(Self)
            .map_err(|e| e.to_string())
    }

    fn names(&self) -> Vec<String> {
        self.0
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.into_owned())
            .collect()
    }

    fn entry(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let mut f = self.0.by_name(name).map_err(|e| format!("{name}: {e}"))?;
        let mut out = Vec::new();
        f.by_ref()
            .take(MAX_FILE + 1)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        if out.len() as u64 > MAX_FILE {
            return Err(format!("{name}: entry is too large"));
        }
        Ok(out)
    }
}

fn phxm_zip(buf: &[u8]) -> Result<Map, String> {
    let mut zip = Zip::open(buf)?;
    let meta = json(&zip.entry("metadata.json")?)?;
    let notes = phxmo(&zip.entry("objects.phxmo")?)?;
    let audio = if meta["HasAudio"].as_bool().unwrap_or(false) {
        Some(zip.entry(&format!("audio.{}", audio_ext(&meta)))?)
    } else {
        None
    };
    Ok(Map::new(title_of(&meta), mappers_of(&meta), notes, audio))
}

#[derive(Debug)]
struct Osu {
    title: String,
    mapper: String,
    audio: String,
    notes: Vec<Note>,
}

fn osu(text: &str) -> Result<Osu, String> {
    let mut section = "";
    let mut found = Osu {
        title: String::new(),
        mapper: String::new(),
        audio: String::new(),
        notes: Vec::new(),
    };
    let (mut title, mut artist, mut mode) = (String::new(), String::new(), 0_u32);
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name;
            continue;
        }
        match section {
            "General" | "Metadata" => {
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let value = value.trim().to_string();
                match key.trim() {
                    "AudioFilename" => found.audio = value,
                    "Mode" => {
                        mode = value
                            .parse()
                            .map_err(|_| format!("unreadable osu mode '{value}'"))?;
                    }
                    "Title" => title = value,
                    "Artist" => artist = value,
                    "Creator" => found.mapper = value,
                    _ => {}
                }
            }
            "HitObjects" => {
                let mut f = line.split(',');
                let (Some(x), Some(y), Some(t), Some(kind)) = (
                    f.next().and_then(|v| v.trim().parse::<f32>().ok()),
                    f.next().and_then(|v| v.trim().parse::<f32>().ok()),
                    f.next().and_then(|v| v.trim().parse::<f64>().ok()),
                    f.next().and_then(|v| v.trim().parse::<u32>().ok()),
                ) else {
                    continue;
                };
                if kind & 8 != 0 {
                    continue;
                }
                found.notes.push(Note {
                    time: t / 1000.0,
                    x: ((x / 512.0 - 0.5) * 2.0).clamp(-1.0, 1.0),
                    y: ((0.5 - y / 384.0) * 2.0).clamp(-1.0, 1.0),
                });
            }
            _ => {}
        }
    }
    if mode != 0 {
        return Err(format!(
            "only osu!standard maps have aim positions (this one is mode {mode})"
        ));
    }
    found.title = if artist.is_empty() {
        title
    } else {
        format!("{artist} - {title}")
    };
    Ok(found)
}

fn osu_file(path: &Path) -> Result<Map, String> {
    let text = read_text(path)?;
    let parsed = osu(&text)?;
    let audio = path
        .parent()
        .zip(Path::new(&parsed.audio).file_name())
        .map(|(d, name)| d.join(name))
        .filter(|p| p.is_file())
        .map(|p| read(&p))
        .transpose()?;
    Ok(Map::new(parsed.title, parsed.mapper, parsed.notes, audio))
}

fn osz(buf: &[u8]) -> Result<Map, String> {
    let mut zip = Zip::open(buf)?;
    let mut best: Option<Osu> = None;
    let mut last_error = String::from("no .osu difficulty inside");
    for name in zip
        .names()
        .into_iter()
        .filter(|n| n.to_lowercase().ends_with(".osu"))
    {
        let text = String::from_utf8_lossy(&zip.entry(&name)?).into_owned();
        match osu(&text) {
            Ok(o) if best.as_ref().is_none_or(|b| o.notes.len() > b.notes.len()) => best = Some(o),
            Ok(_) => {}
            Err(e) => last_error = e,
        }
    }
    let parsed = best.ok_or(last_error)?;
    let audio_name = zip
        .names()
        .into_iter()
        .find(|n| n.eq_ignore_ascii_case(&parsed.audio));
    let audio = audio_name.map(|n| zip.entry(&n)).transpose()?;
    Ok(Map::new(parsed.title, parsed.mapper, parsed.notes, audio))
}

fn rhm(buf: &[u8]) -> Result<Map, String> {
    let mut zip = Zip::open(buf)?;
    let data = json(&zip.entry("map")?)?;
    let notes = data["Notes"]
        .as_array()
        .ok_or("missing Notes")?
        .iter()
        .map(|n| {
            Some(Note {
                time: n["Time"].as_f64()? / 1000.0,
                x: n["X"].as_f64()? as f32 - 1.0,
                y: 1.0 - n["Y"].as_f64()? as f32,
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("malformed note")?;
    let audio = zip
        .entry("audio")
        .or_else(|_| zip.entry(data["AudioFileName"].as_str().unwrap_or("")))
        .ok();
    let title = data["Title"].as_str().unwrap_or("").trim().to_string();
    Ok(Map::new(title, mappers_of(&data), notes, audio))
}

#[cfg(test)]
mod osu_tests {
    use super::*;

    const MAP: &str = "osu file format v14\n[General]\nAudioFilename: song.mp3\nMode: 0\n[Metadata]\nTitle:Tune\nArtist:Band\nCreator:someone\n[HitObjects]\n256,192,1000,1,0\n0,0,1500,2,0,L|100:100,1,100\n100,100,2000,12,0,3000\n512,384,2500,5,0\n";

    #[test]
    fn reads_circles_and_slider_heads_but_not_spinners() {
        let o = osu(MAP).unwrap();
        assert_eq!(o.title, "Band - Tune");
        assert_eq!(o.mapper, "someone");
        assert_eq!(o.audio, "song.mp3");
        let got: Vec<(f64, f32, f32)> = o.notes.iter().map(|n| (n.time, n.x, n.y)).collect();
        assert_eq!(
            got,
            vec![(1.0, 0.0, 0.0), (1.5, -1.0, 1.0), (2.5, 1.0, -1.0)]
        );
    }

    #[test]
    fn other_osu_modes_are_refused() {
        assert!(
            osu(&MAP.replace("Mode: 0", "Mode: 3"))
                .unwrap_err()
                .contains("mode 3")
        );
    }

    #[test]
    fn osz_picks_the_biggest_difficulty_and_its_audio() {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        let small = MAP.replace("2500,5,0\n", "");
        for (name, body) in [
            ("easy.osu", small.as_bytes()),
            ("hard.osu", MAP.as_bytes()),
            ("Song.MP3", b"audio".as_slice()),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let map = osz(&bytes).unwrap();
        assert_eq!(map.notes.len(), 3);
        assert_eq!(map.audio.as_deref(), Some(b"audio".as_slice()));
    }
}

#[cfg(test)]
mod difficulty_tests {
    use super::*;

    fn notes_every(step: f64, count: usize) -> Vec<Note> {
        (0..count)
            .map(|i| Note {
                time: i as f64 * step,
                x: 0.0,
                y: 0.0,
            })
            .collect()
    }

    #[test]
    fn hard_keeps_every_note() {
        let mut notes = notes_every(0.1, 50);
        Difficulty::Hard.apply(&mut notes);
        assert_eq!(notes.len(), 50);
    }

    #[test]
    fn easier_levels_keep_fewer_notes_and_respect_their_gap() {
        let all = notes_every(0.1, 100);
        let mut counts = Vec::new();
        for d in Difficulty::ALL {
            let mut notes = all.clone();
            d.apply(&mut notes);
            for pair in notes.windows(2) {
                assert!(pair[1].time - pair[0].time >= d.min_gap() - 1e-6);
            }
            counts.push(notes.len());
        }
        assert!(counts[0] < counts[1] && counts[1] < counts[2], "{counts:?}");
    }

    #[test]
    fn sparse_maps_are_left_alone() {
        let mut notes = notes_every(0.5, 20);
        Difficulty::Easy.apply(&mut notes);
        assert_eq!(notes.len(), 20);
    }

    #[test]
    fn hard_scores_keep_the_old_key() {
        assert_eq!(Difficulty::Hard.key("song.mp3"), "song.mp3");
        assert_eq!(Difficulty::Easy.key("song.mp3"), "song.mp3 (Easy)");
    }
}
