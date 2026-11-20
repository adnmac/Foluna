//! Disques amovibles du système : liste, place libre, éjection. Windows : lettres de lecteur amovibles ;
//! Linux : points de montage sous `/media`, `/run/media`, `/mnt`.

use std::path::{Path, PathBuf};

/// Disque monté qui peut être une liseuse.
#[derive(Debug, Clone, PartialEq)]
pub struct Volume {
    pub root: PathBuf,
    /// Nom du volume (« Kindle », « KOBOeReader »).
    pub label: String,
    /// Numéro de série du volume (FAT), stable tant que l'appareil n'est pas réinitialisé.
    pub serial: Option<String>,
    pub total_bytes: Option<u64>,
}

impl Volume {
    /// Volume d'un dossier quelconque (liseuse simulée en test, ou disque déjà connu).
    pub fn at(root: &Path) -> Volume {
        Volume {
            root: root.to_path_buf(),
            label: root.file_name().and_then(|n| n.to_str()).unwrap_or("Liseuse").to_string(),
            serial: None,
            total_bytes: None,
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::Volume;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW, FILE_FLAGS_AND_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, IOCTL_STORAGE_EJECT_MEDIA};

    /// `GetDriveTypeW` : support amovible (clé USB, liseuse en mode disque).
    const DRIVE_REMOVABLE: u32 = 2;
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain([0]).collect()
    }

    pub fn removable_volumes() -> Vec<Volume> {
        let mask = unsafe { GetLogicalDrives() };
        (0..26u8)
            .filter(|i| mask & (1 << i) != 0)
            .filter_map(|i| {
                let root = format!("{}:\\", (b'A' + i) as char);
                let root_w = wide(&root);
                if unsafe { GetDriveTypeW(PCWSTR(root_w.as_ptr())) } != DRIVE_REMOVABLE {
                    return None;
                }
                let mut name = [0u16; 261];
                let mut serial = 0u32;
                let ok = unsafe {
                    GetVolumeInformationW(PCWSTR(root_w.as_ptr()), Some(&mut name), Some(&mut serial), None, None, None)
                };
                // Lecteur sans support (lecteur de cartes vide) : ignoré.
                ok.ok()?;
                let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
                let mut total = 0u64;
                let total = unsafe { GetDiskFreeSpaceExW(PCWSTR(root_w.as_ptr()), None, Some(&mut total), None) }
                    .ok()
                    .map(|_| total);
                Some(Volume {
                    root: PathBuf::from(&root),
                    label: String::from_utf16_lossy(&name[..len]),
                    serial: Some(format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)),
                    total_bytes: total,
                })
            })
            .collect()
    }

    pub fn free_space(root: &Path) -> Option<u64> {
        let w = wide(&root.to_string_lossy());
        let mut free = 0u64;
        unsafe { GetDiskFreeSpaceExW(PCWSTR(w.as_ptr()), Some(&mut free), None, None) }.ok().map(|_| free)
    }

    /// Verrouille, démonte et éjecte le volume (comme « Éjecter » dans l'Explorateur). Échoue si un fichier de
    /// l'appareil est encore ouvert.
    pub fn eject(root: &Path) -> Result<(), String> {
        let letter = root.to_string_lossy().chars().next().filter(char::is_ascii_alphabetic).ok_or("lecteur inconnu")?;
        let device = wide(&format!("\\\\.\\{letter}:"));
        let handle: HANDLE = unsafe {
            CreateFileW(
                PCWSTR(device.as_ptr()),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map_err(|e| e.message())?;
        let control = |code: u32| -> Result<(), String> {
            let mut returned = 0u32;
            unsafe { DeviceIoControl(handle, code, None, 0, None, 0, Some(&mut returned), None) }.map_err(|e| e.message())
        };
        // Le verrou peut échouer un instant si l'Explorateur lit le disque : quelques essais.
        let mut locked = Err(String::new());
        for _ in 0..10 {
            locked = control(FSCTL_LOCK_VOLUME);
            if locked.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        let result = locked
            .map_err(|_| "un fichier de la liseuse est encore ouvert".to_string())
            .and_then(|_| control(FSCTL_DISMOUNT_VOLUME))
            .and_then(|_| control(IOCTL_STORAGE_EJECT_MEDIA));
        unsafe {
            let _ = CloseHandle(handle);
        }
        result
    }
}

#[cfg(unix)]
mod imp {
    use super::Volume;
    use std::path::{Path, PathBuf};

    /// Points de montage des disques amovibles (montés par le bureau sous /media, /run/media ou /mnt).
    pub fn removable_volumes() -> Vec<Volume> {
        let Ok(mounts) = std::fs::read_to_string("/proc/mounts") else { return Vec::new() };
        mounts
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let (device, mount) = (parts.next()?, parts.next()?);
                let mount = mount.replace("\\040", " ");
                let removable = ["/media/", "/run/media/", "/mnt/"].iter().any(|p| mount.starts_with(p));
                if !removable || !device.starts_with("/dev/") {
                    return None;
                }
                let root = PathBuf::from(&mount);
                let serial = std::fs::read_dir("/dev/disk/by-uuid").ok().and_then(|entries| {
                    entries.flatten().find_map(|e| {
                        let target = std::fs::canonicalize(e.path()).ok()?;
                        (target == Path::new(device)).then(|| e.file_name().to_string_lossy().into_owned())
                    })
                });
                Some(Volume {
                    label: root.file_name().and_then(|n| n.to_str()).unwrap_or("Liseuse").to_string(),
                    total_bytes: stat(&root).map(|s| s.0),
                    root,
                    serial,
                })
            })
            .collect()
    }

    /// (taille totale, place libre) en octets.
    fn stat(root: &Path) -> Option<(u64, u64)> {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(root.as_os_str().as_bytes()).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut s) } != 0 {
            return None;
        }
        let block = s.f_frsize as u64;
        Some((s.f_blocks as u64 * block, s.f_bavail as u64 * block))
    }

    pub fn free_space(root: &Path) -> Option<u64> {
        stat(root).map(|s| s.1)
    }

    /// Démonte puis met hors tension le disque (udisks, comme le bureau), sinon `umount`.
    pub fn eject(root: &Path) -> Result<(), String> {
        let mounts = std::fs::read_to_string("/proc/mounts").map_err(|e| e.to_string())?;
        let device = mounts
            .lines()
            .find_map(|l| {
                let mut p = l.split_whitespace();
                let (d, m) = (p.next()?, p.next()?);
                (Path::new(&m.replace("\\040", " ")) == root).then(|| d.to_string())
            })
            .ok_or("disque introuvable")?;
        let run = |args: &[&str]| std::process::Command::new(args[0]).args(&args[1..]).output();
        match run(&["udisksctl", "unmount", "-b", &device]) {
            Ok(out) if out.status.success() => {
                let _ = run(&["udisksctl", "power-off", "-b", &device]);
                Ok(())
            }
            Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
            Err(_) => match run(&["umount", &root.to_string_lossy()]) {
                Ok(out) if out.status.success() => Ok(()),
                Ok(out) => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
                Err(e) => Err(e.to_string()),
            },
        }
    }
}

pub use imp::{eject, free_space, removable_volumes};
