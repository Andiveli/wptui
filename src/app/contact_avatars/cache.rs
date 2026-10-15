use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use whatsrust as wr;

const MAX_PERSISTED_BYTES: usize = 512 * 1024;

#[derive(Debug)]
pub struct AvatarDiskCache {
    root: PathBuf,
}

impl AvatarDiskCache {
    pub fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "avatar cache root is a symlink",
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn picture_path(&self, jid: &wr::JID, picture_id: &str) -> PathBuf {
        self.root.join(format!(
            "{}-{}.image",
            safe_key(jid.0.as_ref()),
            safe_key(picture_id)
        ))
    }

    pub(crate) fn current_path(&self, jid: &wr::JID) -> PathBuf {
        self.root
            .join(format!("{}.current", safe_key(jid.0.as_ref())))
    }

    pub fn load_current(&self, jid: &wr::JID) -> io::Result<Option<(Arc<str>, Vec<u8>)>> {
        let current = self.current_path(jid);
        let picture_id = match fs::read_to_string(&current) {
            Ok(value) if value.len() <= 1024 && !value.is_empty() => value,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let path = self.picture_path(jid, &picture_id);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "avatar cache entry is a symlink",
            ));
        }
        let bytes = fs::read(path)?;
        if bytes.is_empty() || bytes.len() > MAX_PERSISTED_BYTES {
            return Ok(None);
        }
        Ok(Some((picture_id.into(), bytes)))
    }

    pub fn store(&self, jid: &wr::JID, picture_id: &str, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_PERSISTED_BYTES || picture_id.len() > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid avatar cache entry",
            ));
        }
        atomic_write(&self.picture_path(jid, picture_id), bytes)?;
        atomic_write(&self.current_path(jid), picture_id.as_bytes())
    }
}

fn safe_key(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(&temporary) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(&temporary)?;
            options.open(&temporary)?
        }
        Err(error) => return Err(error),
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}
