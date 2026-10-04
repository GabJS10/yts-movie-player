//! Streaming cache limit: LRU over the entries of `cache/` (one folder per torrent,
//! `cache/<infohash>/`), storage usage and "Vaciar caché".
//!
//! Sizes are **allocated** bytes (`st_blocks`, what `du` and `df` see), not apparent
//! lengths: librqbit creates every file at its final length up front (`set_len`, sparse),
//! so a stream that just started would otherwise count as the whole movie.
//!
//! Never touched: torrents in use (open streams, open readers), `cache/img` and anything
//! outside `cache/` (so never `library/`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use futures_util::future::BoxFuture;

use crate::error::{AppError, AppResult};
use crate::torrent::TorrentEngine;
use crate::types::{ClearCacheResult, StorageUsage};

/// Image cache inside `cache/`; kept by the LRU and by `clear_cache`.
pub const IMG_DIR: &str = "img";

/// How often the limit is enforced in the background (besides startup and new streams).
pub const ENFORCE_INTERVAL: Duration = Duration::from_secs(120);

/// Who owns the cache entries: knows which are in use and how to release them.
pub trait Evictor: Send + Sync {
    /// Names (infohashes) of the entries that must not be deleted.
    fn in_use(&self) -> HashSet<String>;
    /// Releases and deletes the entry. `Ok(false)` if it turned out to be in use.
    fn evict<'a>(&'a self, name: &'a str, path: &'a Path) -> BoxFuture<'a, AppResult<bool>>;
}

impl Evictor for TorrentEngine {
    fn in_use(&self) -> HashSet<String> {
        TorrentEngine::in_use(self)
    }

    fn evict<'a>(&'a self, name: &'a str, path: &'a Path) -> BoxFuture<'a, AppResult<bool>> {
        Box::pin(TorrentEngine::evict(self, name, path))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub last_access: SystemTime,
}

/// Bytes allocated on disk by a file or a folder (recursive, symlinks not followed).
pub fn allocated_size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    let own = allocated(&meta);
    if !meta.is_dir() {
        return own;
    }
    let children = std::fs::read_dir(path)
        .map(|rd| rd.flatten().map(|e| allocated_size(&e.path())).sum::<u64>())
        .unwrap_or(0);
    own + children
}

#[cfg(unix)]
fn allocated(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(not(unix))]
fn allocated(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// Free space for the user on the filesystem that holds `path`.
#[cfg(unix)]
pub fn free_disk_bytes(path: &Path) -> u64 {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) else {
        return 0;
    };
    // SAFETY: `statvfs` only writes into the zeroed struct we pass; `c_path` is a valid
    // NUL-terminated string that outlives the call.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return 0;
    }
    // The field types differ between platforms (u32 on some 32-bit targets).
    #[allow(clippy::useless_conversion)]
    let (avail, frsize) = (u64::from(st.f_bavail), u64::from(st.f_frsize));
    avail.saturating_mul(frsize)
}

#[cfg(not(unix))]
pub fn free_disk_bytes(_path: &Path) -> u64 {
    0
}

/// Entries of `cache_dir` (except `img/`) and the bytes of `img/`.
pub fn scan(cache_dir: &Path) -> std::io::Result<(Vec<CacheEntry>, u64)> {
    let mut entries = Vec::new();
    let mut img_bytes = 0;
    let read = match std::fs::read_dir(cache_dir) {
        Ok(read) => read,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((entries, 0)),
        Err(e) => return Err(e),
    };
    for item in read.flatten() {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        if name == IMG_DIR {
            img_bytes = allocated_size(&path);
            continue;
        }
        let last_access = std::fs::symlink_metadata(&path)
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        entries.push(CacheEntry {
            bytes: allocated_size(&path),
            name,
            path,
            last_access,
        });
    }
    Ok((entries, img_bytes))
}

