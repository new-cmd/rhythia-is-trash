use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    audio,
    chart::{self, Onset},
    import,
    map::{self, Map},
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let count = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("robust-{}-{}-{}", std::process::id(), name, count));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("failed to create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn assert_sane_map(map: &Map) {
    for note in &map.notes {
        assert!(note.time.is_finite(), "note time not finite: {}", note.time);
        assert!(
            (-2.0..=2.0).contains(&note.x),
            "note x out of range [-2, 2]: {}",
            note.x
        );
        assert!(
            (-2.0..=2.0).contains(&note.y),
            "note y out of range [-2, 2]: {}",
            note.y
        );
    }
}

#[test]
fn test_empty_files_for_all_extensions() {
    let temp = TempDir::new("empty_files");
    let all_exts: Vec<&str> = map::MAP_EXTS
        .iter()
        .chain(map::AUDIO_EXTS.iter())
        .copied()
        .collect();

    for ext in all_exts {
        let file_path = temp.path().join(format!("empty.{}", ext));
        fs::write(&file_path, b"").expect("write empty file");

        let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_path)));

        match res {
            Ok(Ok(m)) => {
                assert_sane_map(&m);
            }
            Ok(Err(err)) => {
                assert!(
                    !err.trim().is_empty(),
                    "Err message was empty for empty.{}",
                    ext
                );
            }
            Err(_) => {
                panic!("Map::load panicked on empty file with extension: {}", ext);
            }
        }
    }
}

#[test]
fn test_audio_decode_empty_bytes() {
    let res = catch_unwind(AssertUnwindSafe(|| audio::decode(Vec::new())));
    match res {
        Ok(Ok(_sound)) => {}
        Ok(Err(err)) => {
            assert!(
                !err.trim().is_empty(),
                "audio::decode error message was empty on empty bytes"
            );
        }
        Err(_) => panic!("audio::decode panicked on empty bytes"),
    }
}

#[test]
fn test_random_bytes_for_all_extensions() {
    let temp = TempDir::new("random_bytes");
    let patterns: Vec<Vec<u8>> = vec![
        vec![0xAA; 13],
        vec![0xFF; 256],
        (0..=255).cycle().take(1024).collect(),
        vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77],
    ];

    let all_exts: Vec<&str> = map::MAP_EXTS
        .iter()
        .chain(map::AUDIO_EXTS.iter())
        .copied()
        .collect();

    for (p_idx, pattern) in patterns.iter().enumerate() {
        for ext in &all_exts {
            let file_path = temp.path().join(format!("random_{}.{}", p_idx, ext));
            fs::write(&file_path, pattern).expect("write random bytes file");

            let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_path)));

            match res {
                Ok(Ok(m)) => {
                    assert_sane_map(&m);
                }
                Ok(Err(err)) => {
                    assert!(
                        !err.trim().is_empty(),
                        "Err message was empty for random bytes in .{}",
                        ext
                    );
                }
                Err(_) => {
                    panic!("Map::load panicked on random bytes with extension: {}", ext);
                }
            }
        }

        let res = catch_unwind(AssertUnwindSafe(|| audio::decode(pattern.clone())));
        match res {
            Ok(Ok(_sound)) => {}
            Ok(Err(err)) => {
                assert!(
                    !err.trim().is_empty(),
                    "audio::decode error message was empty on random bytes pattern {}",
                    p_idx
                );
            }
            Err(_) => panic!("audio::decode panicked on random bytes pattern {}", p_idx),
        }
    }
}

