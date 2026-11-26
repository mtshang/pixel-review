//! One local, owner-accessible named pipe per canvas. The kernel releases its
//! name when the worker exits, including on a crash.
use super::Result;
use std::fs::{File, OpenOptions};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::ptr;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    LocalFree, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::{
    FlushFileBuffers, FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW,
    PIPE_REJECT_REMOTE_CLIENTS,
};

fn address(name: &str) -> Result<String> {
    if name.trim().is_empty() || name.len() > 240 || name.chars().any(char::is_control) {
        return Err("invalid canvas name (1..240 UTF-8 bytes)".into());
    }
    let mut hash = DefaultHasher::new();
    std::env::var_os("LOCALAPPDATA")
        .ok_or("missing LOCALAPPDATA")?
        .hash(&mut hash);
    name.hash(&mut hash);
    Ok(format!(
        r"\\.\pipe\pixel-review.canvas.v1.{:016x}",
        hash.finish()
    ))
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub fn bind(name: &str) -> Result<File> {
    let path = wide(&address(name)?);
    let sddl = wide("D:P(A;;GA;;;OW)");
    let mut descriptor = ptr::null_mut();
    // The descriptor lives through CreateNamedPipeW; its returned handle is
    // uniquely owned by File. Only local clients with owner rights may connect.
    unsafe {
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            ptr::null_mut(),
        ) == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let handle = CreateNamedPipeW(
            path.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_REJECT_REMOTE_CLIENTS,
            1,
            65536,
            65536,
            5000,
            &attrs,
        );
        let error = io::Error::last_os_error();
        LocalFree(descriptor);
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!("cannot open canvas {name} (name may be in use): {error}").into());
        }
        Ok(File::from_raw_handle(handle))
    }
}

pub fn connect(name: &str) -> Result<File> {
    let path = address(name)?;
    let wide_path = wide(&path);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION)
            .open(&path)
        {
            Ok(file) => return Ok(file),
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
                    && Instant::now() < deadline =>
            unsafe {
                WaitNamedPipeW(wide_path.as_ptr(), 500);
            },
            Err(e) => return Err(format!("cannot reach canvas {name}: {e}").into()),
        }
    }
}

pub fn accept(file: &File) -> Result<()> {
    loop {
        if unsafe { ConnectNamedPipe(file.as_raw_handle(), ptr::null_mut()) } != 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(code) if code == ERROR_PIPE_CONNECTED as i32 => return Ok(()),
            Some(code) if code == ERROR_NO_DATA as i32 => unsafe {
                DisconnectNamedPipe(file.as_raw_handle());
            },
            _ => return Err(e.into()),
        }
    }
}

pub fn disconnect(file: &File) {
    unsafe {
        FlushFileBuffers(file.as_raw_handle());
        DisconnectNamedPipe(file.as_raw_handle());
    }
}

pub fn receive(file: &mut File) -> Result<Vec<u8>> {
    let mut length = [0; 4];
    file.read_exact(&mut length)?;
    let length = u32::from_le_bytes(length) as usize;
    if length > 1024 * 1024 {
        return Err("canvas message exceeds 1 MiB".into());
    }
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub fn send(file: &mut File, bytes: &[u8]) -> Result<()> {
    file.write_all(&u32::try_from(bytes.len())?.to_le_bytes())?;
    file.write_all(bytes)?;
    Ok(())
}