/// Least recently used entries to delete so that `total` drops to `limit` or below,
/// skipping `protected` ones. Oldest first.
pub fn plan_eviction<'a>(
    entries: &'a [CacheEntry],
    total: u64,
    limit: u64,
    protected: &HashSet<String>,
) -> Vec<&'a CacheEntry> {
    let mut candidates: Vec<&CacheEntry> = entries
        .iter()
        .filter(|e| !protected.contains(&e.name))
        .collect();
    candidates.sort_by_key(|e| e.last_access);
    let mut remaining = total;
    let mut plan = Vec::new();
    for entry in candidates {
        if remaining <= limit {
            break;
        }
        remaining = remaining.saturating_sub(entry.bytes);
        plan.push(entry);
    }
    plan
}

pub struct CacheManager {
    cache_dir: PathBuf,
    library_dir: PathBuf,
    evictor: Arc<dyn Evictor>,
    limit: AtomicU64,
    /// One cleanup at a time.
    lock: tokio::sync::Mutex<()>,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> AppResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::Internal(format!("cache task: {e}")))
}

impl CacheManager {
    pub fn new(
        cache_dir: PathBuf,
        library_dir: PathBuf,
        evictor: Arc<dyn Evictor>,
        limit_bytes: u64,
    ) -> Self {
        Self {
            cache_dir,
            library_dir,
            evictor,
            limit: AtomicU64::new(limit_bytes),
            lock: tokio::sync::Mutex::new(()),
        }
    }

    pub fn limit_bytes(&self) -> u64 {
        self.limit.load(Ordering::Relaxed)
    }

    pub fn set_limit_bytes(&self, bytes: u64) {
        self.limit.store(bytes, Ordering::Relaxed);
    }

    async fn scan(&self) -> AppResult<(Vec<CacheEntry>, u64)> {
        let dir = self.cache_dir.clone();
        Ok(blocking(move || scan(&dir)).await??)
    }

    /// Deletes `plan` (skipping what became in use meanwhile). Returns the bytes freed.
    async fn evict_all(&self, plan: &[&CacheEntry]) -> u64 {
        let mut freed = 0;
        for entry in plan {
            match self.evictor.evict(&entry.name, &entry.path).await {
                Ok(true) => {
                    tracing::info!(name = %entry.name, bytes = entry.bytes, "cache entry evicted");
                    freed += entry.bytes;
                }
                Ok(false) => tracing::debug!(name = %entry.name, "cache entry in use, kept"),
                Err(e) => tracing::warn!(name = %entry.name, error = %e, "could not evict"),
            }
        }
        freed
    }

    /// Evicts least recently used entries until the cache fits the limit.
    pub async fn enforce_limit(&self) -> AppResult<u64> {
        let _guard = self.lock.lock().await;
        let (entries, img_bytes) = self.scan().await?;
        let total = img_bytes + entries.iter().map(|e| e.bytes).sum::<u64>();
        let limit = self.limit_bytes();
        if total <= limit {
            return Ok(0);
        }
        let protected = self.evictor.in_use();
        let plan = plan_eviction(&entries, total, limit, &protected);
        let freed = self.evict_all(&plan).await;
        let left = total.saturating_sub(freed);
        if left > limit {
            tracing::warn!(left, limit, "cache still over the limit (entries in use)");
        } else {
            tracing::info!(freed, left, limit, "cache limit enforced");
        }
        Ok(freed)
    }

    /// Deletes every entry except the ones in use and `img/`.
    pub async fn clear(&self) -> AppResult<ClearCacheResult> {
        let _guard = self.lock.lock().await;
        let (entries, _) = self.scan().await?;
        let protected = self.evictor.in_use();
        let plan: Vec<&CacheEntry> = entries
            .iter()
            .filter(|e| !protected.contains(&e.name))
            .collect();
        let freed_bytes = self.evict_all(&plan).await;
        tracing::info!(freed_bytes, "cache cleared");
        Ok(ClearCacheResult { freed_bytes })
    }

    pub async fn usage(&self) -> AppResult<StorageUsage> {
        let (cache, library) = (self.cache_dir.clone(), self.library_dir.clone());
        let (cache_bytes, library_bytes, free_disk_bytes) = blocking(move || {
            let free_on = if cache.exists() { &cache } else { &library };
            (
                allocated_size(&cache),
                allocated_size(&library),
                free_disk_bytes(free_on),
            )
        })
        .await?;
        Ok(StorageUsage {
            cache_bytes,
            cache_limit_bytes: self.limit_bytes(),
            library_bytes,
            free_disk_bytes,
        })
    }

