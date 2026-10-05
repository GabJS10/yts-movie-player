//! What differs between operating systems: free space, write access, cross-disk moves,
//! files locked by another process, path comparison and Windows-only lookups (registry,
//! `Program Files`). Linux keeps the behaviour of v1.0; most of this is for Windows.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Waits between attempts when a file is locked by another process (Windows: antivirus,
/// indexer, a player that still has it open). About 4 s in total.
pub const LOCKED_RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(100),
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_millis(1000),
    Duration::from_millis(2000),
];

/// Free space for the user on the filesystem that holds `path` (0 if unknown).
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

/// Free space for the user (quotas included) on the volume that holds `path`.
#[cfg(windows)]
pub fn free_disk_bytes(path: &Path) -> u64 {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide = win::wide(path.as_os_str());
    let mut avail: u64 = 0;
    // SAFETY: `wide` is NUL-terminated and outlives the call; the other two outputs are
    // optional and passed as null.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut avail,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        0
    } else {
        avail
    }
}

#[cfg(not(any(unix, windows)))]
pub fn free_disk_bytes(_path: &Path) -> u64 {
    0
}

/// This process can create files in the folder `path` (which exists).
#[cfg(unix)]
pub fn dir_writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the call.
    unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
}

/// Windows has no `access(W_OK)` that tells the truth about ACLs or read-only media, so a
/// hidden temporary file is created and deleted when closed (it never reaches the disk).
#[cfg(windows)]
pub fn dir_writable(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_TEMPORARY, FILE_FLAG_DELETE_ON_CLOSE,
    };
    static N: AtomicU64 = AtomicU64::new(0);
    let probe = path.join(format!(
        ".yts-player-probe-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .attributes(FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_TEMPORARY)
        .custom_flags(FILE_FLAG_DELETE_ON_CLOSE)
        .open(probe)
        .is_ok()
}

#[cfg(not(any(unix, windows)))]
pub fn dir_writable(_path: &Path) -> bool {
    true
}

/// Bytes allocated on disk by the file `path` (`meta` is its metadata): blocks on Unix
/// (sparse holes don't count), `GetCompressedFileSizeW` on Windows (sparse and compressed
/// files), the length elsewhere.
#[cfg(unix)]
pub fn allocated_bytes(_path: &Path, meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks() * 512
}

#[cfg(windows)]
pub fn allocated_bytes(path: &Path, meta: &std::fs::Metadata) -> u64 {
    use windows_sys::Win32::Storage::FileSystem::{GetCompressedFileSizeW, INVALID_FILE_SIZE};
    if meta.is_dir() {
        return 0;
    }
    let wide = win::wide(path.as_os_str());
    let mut high: u32 = 0;
    // SAFETY: `wide` is NUL-terminated and outlives the call; `high` receives the upper
    // 32 bits.
    let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
    // INVALID_FILE_SIZE is also a valid low half; only an error if GetLastError says so.
    if low == INVALID_FILE_SIZE && io::Error::last_os_error().raw_os_error() != Some(0) {
        return meta.len();
    }
    (u64::from(high) << 32) | u64::from(low)
}

#[cfg(not(any(unix, windows)))]
pub fn allocated_bytes(_path: &Path, meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

/// Sets the modification time of the folder `dir` to now (the cache LRU uses it as the
/// last access). On Windows a folder only opens with `FILE_FLAG_BACKUP_SEMANTICS`.
pub fn touch_dir(dir: &Path) -> io::Result<()> {
    set_dir_modified(dir, std::time::SystemTime::now())
}

pub fn set_dir_modified(dir: &Path, time: std::time::SystemTime) -> io::Result<()> {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_WRITE_ATTRIBUTES,
        };
        std::fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(dir)?
    };
    #[cfg(not(windows))]
    let file = std::fs::File::open(dir)?;
    file.set_modified(time)
}

/// A rename failed because source and destination are on different disks (`EXDEV` on
/// Unix, `ERROR_NOT_SAME_DEVICE` on Windows): copy instead.
pub fn is_cross_device(e: &io::Error) -> bool {
    is_cross_device_code(e.raw_os_error())
}

