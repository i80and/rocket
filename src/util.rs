use std::io;
use std::fs;
use std::path::Path;

pub fn visit_dirs(dir: &Path, cb: &Fn(&Path)) -> io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, cb)?;
            } else {
                cb(&path);
            }
        }
    }
    Ok(())
}