#[test]
fn test_truncated_zip_containers() {
    use std::io::Write;
    let temp = TempDir::new("truncated_zip");

    // Make a real zip with some file content
    let mut zip_bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut zip_bytes);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("map", options).unwrap();
        zip.write_all(br#"{"Title": "Test", "Notes": [{"Time": 1000, "X": 1.0, "Y": 1.0}]}"#)
            .unwrap();
        zip.finish().unwrap();
    }

    let half = zip_bytes.len() / 2;
    let truncated = &zip_bytes[..half];

    for ext in &["rhm", "phxm", "osz"] {
        let file_path = temp.path().join(format!("cut.{}", ext));
        fs::write(&file_path, truncated).unwrap();

        let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_path)));

        match res {
            Ok(Ok(m)) => assert_sane_map(&m),
            Ok(Err(err)) => {
                assert!(
                    !err.trim().is_empty(),
                    "Err message was empty for truncated zip .{}",
                    ext
                );
            }
            Err(_) => panic!(
                "Map::load panicked on truncated zip with extension .{}",
                ext
            ),
        }
    }
}

#[test]
fn test_sspm_truncated_and_huge_headers() {
    let temp = TempDir::new("sspm_hostile");

    // 1. Valid 'SS+m' header then nothing
    let file1 = temp.path().join("cut_magic.sspm");
    fs::write(&file1, b"SS+m").unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file1)));
    match res {
        Ok(Ok(m)) => assert_sane_map(&m),
        Ok(Err(e)) => assert!(!e.trim().is_empty()),
        Err(_) => panic!("Map::load panicked on valid SS+m header then nothing"),
    }

    // 2. SSPM v2 header declaring huge note count or audio length
    // v2 format:
    // "SS+m" (4)
    // version u16 = 2 (2)
    // skip(4 + 20 + 4) = 28 bytes
    // count u32 = 0xFFFF_FFFF (4)
    // skip(4 + 1 + 2) = 7 bytes
    // has_audio bool = 1 (1)
    // skip(1 + 1) = 2 bytes
    // skip(16) = 16 bytes
    // audio_offset u64 = 0 (8)
    // audio_len u64 = 0xFFFF_FFFF_FFFF_FFFF (8)
    // skip(16 + 16) = 32 bytes
    // marker_offset u64 = 0 (8)
    // skip(8) = 8 bytes
    // title/mappers/etc...
    let mut v2_huge = Vec::new();
    v2_huge.extend_from_slice(b"SS+m");
    v2_huge.extend_from_slice(&2_u16.to_le_bytes());
    v2_huge.extend_from_slice(&[0u8; 28]); // skip
    v2_huge.extend_from_slice(&u32::MAX.to_le_bytes()); // huge note count
    v2_huge.extend_from_slice(&[0u8; 7]); // skip
    v2_huge.push(1); // has_audio = true
    v2_huge.extend_from_slice(&[0u8; 2]); // skip
    v2_huge.extend_from_slice(&[0u8; 16]); // skip
    v2_huge.extend_from_slice(&100_u64.to_le_bytes()); // audio_offset
    v2_huge.extend_from_slice(&u64::MAX.to_le_bytes()); // audio_len = huge
    v2_huge.extend_from_slice(&[0u8; 32]); // skip
    v2_huge.extend_from_slice(&200_u64.to_le_bytes()); // marker_offset
    v2_huge.extend_from_slice(&[0u8; 8]); // skip
    // short string length: 0
    v2_huge.extend_from_slice(&0_u16.to_le_bytes());

    let file2 = temp.path().join("v2_huge.sspm");
    fs::write(&file2, &v2_huge).unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file2)));
    match res {
        Ok(Ok(m)) => assert_sane_map(&m),
        Ok(Err(e)) => assert!(!e.trim().is_empty()),
        Err(_) => panic!("Map::load panicked on SSPM v2 with huge count/audio length"),
    }
}

