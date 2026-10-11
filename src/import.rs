use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::map::{self, Map};

#[derive(Default)]
pub struct Report {
    pub added: Vec<PathBuf>,
    pub audio: usize,
    pub errors: Vec<String>,
}

impl Report {
    pub fn notice(&self) -> Option<String> {
        let first = self
            .added
            .last()?
            .file_name()?
            .to_string_lossy()
            .into_owned();
        Some(match (self.added.len(), self.audio) {
            (1, 0) => format!("added {first}"),
            (1, a) => format!("added {first} (+{a} audio)"),
            (n, _) => format!("added {n} maps"),
        })
    }

    pub fn error(&self) -> Option<String> {
        let first = self.errors.first()?;
        Some(match self.errors.len() {
            1 => first.clone(),
            n => format!("{first} (and {} more)", n - 1),
        })
    }
}

pub fn import(dir: &Path, paths: &[PathBuf]) -> Report {
    let mut report = Report::default();
    if let Err(e) = fs::create_dir_all(dir) {
        report.errors.push(format!("{}: {e}", dir.display()));
        return report;
    }
    let unpacked = Unpacked::new();
    let paths = expand(paths, &unpacked, &mut report);
    for path in &paths {
        match import_one(dir, path, &paths) {
            Ok(Some(dest)) => report.added.push(dest),
            Ok(None) => report.audio += 1,
            Err(e) => report.errors.push(e),
        }
    }
    report
}

const MAX_ENTRY: u64 = 1 << 30;
const MAX_ENTRIES: usize = 4000;

struct Unpacked(PathBuf);

impl Unpacked {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!("rhythm-unpack-{}-{n}", std::process::id())))
    }
}

impl Drop for Unpacked {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn is_zip(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
}

fn wanted(path: &Path) -> bool {
    map::is_map(path) || map::is_audio(path)
}

/// Turns folders and .zip packs into the plain map and song files inside them.
fn expand(paths: &[PathBuf], unpacked: &Unpacked, report: &mut Report) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() && !map::is_map(path) {
            collect(path, 0, &mut out);
        } else if is_zip(path) {
            if let Err(e) = unzip(path, unpacked, &mut out) {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        } else {
            out.push(path.clone());
        }
    }
    out
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut found: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    found.sort();
    for p in found {
        if p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if p.is_dir() && !map::is_map(&p) {
            if depth < 3 && out.len() < MAX_ENTRIES {
                collect(&p, depth + 1, out);
            }
        } else if wanted(&p) && out.len() < MAX_ENTRIES {
            out.push(p);
        }
    }
}

fn unzip(path: &Path, unpacked: &Unpacked, out: &mut Vec<PathBuf>) -> Result<(), String> {
    use std::io::Read;

    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file)).map_err(|e| e.to_string())?;
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let folder = unpacked
        .0
        .join(if stem.is_empty() { "pack".into() } else { stem });
    let mut found = 0;
    for i in 0..zip.len().min(MAX_ENTRIES) {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() || entry.size() > MAX_ENTRY {
            continue;
        }
        let Some(name) = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(PathBuf::from))
        else {
            continue;
        };
        if !wanted(&name) {
            continue;
        }
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let target = unique(&folder, name.as_os_str());
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_ENTRY {
            continue;
        }
        fs::write(&target, bytes).map_err(|e| e.to_string())?;
        out.push(target);
        found += 1;
    }
    if found == 0 {
        return Err("no maps or songs inside this .zip".into());
    }
    Ok(())
}