fn is_cross_device_code(code: Option<i32>) -> bool {
    #[cfg(unix)]
    let expected = libc::EXDEV;
    #[cfg(windows)]
    let expected = win::ERROR_NOT_SAME_DEVICE;
    #[cfg(not(any(unix, windows)))]
    let expected = -1;
    code == Some(expected)
}

/// Windows only: the file is in use by another process (sharing or lock violation, or
/// access denied while a delete is pending), which usually goes away by itself. Always
/// `false` elsewhere, where open files can be deleted and renamed.
pub fn is_locked_error(e: &io::Error) -> bool {
    cfg!(windows) && is_locked_code(e.raw_os_error())
}

fn is_locked_code(code: Option<i32>) -> bool {
    matches!(
        code,
        Some(win::ERROR_ACCESS_DENIED | win::ERROR_SHARING_VIOLATION | win::ERROR_LOCK_VIOLATION)
    )
}

/// Runs `op` again after each of `delays` while it fails with an error `retryable` accepts.
pub async fn retry_io<T, F, Fut>(
    mut op: F,
    retryable: impl Fn(&io::Error) -> bool,
    delays: &[Duration],
) -> io::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = io::Result<T>>,
{
    let mut delays = delays.iter();
    loop {
        match op().await {
            Err(e) if retryable(&e) => match delays.next() {
                Some(delay) => {
                    tracing::debug!(error = %e, ?delay, "file in use, trying again");
                    tokio::time::sleep(*delay).await;
                }
                None => return Err(e),
            },
            other => return other,
        }
    }
}

/// [`retry_io`] for files locked by another process (only happens on Windows).
pub async fn retry_locked<T, F, Fut>(op: F) -> io::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = io::Result<T>>,
{
    retry_io(op, is_locked_error, &LOCKED_RETRY_DELAYS).await
}

/// Form of a path for comparisons: as is on Unix, lowercase on Windows (whose
/// filesystems ignore case: `D:\Pelis` and `d:\pelis` are the same folder).
pub fn path_key(path: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

/// One folder is the other or is inside it (by components, ignoring case on Windows).
pub fn nested(a: &Path, b: &Path) -> bool {
    let (a, b) = (path_key(a), path_key(b));
    a.starts_with(&b) || b.starts_with(&a)
}

/// Windows error codes (`winerror.h`), also used by the tests on every platform.
mod win {
    pub const ERROR_ACCESS_DENIED: i32 = 5;
    #[cfg_attr(not(windows), allow(dead_code))]
    pub const ERROR_NOT_SAME_DEVICE: i32 = 17;
    pub const ERROR_SHARING_VIOLATION: i32 = 32;
    pub const ERROR_LOCK_VIOLATION: i32 = 33;

    /// NUL-terminated UTF-16 for the `…W` functions.
    #[cfg(windows)]
    pub fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        s.encode_wide().chain(std::iter::once(0)).collect()
    }
}