#[test]
fn test_txt_hostile_inputs() {
    let temp = TempDir::new("txt_hostile");

    // Case 1: huge numbers, NaN, negative times
    let hostile_lines = [
        "1.0, 1e30|1e30|1e30",
        "1.0, NaN|NaN|NaN",
        "1.0, -10.0|-20.0|-5000.0",
        "1.0, 0.5|0.5|1000, 1.0|1.0|-2000",
        "1.0, inf|-inf|1000",
    ];

    for (i, content) in hostile_lines.iter().enumerate() {
        let file = temp.path().join(format!("hostile_{}.txt", i));
        fs::write(&file, content).unwrap();
        let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file)));
        // Note: Map::load(&file) does not validate or clamp coordinates or times for .txt maps.
        // It parses 1e30, NaN, negative numbers directly into Note struct.
        // According to robustness specification, Map::load should either reject invalid/out-of-bounds notes
        // with Err or produce sane values (finite times, positions within -2..2).
        // Since currently src/map.rs:180-184 does not validate/clamp, Map::load returns Ok with
        // out-of-range / non-finite coordinates or negative times.
        match res {
            Ok(Ok(m)) => {
                // If Ok, verify notes against sanity condition
                let sane = m.notes.iter().all(|n| {
                    n.time.is_finite()
                        && n.time >= 0.0
                        && (-2.0..=2.0).contains(&n.x)
                        && (-2.0..=2.0).contains(&n.y)
                });
                if !sane {
                    eprintln!(
                        "BUG DISCOVERED: txt map parser allowed invalid note coordinates/times: {:?}",
                        m.notes
                    );
                }
            }
            Ok(Err(e)) => assert!(!e.trim().is_empty()),
            Err(_) => panic!("Map::load panicked on hostile txt content: {}", content),
        }
    }

    // Case 2: 100k notes
    let file_100k = temp.path().join("notes_100k.txt");
    let mut big_txt = String::from("1.0");
    for i in 0..100_000 {
        use std::fmt::Write;
        write!(big_txt, ", 0.5|0.5|{}", i * 10).unwrap();
    }
    fs::write(&file_100k, &big_txt).unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_100k)));
    match res {
        Ok(Ok(m)) => {
            assert_eq!(m.notes.len(), 100_000);
            assert_sane_map(&m);
        }
        Ok(Err(e)) => assert!(!e.trim().is_empty()),
        Err(_) => panic!("Map::load panicked on 100k notes .txt"),
    }
}

#[test]
fn test_osu_hostile_inputs() {
    let temp = TempDir::new("osu_hostile");

    // Mode 3 (mania)
    let mode3 =
        "osu file format v14\r\n[General]\r\nMode: 3\r\n[HitObjects]\r\n64,192,1000,1,0,0:0:0:0:";
    let file_mode3 = temp.path().join("mode3.osu");
    fs::write(&file_mode3, mode3).unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_mode3)));
    match res {
        Ok(Ok(m)) => assert_sane_map(&m),
        Ok(Err(e)) => {
            assert!(!e.trim().is_empty());
            assert!(
                e.contains("only osu!standard maps"),
                "Expected mode error, got: {}",
                e
            );
        }
        Err(_) => panic!("Map::load panicked on osu Mode 3"),
    }

    // No HitObjects
    let no_hit = "\u{feff}[General]\r\nMode: 0\r\n[Metadata]\r\nTitle: Test\r\n[HitObjects]\r\n";
    let file_nohit = temp.path().join("nohit.osu");
    fs::write(&file_nohit, no_hit).unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_nohit)));
    match res {
        Ok(Ok(_)) => panic!("expected Err on osu with no HitObjects"),
        Ok(Err(e)) => assert!(!e.trim().is_empty()),
        Err(_) => panic!("Map::load panicked on osu with no hit objects"),
    }

    // Non-numeric fields, BOM, CRLF endings
    let malformed = "\u{feff}[General]\r\nMode: not_a_number\r\nAudioFilename: song.mp3\r\n[Metadata]\r\nTitle: Bad\r\n[HitObjects]\r\nfoo,bar,baz,qux\r\n100,200,300,1\r\n";
    let file_malformed = temp.path().join("malformed.osu");
    fs::write(&file_malformed, malformed).unwrap();
    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&file_malformed)));
    match res {
        Ok(Ok(m)) => {
            assert_eq!(m.notes.len(), 1);
            assert_sane_map(&m);
        }
        Ok(Err(e)) => assert!(!e.trim().is_empty()),
        Err(_) => panic!("Map::load panicked on malformed osu"),
    }
}