fn import_one(dir: &Path, path: &Path, all: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{}: no file name", path.display()))?;
    let label = name.to_string_lossy().into_owned();
    let fail = |e: &dyn std::fmt::Display| format!("{label}: {e}");
    if map::is_audio(path) && map::has_sibling_map(path, all) {
        copy_file(dir, path, name).map_err(|e| fail(&e))?;
        return Ok(None);
    }
    if !map::is_map(path) && !map::is_audio(path) {
        return Err(fail(
            &"not a map or song. Drop maps (.sspm .phxm .rhm .txt .osz .osu), any song or video file (.mp3 .ogg .wav .flac .m4a .opus .mp4 .mkv and more), a folder, or a .zip pack",
        ));
    }
    Map::load(path).map_err(|e| {
        let prefix = format!("{}: ", path.display());
        fail(&e.strip_prefix(&prefix).unwrap_or(&e))
    })?;
    let dest = if path.is_dir() {
        copy_dir(dir, path, name)
    } else {
        copy_file(dir, path, name)
    };
    dest.map(Some).map_err(|e| fail(&e))
}

fn unique(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()));
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("an unused name exists")
}

fn copy_file(dir: &Path, src: &Path, name: &std::ffi::OsStr) -> io::Result<PathBuf> {
    let dest = dir.join(name);
    if dest.is_file() && same_content(&dest, src)? {
        return Ok(dest);
    }
    let dest = unique(dir, name);
    fs::copy(src, &dest)?;
    Ok(dest)
}

fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    use std::io::Read;

    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let mut fa = io::BufReader::new(fs::File::open(a)?);
    let mut fb = io::BufReader::new(fs::File::open(b)?);
    let (mut x, mut y) = (vec![0_u8; 64 * 1024], vec![0_u8; 64 * 1024]);
    loop {
        let n = fa.read(&mut x)?;
        if n == 0 {
            return Ok(true);
        }
        fb.read_exact(&mut y[..n])?;
        if x[..n] != y[..n] {
            return Ok(false);
        }
    }
}

fn copy_dir(dir: &Path, src: &Path, name: &std::ffi::OsStr) -> io::Result<PathBuf> {
    fn walk(from: &Path, to: &Path) -> io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let target = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                walk(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), target)?;
            }
        }
        Ok(())
    }
    let same = |a: &Path, b: &Path| {
        let len = |p: &Path| fs::metadata(p.join("objects.phxmo")).map(|m| m.len()).ok();
        len(a).is_some() && len(a) == len(b)
    };
    let existing = dir.join(name);
    if existing.is_dir() && same(src, &existing) {
        return Ok(existing);
    }
    let dest = unique(dir, name);
    walk(src, &dest)?;
    Ok(dest)
}

const TRASH: &str = ".trash";

pub struct Removed {
    pub label: String,
    moved: Vec<(PathBuf, PathBuf)>,
}

pub fn remove(dir: &Path, path: &Path) -> Result<Removed, String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{}: no file name", path.display()))?;
    let label = name.to_string_lossy().into_owned();
    let trash = dir.join(TRASH);
    fs::create_dir_all(&trash).map_err(|e| format!("{label}: {e}"))?;
    let mut items = vec![path.to_path_buf()];
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
    {
        items.extend(
            map::AUDIO_EXTS
                .iter()
                .map(|e| path.with_extension(e))
                .filter(|p| p.is_file()),
        );
    }
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for item in items {
        let file = item.file_name().unwrap_or(name);
        let dest = unique(&trash, file);
        if let Err(e) = fs::rename(&item, &dest) {
            for (from, trashed) in moved.iter().rev() {
                let _ = fs::rename(trashed, from);
            }
            return Err(format!("{label}: {e}"));
        }
        moved.push((item, dest));
    }
    Ok(Removed { label, moved })
}

