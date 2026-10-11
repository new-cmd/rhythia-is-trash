use std::{fs, io, path::Path};

pub fn write_atomic(path: &str, contents: &str) -> io::Result<()> {
    let tmp = format!("{path}.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, Path::new(path)).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_the_file_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!("rhythm-store-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("data.json");
        let path = path.to_str().unwrap();
        write_atomic(path, "one").unwrap();
        write_atomic(path, "two").unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "two");
        assert!(!Path::new(&format!("{path}.tmp")).exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