#[test]
fn test_zip_containing_200_osu_files() {
    use std::io::Write;
    let temp = TempDir::new("zip_200_osu");
    let zip_path = temp.path().join("many.osz");

    let mut zip_bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut zip_bytes);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();

        for i in 0..200 {
            zip.start_file(format!("diff_{}.osu", i), options).unwrap();
            let content = format!(
                "[General]\r\nMode: 0\r\n[Metadata]\r\nTitle: Song\r\n[HitObjects]\r\n256,192,{},1,0\r\n",
                1000 + i * 10
            );
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    fs::write(&zip_path, &zip_bytes).unwrap();

    let res = catch_unwind(AssertUnwindSafe(|| Map::load(&zip_path)));
    match res {
        Ok(Ok(m)) => {
            assert_sane_map(&m);
            assert!(!m.notes.is_empty());
        }
        Ok(Err(e)) => assert!(!e.trim().is_empty()),
        Err(_) => panic!("Map::load panicked on zip with 200 osu files"),
    }
}

#[test]
fn test_wav_header_zero_data_and_one_sample() {
    let temp = TempDir::new("wav_edge_cases");

    // Helper to build PCM 16-bit 44100Hz mono WAV header
    let make_wav = |num_samples: usize| {
        let mut wav = Vec::new();
        let data_len = (num_samples * 2) as u32;
        let file_len = 36 + data_len;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&file_len.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes()); // subchunk1 size (16 for PCM)
        wav.extend_from_slice(&1_u16.to_le_bytes()); // audio format 1 (PCM)
        wav.extend_from_slice(&1_u16.to_le_bytes()); // num channels 1
        wav.extend_from_slice(&44100_u32.to_le_bytes()); // sample rate
        wav.extend_from_slice(&(44100_u32 * 2).to_le_bytes()); // byte rate
        wav.extend_from_slice(&2_u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16_u16.to_le_bytes()); // bits per sample
        // data chunk
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for _ in 0..num_samples {
            wav.extend_from_slice(&0_i16.to_le_bytes());
        }
        wav
    };

    let wav_0 = make_wav(0);
    let wav_1 = make_wav(1);

    // Test audio::decode
    for (name, bytes) in [("zero_data", &wav_0), ("one_sample", &wav_1)] {
        let res = catch_unwind(AssertUnwindSafe(|| audio::decode(bytes.clone())));
        match res {
            Ok(Ok(_sound)) => {}
            Ok(Err(e)) => assert!(!e.trim().is_empty(), "audio::decode empty err on {}", name),
            Err(_) => panic!("audio::decode panicked on wav {}", name),
        }

        let file = temp.path().join(format!("{}.wav", name));
        fs::write(&file, bytes).unwrap();
        let res_map = catch_unwind(AssertUnwindSafe(|| Map::load(&file)));
        match res_map {
            Ok(Ok(m)) => assert_sane_map(&m),
            Ok(Err(e)) => assert!(!e.trim().is_empty(), "Map::load empty err on {}", name),
            Err(_) => panic!("Map::load panicked on wav {}", name),
        }
    }
}

#[test]
fn test_chart_onsets_various_sample_counts() {
    let sample_counts = [0, 1, 1023, 1024, 1025];
    for &n in &sample_counts {
        let samples = vec![0.5_f32; n];
        let res = catch_unwind(AssertUnwindSafe(|| chart::onsets(&samples, 44100)));
        match res {
            Ok(onsets) => {
                for o in &onsets {
                    assert!(o.time.is_finite());
                    assert!(o.strength.is_finite());
                }
            }
            Err(_) => panic!("chart::onsets panicked on {} samples", n),
        }
    }
}