    /// Enforces the limit now and then every `interval`, while the manager is alive.
    pub fn spawn_periodic(self: &Arc<Self>, interval: Duration) {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let Some(manager) = weak.upgrade() else {
                    break;
                };
                if let Err(e) = manager.enforce_limit().await {
                    tracing::warn!(error = %e, "cache cleanup failed");
                }
            }
        });
    }

    /// Enforces the limit in the background (after opening a stream, changing the limit).
    pub fn enforce_soon(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            if let Err(e) = manager.enforce_limit().await {
                tracing::warn!(error = %e, "cache cleanup failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const KB: u64 = 1024;

    /// Removes entries like the engine would, except the ones marked in use.
    #[derive(Default)]
    struct FakeEvictor {
        in_use: Mutex<HashSet<String>>,
        evicted: Mutex<Vec<String>>,
    }

    impl Evictor for FakeEvictor {
        fn in_use(&self) -> HashSet<String> {
            self.in_use.lock().unwrap().clone()
        }

        fn evict<'a>(&'a self, name: &'a str, path: &'a Path) -> BoxFuture<'a, AppResult<bool>> {
            Box::pin(async move {
                if self.in_use.lock().unwrap().contains(name) {
                    return Ok(false);
                }
                if path.is_dir() {
                    std::fs::remove_dir_all(path)?;
                } else {
                    std::fs::remove_file(path)?;
                }
                self.evicted.lock().unwrap().push(name.to_owned());
                Ok(true)
            })
        }
    }

    fn write(path: &Path, bytes: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; bytes as usize]).unwrap();
    }

    /// `cache/<name>/movie.mp4` with `bytes` of real data, accessed `age_s` seconds ago.
    fn entry(cache: &Path, name: &str, bytes: u64, age_s: u64) {
        let dir = cache.join(name);
        write(&dir.join("movie.mp4"), bytes);
        let t = SystemTime::now() - Duration::from_secs(age_s);
        std::fs::File::open(&dir).unwrap().set_modified(t).unwrap();
    }

    struct Setup {
        _tmp: tempfile::TempDir,
        cache: PathBuf,
        library: PathBuf,
        evictor: Arc<FakeEvictor>,
        manager: CacheManager,
    }

    fn setup(limit: u64) -> Setup {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("cache");
        let library = tmp.path().join("library");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        let evictor = Arc::new(FakeEvictor::default());
        let manager = CacheManager::new(
            cache.clone(),
            library.clone(),
            Arc::clone(&evictor) as Arc<dyn Evictor>,
            limit,
        );
        Setup {
            _tmp: tmp,
            cache,
            library,
            evictor,
            manager,
        }
    }

    fn names(cache: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(cache)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn allocated_size_ignores_sparse_holes() {
        let tmp = tempfile::tempdir().unwrap();
        let sparse = tmp.path().join("sparse.mp4");
        // Like librqbit: full length up front, only a little data written.
        let f = std::fs::File::create(&sparse).unwrap();
        f.set_len(512 * 1024 * 1024).unwrap();
        drop(f);
        assert!(allocated_size(&sparse) < 1024 * 1024);
        write(&tmp.path().join("dir/real.bin"), 64 * KB);
        let dir = allocated_size(&tmp.path().join("dir"));
        assert!(dir >= 64 * KB, "{dir}");
        assert_eq!(allocated_size(&tmp.path().join("missing")), 0);
    }

    #[test]
    fn plan_evicts_oldest_first_until_under_the_limit() {
        let t = |s| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        let e = |name: &str, bytes, at| CacheEntry {
            name: name.into(),
            path: PathBuf::from(name),
            bytes,
            last_access: t(at),
        };
        let entries = vec![e("new", 300, 30), e("old", 300, 10), e("mid", 300, 20)];
        let none = HashSet::new();
        let names =
            |plan: Vec<&CacheEntry>| plan.iter().map(|e| e.name.clone()).collect::<Vec<_>>();

        assert!(plan_eviction(&entries, 900, 900, &none).is_empty());
        assert_eq!(names(plan_eviction(&entries, 900, 700, &none)), ["old"]);
        assert_eq!(
            names(plan_eviction(&entries, 900, 300, &none)),
            ["old", "mid"]
        );
        assert_eq!(
            names(plan_eviction(&entries, 900, 0, &none)),
            ["old", "mid", "new"]
        );
        // A protected entry is skipped even if it is the oldest.
        let protected: HashSet<String> = ["old".to_owned()].into();
        assert_eq!(
            names(plan_eviction(&entries, 900, 300, &protected)),
            ["mid", "new"]
        );
    }

    #[tokio::test]
    async fn enforce_respects_limit_streams_in_use_img_and_library() {
        let s = setup(250 * KB);
        entry(&s.cache, "aaaa", 100 * KB, 300); // oldest, but streaming
        entry(&s.cache, "bbbb", 100 * KB, 200);
        entry(&s.cache, "cccc", 100 * KB, 100);
        entry(&s.cache, "dddd", 100 * KB, 10);
        write(&s.cache.join(IMG_DIR).join("cover"), 20 * KB);
        write(&s.library.join("Saved Movie/movie.mp4"), 500 * KB);
        s.evictor.in_use.lock().unwrap().insert("aaaa".into());

        let freed = s.manager.enforce_limit().await.unwrap();
        // 420 KB → evict bbbb and cccc (LRU, skipping aaaa) → ~220 KB.
        assert_eq!(*s.evictor.evicted.lock().unwrap(), ["bbbb", "cccc"]);
        assert!(freed >= 200 * KB, "{freed}");
        assert_eq!(names(&s.cache), ["aaaa", "dddd", IMG_DIR]);
        assert!(s.library.join("Saved Movie/movie.mp4").exists());

        // Under the limit: nothing else happens.
        assert_eq!(s.manager.enforce_limit().await.unwrap(), 0);

        // Lowering the limit can't touch the stream in use.
        s.manager.set_limit_bytes(0);
        s.manager.enforce_limit().await.unwrap();
        assert_eq!(names(&s.cache), ["aaaa", IMG_DIR]);
        assert!(s.library.join("Saved Movie/movie.mp4").exists());
    }

    #[tokio::test]
    async fn clear_keeps_streams_in_use_and_img() {
        let s = setup(10 * 1024 * KB);
        entry(&s.cache, "aaaa", 100 * KB, 10);
        entry(&s.cache, "bbbb", 100 * KB, 10);
        write(&s.cache.join("Old Layout Movie.mp4"), 50 * KB); // loose file (phase 3 layout)
        write(&s.cache.join(IMG_DIR).join("cover"), 20 * KB);
        write(&s.library.join("x/movie.mp4"), 100 * KB);
        s.evictor.in_use.lock().unwrap().insert("aaaa".into());

        let before = s.manager.usage().await.unwrap();
        let ClearCacheResult { freed_bytes } = s.manager.clear().await.unwrap();
        assert_eq!(names(&s.cache), ["aaaa", IMG_DIR]);
        assert!(freed_bytes >= 150 * KB, "{freed_bytes}");
        let after = s.manager.usage().await.unwrap();
        assert_eq!(after.cache_bytes, before.cache_bytes - freed_bytes);
        assert_eq!(after.library_bytes, before.library_bytes);
        assert!(s.library.join("x/movie.mp4").exists());
    }

    #[tokio::test]
    async fn usage_reports_cache_library_limit_and_free_space() {
        let s = setup(5 * 1024 * KB);
        entry(&s.cache, "aaaa", 100 * KB, 10);
        write(&s.library.join("x/movie.mp4"), 200 * KB);
        let u = s.manager.usage().await.unwrap();
        assert!(
            u.cache_bytes >= 100 * KB && u.cache_bytes < 200 * KB,
            "{u:?}"
        );
        assert!(
            u.library_bytes >= 200 * KB && u.library_bytes < 300 * KB,
            "{u:?}"
        );
        assert_eq!(u.cache_limit_bytes, 5 * 1024 * KB);
        assert!(u.free_disk_bytes > 0);
    }
}
