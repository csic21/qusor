use super::BlobStage;
use core_api::{
    contract::sha256,
    knowledge::{KnowledgeError, KnowledgeErrorCode, LIMIT, Result},
};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
type Internal<T> = std::result::Result<T, Box<dyn Error>>;
fn storage(error: Box<dyn Error>) -> KnowledgeError {
    if let Some(value) = error.downcast_ref::<KnowledgeError>() {
        return value.clone();
    }
    if let Some(value) = error.downcast_ref::<std::io::Error>() {
        if value.kind() == std::io::ErrorKind::NotFound {
            return KnowledgeError::new(
                KnowledgeErrorCode::Integrity,
                "referenced immutable blob is missing [category=io kind=NotFound]",
            );
        }
        if value.kind() == std::io::ErrorKind::WouldBlock {
            return KnowledgeError::new(
                KnowledgeErrorCode::Conflict,
                "knowledge store already has a writer [category=io kind=WouldBlock]",
            );
        }
        let os_detail = value
            .raw_os_error()
            .map_or_else(String::new, |code| format!(" os={code}"));
        return KnowledgeError::new(
            KnowledgeErrorCode::Storage,
            format!(
                "immutable blob storage operation failed [category=io kind={:?}{os_detail}]",
                value.kind()
            ),
        );
    }
    KnowledgeError::new(
        KnowledgeErrorCode::Storage,
        "immutable blob storage operation failed [category=storage]",
    )
}
fn fail<T>(message: &str) -> Internal<T> {
    Err(KnowledgeError::new(KnowledgeErrorCode::Integrity, message).into())
}
struct WriterLock(File);
impl Drop for WriterLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}
pub struct BlobStore {
    _lock: WriterLock,
    root: PathBuf,
    reads: AtomicU64,
}
impl BlobStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_inner(path.as_ref()).map_err(storage)
    }
    fn open_inner(root: &Path) -> Internal<Self> {
        directory(root)?;
        directory(&root.join("blobs"))?;
        directory(&root.join("staging"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join("writer.lock"))?;
        if !lock.metadata()?.is_file() || lock.metadata()?.permissions().mode() & 0o077 != 0 {
            return fail("invalid writer lock");
        }
        fs2::FileExt::try_lock_exclusive(&lock)?;
        let guard = WriterLock(lock);
        Ok(Self {
            _lock: guard,
            root: root.to_owned(),
            reads: AtomicU64::new(0),
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn read_count(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
    }
    pub fn reset_read_count(&self) {
        self.reads.store(0, Ordering::Relaxed);
    }
    pub fn read(&self, hash: &str) -> Result<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.read_inner(hash).map_err(storage)
    }
    fn read_inner(&self, hash: &str) -> Internal<Vec<u8>> {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return fail("invalid blob hash");
        }
        let bytes = read_regular(&self.root.join("blobs").join(hash))?;
        if sha256(&bytes) != hash {
            return fail("snapshot corruption");
        }
        Ok(bytes)
    }
    pub fn publish(&self, bytes: &[u8], observe: &dyn Fn(BlobStage)) -> Result<()> {
        self.publish_inner(bytes, observe).map_err(storage)
    }
    fn publish_inner(&self, bytes: &[u8], observe: &dyn Fn(BlobStage)) -> Internal<()> {
        if bytes.len() > LIMIT {
            return fail("blob budget exceeded");
        }
        let hash = sha256(bytes);
        let target = self.root.join("blobs").join(&hash);
        if target.exists() {
            if self.read_inner(&hash)? != bytes {
                return fail("CAS mismatch");
            }
            return Ok(());
        }
        let mut counter = 0u64;
        let (tmp, mut file) = loop {
            let p = self
                .root
                .join("staging")
                .join(format!("{}-{counter}", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&p)
            {
                Ok(f) => break (p, f),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => counter += 1,
                Err(e) => return Err(e.into()),
            }
        };
        file.write_all(bytes)?;
        file.sync_all()?;
        observe(BlobStage::AfterWrite);
        observe(BlobStage::BeforePublish);
        match publish_noreplace(&tmp, &target) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                self.read_inner(&hash)?;
            }
            Err(e) => return Err(e.into()),
        };
        File::open(self.root.join("blobs"))?.sync_all()?;
        match fs::remove_file(tmp) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        File::open(self.root.join("staging"))?.sync_all()?;
        observe(BlobStage::AfterPublish);
        Ok(())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn publish_noreplace(source: &Path, target: &Path) -> std::io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};

    match renameat_with(CWD, source, CWD, target, RenameFlags::NOREPLACE) {
        Ok(()) => Ok(()),
        Err(error) if error == rustix::io::Errno::EXIST => {
            Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
        }
        Err(error) => Err(std::io::Error::from_raw_os_error(error.raw_os_error())),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn publish_noreplace(source: &Path, target: &Path) -> std::io::Result<()> {
    fs::hard_link(source, target)
}

fn directory(path: &Path) -> Internal<()> {
    if !path.exists() {
        fs::create_dir(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        if let Some(p) = path.parent() {
            File::open(p)?.sync_all()?;
        }
    }
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.permissions().mode() & 0o077 != 0 {
        return fail("store directory must be private and real");
    }
    Ok(())
}
fn read_regular(path: &Path) -> Internal<Vec<u8>> {
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !f.metadata()?.is_file() || f.metadata()?.len() > LIMIT as u64 {
        return fail("invalid or oversized file");
    }
    let mut b = Vec::new();
    Read::by_ref(&mut f)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut b)?;
    if b.len() > LIMIT {
        return fail("file budget exceeded");
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::super::BlobStore;
    use core_api::knowledge::KnowledgeErrorCode;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn blob_store_errors_keep_structured_context_and_hide_paths() {
        let dir =
            std::env::temp_dir().join(format!("qusor-blob-canary-77311-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        let store = BlobStore::open(&dir).unwrap();
        let err = store.read(&"a".repeat(64)).unwrap_err();
        assert_eq!(err.code, KnowledgeErrorCode::Integrity);
        assert!(err.message.contains("[category=io kind=NotFound]"));
        assert!(!err.message.contains("qusor-blob-canary-77311"));
        assert_eq!(store.read_count(), 1);
        store.reset_read_count();
        assert_eq!(store.read_count(), 0);

        let lock_err = match BlobStore::open(&dir) {
            Ok(_) => panic!("second writer must conflict"),
            Err(e) => e,
        };
        assert_eq!(lock_err.code, KnowledgeErrorCode::Conflict);
        assert!(lock_err.message.contains("[category=io kind=WouldBlock]"));
        assert!(!lock_err.message.contains("qusor-blob-canary-77311"));

        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