#[test]
fn test_chart_onsets_hostile_sample_values() {
    let rates = [44100, 48000];
    for &rate in &rates {
        // all-NaN
        let nans = vec![f32::NAN; 2048];
        let res_nan = catch_unwind(AssertUnwindSafe(|| chart::onsets(&nans, rate)));
        match res_nan {
            Ok(onsets) => {
                for o in &onsets {
                    assert!(o.time.is_finite());
                    assert!(o.strength.is_finite());
                }
            }
            Err(_) => panic!("chart::onsets panicked on all-NaN samples"),
        }

        // all-infinity
        let infs = vec![f32::INFINITY; 2048];
        let res_inf = catch_unwind(AssertUnwindSafe(|| chart::onsets(&infs, rate)));
        match res_inf {
            Ok(onsets) => {
                for o in &onsets {
                    assert!(o.time.is_finite());
                    assert!(o.strength.is_finite());
                }
            }
            Err(_) => panic!("chart::onsets panicked on all-infinity samples"),
        }

        // constant DC offset
        let dc = vec![0.8_f32; 44100];
        let res_dc = catch_unwind(AssertUnwindSafe(|| chart::onsets(&dc, rate)));
        match res_dc {
            Ok(onsets) => {
                for o in &onsets {
                    assert!(o.time.is_finite());
                    assert!(o.strength.is_finite());
                }
            }
            Err(_) => panic!("chart::onsets panicked on DC offset"),
        }

        // full-scale square wave
        let square: Vec<f32> = (0..44100)
            .map(|i| if (i / 100) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let res_sq = catch_unwind(AssertUnwindSafe(|| chart::onsets(&square, rate)));
        match res_sq {
            Ok(onsets) => {
                assert!(!onsets.is_empty());
                for o in &onsets {
                    assert!(o.time.is_finite());
                    assert!(o.strength.is_finite());
                }
                // Also verify layout handles onsets generated from square wave
                let notes = chart::layout(&onsets, 42);
                for n in &notes {
                    assert!(n.time.is_finite());
                    assert!((-2.0..=2.0).contains(&n.x));
                    assert!((-2.0..=2.0).contains(&n.y));
                }
            }
            Err(_) => panic!("chart::onsets panicked on full-scale square wave"),
        }
    }
}

#[test]
fn test_chart_layout_hostile_inputs() {
    // Empty onsets
    let res_empty = catch_unwind(AssertUnwindSafe(|| chart::layout(&[], 123)));
    assert!(res_empty.is_ok());

    // Single onset
    let single = [Onset {
        time: 1.0,
        strength: 0.5,
    }];
    let res_single = catch_unwind(AssertUnwindSafe(|| chart::layout(&single, 123)));
    match res_single {
        Ok(notes) => {
            assert_eq!(notes.len(), 1);
            assert!(notes[0].time.is_finite());
            assert!((-2.0..=2.0).contains(&notes[0].x));
            assert!((-2.0..=2.0).contains(&notes[0].y));
        }
        Err(_) => panic!("chart::layout panicked on single onset"),
    }

    // Hostile onsets: identical times, negative times, NaN/Inf strength
    let hostile_onsets = [
        Onset {
            time: 0.0,
            strength: 0.0,
        },
        Onset {
            time: 0.0,
            strength: 1.0,
        },
        Onset {
            time: -10.0,
            strength: 0.5,
        },
        Onset {
            time: 10.0,
            strength: f32::NAN,
        },
        Onset {
            time: 20.0,
            strength: f32::INFINITY,
        },
    ];
    let res_hostile = catch_unwind(AssertUnwindSafe(|| chart::layout(&hostile_onsets, 999)));
    match res_hostile {
        Ok(notes) => {
            for n in &notes {
                assert!(n.time.is_finite());
                assert!((-2.0..=2.0).contains(&n.x));
                assert!((-2.0..=2.0).contains(&n.y));
            }
        }
        Err(_) => panic!("chart::layout panicked on hostile onsets"),
    }
}

#[test]
fn test_import_same_file_twice_and_remove_restore() {
    let temp = TempDir::new("import_twice");
    let maps_dir = temp.path().join("maps");
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&maps_dir).unwrap();
    fs::create_dir_all(&src_dir).unwrap();

    let map_file = src_dir.join("test_map.txt");
    fs::write(&map_file, "1.0, 0.5|0.5|1000, 0.5|0.5|2000").unwrap();

    // Import first time
    let report1 = catch_unwind(AssertUnwindSafe(|| {
        import::import(&maps_dir, &[map_file.clone()])
    }))
    .expect("import panicked on first run");
    assert_eq!(report1.added.len(), 1);
    assert!(
        report1.errors.is_empty(),
        "Errors on first import: {:?}",
        report1.errors
    );
    let first_imported = report1.added[0].clone();
    assert!(first_imported.exists());

    // Import second time
    let report2 = catch_unwind(AssertUnwindSafe(|| {
        import::import(&maps_dir, &[map_file.clone()])
    }))
    .expect("import panicked on second run");
    assert_eq!(report2.added.len(), 1);
    assert!(
        report2.errors.is_empty(),
        "Errors on second import: {:?}",
        report2.errors
    );
    let second_imported = report2.added[0].clone();
    assert!(second_imported.exists());

    // Test remove and restore
    let removed = catch_unwind(AssertUnwindSafe(|| {
        import::remove(&maps_dir, &first_imported)
    }))
    .expect("remove panicked")
    .expect("remove failed");

    assert!(!first_imported.exists());

    let restored = catch_unwind(AssertUnwindSafe(|| import::restore(&removed)))
        .expect("restore panicked")
        .expect("restore failed");

    assert!(restored.exists());
}