pub fn restore(removed: &Removed) -> Result<PathBuf, String> {
    let mut done: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (from, trashed) in &removed.moved {
        let dir = from.parent().unwrap_or(Path::new("."));
        let name = from.file_name().unwrap_or_default();
        let dest = if from.exists() {
            unique(dir, name)
        } else {
            from.clone()
        };
        if let Err(e) = fs::rename(trashed, &dest) {
            for (placed, was) in done.iter().rev() {
                let _ = fs::rename(placed, was);
            }
            return Err(format!("{}: {e}", removed.label));
        }
        done.push((dest, trashed.clone()));
    }
    done.first()
        .map(|(dest, _)| dest.clone())
        .ok_or_else(|| format!("{}: nothing to restore", removed.label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rhythm-import-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_and_zip_packs_are_opened_and_junk_is_skipped() {
        use std::io::Write;

        let root = scratch("packs");
        let (src, maps) = (root.join("src"), root.join("maps"));
        fs::create_dir_all(src.join("inner/deeper")).unwrap();
        fs::write(src.join("inner/a.txt"), "x,1|1|500,0|2|900").unwrap();
        fs::write(src.join("inner/deeper/notes.md"), "ignore me").unwrap();
        fs::write(src.join("inner/.hidden.txt"), "x,1|1|500").unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(src.join("pack.zip")).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("songs/b.txt", "x,1|1|700,0|2|1200"),
            ("readme.md", "hi"),
            ("../evil.txt", "x,1|1|1"),
        ] {
            zip.start_file(name, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let report = import(&maps, &[src.join("inner"), src.join("pack.zip")]);
        assert_eq!(report.errors, Vec::<String>::new());
        let mut names: Vec<String> = map::scan(&maps)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert!(!maps.join("readme.md").exists() && !root.join("evil.txt").exists());
        let empty = src.join("empty.zip");
        zip::ZipWriter::new(fs::File::create(&empty).unwrap())
            .finish()
            .unwrap();
        assert!(
            import(&maps, &[empty])
                .error()
                .unwrap()
                .contains("no maps or songs")
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_removed_song_comes_back_by_dragging_it_in_again() {
        let root = scratch("readd");
        let (src, maps) = (root.join("src"), root.join("maps"));
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), "x,1|1|500,0|2|900").unwrap();
        fs::write(src.join("a.mp3"), b"not really audio").unwrap();

        let first = import(&maps, &[src.join("a.txt"), src.join("a.mp3")]);
        assert_eq!(first.errors, Vec::<String>::new());
        assert_eq!(map::scan(&maps).len(), 1);

        let gone = remove(&maps, &maps.join("a.txt")).unwrap();
        assert!(map::scan(&maps).is_empty());
        assert!(!maps.join("a.mp3").exists(), "its audio goes with it");

        let again = import(&maps, &[src.join("a.txt"), src.join("a.mp3")]);
        assert_eq!(again.errors, Vec::<String>::new());
        assert_eq!(map::scan(&maps), vec![maps.join("a.txt")]);
        assert!(maps.join("a.mp3").is_file());

        let restored = restore(&gone).unwrap();
        assert_ne!(
            restored,
            maps.join("a.txt"),
            "never overwrites a re-added copy"
        );
        assert!(restored.is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn undo_puts_the_files_back_where_they_were() {
        let root = scratch("undo");
        let maps = root.join("maps");
        fs::create_dir_all(&maps).unwrap();
        fs::write(maps.join("b.txt"), "x,1|1|500").unwrap();
        fs::write(maps.join("b.ogg"), b"audio").unwrap();
        let gone = remove(&maps, &maps.join("b.txt")).unwrap();
        assert!(!maps.join("b.txt").exists() && !maps.join("b.ogg").exists());
        assert_eq!(restore(&gone).unwrap(), maps.join("b.txt"));
        assert!(maps.join("b.ogg").is_file());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn removing_two_songs_with_the_same_name_keeps_both() {
        let root = scratch("twice");
        let maps = root.join("maps");
        fs::create_dir_all(&maps).unwrap();
        for body in ["x,1|1|500", "x,1|1|600"] {
            fs::write(maps.join("c.txt"), body).unwrap();
            remove(&maps, &maps.join("c.txt")).unwrap();
        }
        assert_eq!(fs::read_dir(maps.join(TRASH)).unwrap().count(), 2);
        fs::remove_dir_all(&root).unwrap();
    }
}
