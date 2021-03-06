use errors::*;
use std::fs;
use std::path::{Path, PathBuf};
use utils::try_hard_limit;
use walkdir::{DirEntry, WalkDir};

pub(crate) fn try_canonicalize<P: AsRef<Path>>(path: P) -> PathBuf {
    fs::canonicalize(&path).unwrap_or_else(|_| path.as_ref().to_path_buf())
}

pub(crate) fn remove_dir_all(dir: &Path) -> Result<()> {
    try_hard_limit(10, || {
        fs::remove_dir_all(dir)?;
        if dir.exists() {
            bail!("unable to remove directory");
        } else {
            Ok(())
        }
    })
}

pub(crate) fn copy_dir(src_dir: &Path, dest_dir: &Path) -> Result<()> {
    info!("copying {} to {}", src_dir.display(), dest_dir.display());

    if dest_dir.exists() {
        remove_dir_all(dest_dir).chain_err(|| "unable to remove test dir")?;
    }
    fs::create_dir_all(dest_dir).chain_err(|| "unable to create test dir")?;

    fn is_hidden(entry: &DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    }

    let mut partial_dest_dir = PathBuf::from("./");
    let mut depth = 0;
    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
    {
        let entry = entry.chain_err(|| "walk dir")?;
        while entry.depth() <= depth && depth > 0 {
            assert!(partial_dest_dir.pop());
            depth -= 1;
        }
        let path = dest_dir.join(&partial_dest_dir).join(entry.file_name());
        if entry.file_type().is_dir() && entry.depth() > 0 {
            fs::create_dir_all(&path)?;
            assert_eq!(entry.depth(), depth + 1);
            partial_dest_dir.push(entry.file_name());
            depth += 1;
        }
        if entry.file_type().is_file() {
            fs::copy(&entry.path(), path)?;
        }
    }

    Ok(())
}