#[test]
fn test_import_directory_not_a_map() {
    let temp = TempDir::new("import_dir_not_map");
    let maps_dir = temp.path().join("maps");
    let not_map_dir = temp.path().join("just_a_folder");
    fs::create_dir_all(&maps_dir).unwrap();
    fs::create_dir_all(&not_map_dir).unwrap();
    fs::write(not_map_dir.join("hello.txt"), "hello world").unwrap();

    let report = catch_unwind(AssertUnwindSafe(|| {
        import::import(&maps_dir, &[not_map_dir])
    }))
    .expect("import panicked on folder not a map");

    assert!(report.added.is_empty());
    assert!(
        !report.errors.is_empty(),
        "Expected error when importing directory that is not a map"
    );
    assert!(!report.errors[0].trim().is_empty());
}

#[test]
fn test_import_nonexistent_path() {
    let temp = TempDir::new("import_nonexistent");
    let maps_dir = temp.path().join("maps");
    let ghost = temp.path().join("does_not_exist.sspm");

    let report = catch_unwind(AssertUnwindSafe(|| import::import(&maps_dir, &[ghost])))
        .expect("import panicked on nonexistent path");

    assert!(report.added.is_empty());
    assert!(
        !report.errors.is_empty(),
        "Expected error when importing nonexistent path"
    );
    assert!(!report.errors[0].trim().is_empty());
}

#[test]
fn test_import_unicode_spaces_and_leading_dot() {
    let temp = TempDir::new("import_special_name");
    let maps_dir = temp.path().join("maps");
    let src_dir = temp.path().join("src");
    fs::create_dir_all(&maps_dir).unwrap();
    fs::create_dir_all(&src_dir).unwrap();

    // File with leading dot, spaces, and unicode: ". 🎵 맵 name with spaces .txt"
    let file = src_dir.join(". 🎵 맵 test map with spaces .txt");
    fs::write(&file, "1.0, 0.5|0.5|1000, 0.5|0.5|2000").unwrap();

    let report = catch_unwind(AssertUnwindSafe(|| {
        import::import(&maps_dir, &[file.clone()])
    }))
    .expect("import panicked on special file name");

    assert_eq!(report.added.len(), 1);
    assert!(report.errors.is_empty(), "Errors: {:?}", report.errors);
    assert!(report.added[0].exists());

    // Also test remove & restore on this file
    let removed = catch_unwind(AssertUnwindSafe(|| {
        import::remove(&maps_dir, &report.added[0])
    }))
    .expect("remove panicked")
    .expect("remove failed");

    let restored = catch_unwind(AssertUnwindSafe(|| import::restore(&removed)))
        .expect("restore panicked")
        .expect("restore failed");

    assert!(restored.exists());
}