/// Install folders of a program in the registry (`HKLM\SOFTWARE\…`, 64- and 32-bit
/// views): the `InstallDir` value, else the folder of the default value (the `.exe`).
#[cfg(windows)]
pub fn registry_install_dirs(subkey: &str) -> Vec<PathBuf> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY,
        RRF_SUBKEY_WOW6464KEY,
    };

    fn read(subkey: &[u16], value: Option<&[u16]>, view: u32) -> Option<PathBuf> {
        let value_ptr = value.map_or(std::ptr::null(), |v| v.as_ptr());
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: the key and value names are NUL-terminated and outlive the call; `len`
        // is the size in bytes of `buf`, which the call fills (NUL included).
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                subkey.as_ptr(),
                value_ptr,
                RRF_RT_REG_SZ | view,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        if status != 0 {
            return None;
        }
        let chars = (len as usize / 2).min(buf.len());
        let s = &buf[..chars];
        let s = s.split(|&c| c == 0).next().unwrap_or_default();
        let path = PathBuf::from(std::ffi::OsString::from_wide(s));
        (!path.as_os_str().is_empty()).then_some(path)
    }

    let key = win::wide(OsStr::new(subkey));
    let install_dir = win::wide(OsStr::new("InstallDir"));
    let mut dirs = Vec::new();
    for view in [RRF_SUBKEY_WOW6464KEY, RRF_SUBKEY_WOW6432KEY] {
        let dir = read(&key, Some(&install_dir), view)
            .or_else(|| read(&key, None, view).and_then(|exe| exe.parent().map(Path::to_path_buf)));
        if let Some(dir) = dir {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

#[cfg(not(windows))]
pub fn registry_install_dirs(_subkey: &str) -> Vec<PathBuf> {
    Vec::new()
}

/// Windows: process creation flag that keeps a console program from opening a window.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const QUICK: [Duration; 5] = [Duration::from_millis(1); 5];

    #[test]
    fn free_space_of_a_real_folder() {
        let tmp = tempfile::tempdir().unwrap();
        if cfg!(any(unix, windows)) {
            assert!(free_disk_bytes(tmp.path()) > 0);
        }
        assert_eq!(free_disk_bytes(&tmp.path().join("missing/deeper")), 0);
    }

    #[test]
    fn folder_mtime_can_be_set() {
        let tmp = tempfile::tempdir().unwrap();
        let t = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        set_dir_modified(tmp.path(), t).unwrap();
        assert_eq!(tmp.path().metadata().unwrap().modified().unwrap(), t);
        touch_dir(tmp.path()).unwrap();
        assert!(tmp.path().metadata().unwrap().modified().unwrap() > t);
    }

    #[test]
    fn writable_folder() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(dir_writable(tmp.path()));
        // The probe leaves nothing behind.
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
        assert!(!dir_writable(&tmp.path().join("missing")));
    }

    #[test]
    fn error_codes() {
        #[cfg(unix)]
        assert!(is_cross_device_code(Some(libc::EXDEV)));
        #[cfg(windows)]
        assert!(is_cross_device_code(Some(17)));
        assert!(!is_cross_device_code(Some(2)));
        assert!(!is_cross_device_code(None));

        assert!(is_locked_code(Some(32)));
        assert!(is_locked_code(Some(33)));
        assert!(is_locked_code(Some(5)));
        assert!(!is_locked_code(Some(2)));
        assert!(!is_locked_code(None));
        let sharing = io::Error::from_raw_os_error(32);
        assert_eq!(is_locked_error(&sharing), cfg!(windows));
        assert!(!is_locked_error(&io::Error::other("x")));
    }

    #[tokio::test]
    async fn retries_until_the_file_is_released() {
        let calls = AtomicU32::new(0);
        let locked = || io::Error::from_raw_os_error(32);
        let result = retry_io(
            || {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n < 2 {
                        Err(locked())
                    } else {
                        Ok(n)
                    }
                }
            },
            |e| is_locked_code(e.raw_os_error()),
            &QUICK,
        )
        .await;
        assert_eq!(result.unwrap(), 2);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn gives_up_after_the_last_delay_and_never_retries_other_errors() {
        let calls = AtomicU32::new(0);
        let result: io::Result<()> = retry_io(
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err(io::Error::from_raw_os_error(32)) }
            },
            |e| is_locked_code(e.raw_os_error()),
            &QUICK,
        )
        .await;
        assert_eq!(result.unwrap_err().raw_os_error(), Some(32));
        assert_eq!(calls.load(Ordering::SeqCst), 6);

        calls.store(0, Ordering::SeqCst);
        let result: io::Result<()> = retry_io(
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err(io::Error::from(io::ErrorKind::NotFound)) }
            },
            |e| is_locked_code(e.raw_os_error()),
            &QUICK,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn nesting_ignores_case_only_on_windows() {
        assert!(nested(Path::new("/a/b"), Path::new("/a")));
        assert!(nested(Path::new("/a"), Path::new("/a/b/c")));
        assert!(!nested(Path::new("/a/bc"), Path::new("/a/b")));
        assert_eq!(
            nested(Path::new("/Pelis/YTS"), Path::new("/pelis")),
            cfg!(windows)
        );
    }

    #[test]
    fn registry_is_empty_off_windows() {
        if !cfg!(windows) {
            assert!(registry_install_dirs(r"SOFTWARE\VideoLAN\VLC").is_empty());
        }
    }
}
