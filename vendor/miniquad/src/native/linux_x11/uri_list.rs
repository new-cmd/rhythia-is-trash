use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

pub fn parse(bytes: &[u8]) -> Vec<PathBuf> {
    bytes
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty() && line[0] != b'#')
        .filter_map(to_path)
        .collect()
}

fn to_path(line: &[u8]) -> Option<PathBuf> {
    let Some(rest) = line.strip_prefix(b"file://") else {
        return (line.first() == Some(&b'/')).then(|| PathBuf::from(OsString::from_vec(line.to_vec())));
    };
    let start = rest.iter().position(|&b| b == b'/')?;
    Some(PathBuf::from(OsString::from_vec(percent_decode(&rest[start..]))))
}

fn percent_decode(input: &[u8]) -> Vec<u8> {
    let hex = |b: Option<&u8>| b.and_then(|b| (*b as char).to_digit(16)).map(|d| d as u8);
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let decoded = if input[i] == b'%' { hex(input.get(i + 1)).zip(hex(input.get(i + 2))) } else { None };
        match decoded {
            Some((hi, lo)) => {
                out.push(hi * 16 + lo);
                i += 3;
            }
            None => {
                out.push(input[i]);
                i += 1;
            }
        }
    }
    out
}
