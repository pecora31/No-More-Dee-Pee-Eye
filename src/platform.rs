use std::{
    ffi::OsStr,
    fs::File,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{JobObjects::*, Pipes::*, Registry::*, Threading::*},
    UI::{Shell::*, WindowsAndMessaging::*},
};

pub fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}
fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}
pub fn replace_file(from: &Path, to: &Path) -> Result<(), String> {
    if unsafe {
        MoveFileExW(
            wide(from).as_ptr(),
            wide(to).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(last_error())
    } else {
        Ok(())
    }
}

pub struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
// The handle is exclusively owned; Win32 kernel handles can move between threads.
unsafe impl Send for Handle {}

pub struct Connection {
    pub reader: File,
    pub writer: File,
}

pub fn connect_helper() -> Result<Connection, String> {
    connect_helper_mode(true)
}
pub fn connect_helper_mode(elevate: bool) -> Result<Connection, String> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let nonce: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    unsafe {
        let mut descriptor = null_mut();
        let sddl = wide("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)");
        if ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ) == 0
        {
            return Err(last_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let create = |suffix| {
            let name_w = wide(format!(r"\\.\pipe\Umbra-{nonce}-{suffix}"));
            let pipe = CreateNamedPipeW(
                name_w.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                65_536,
                65_536,
                0,
                &attributes,
            );
            if pipe == INVALID_HANDLE_VALUE {
                Err(last_error())
            } else {
                Ok(File::from_raw_handle(pipe))
            }
        };
        let events = create("events");
        let commands = create("commands");
        LocalFree(descriptor);
        let events = events?;
        let commands = commands?;
        let exe = wide(std::env::current_exe().map_err(|e| e.to_string())?);
        let verb = wide(if elevate { "runas" } else { "open" });
        let parameters = wide(format!("--engine-worker {nonce} {}", std::process::id()));
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
        info.lpVerb = verb.as_ptr();
        info.lpFile = exe.as_ptr();
        info.lpParameters = parameters.as_ptr();
        info.nShow = SW_HIDE;
        if ShellExecuteExW(&mut info) == 0 {
            return Err(if GetLastError() == ERROR_CANCELLED {
                "Đã hủy yêu cầu quyền quản trị. Lõi chưa được bật.".into()
            } else {
                last_error()
            });
        }
        let process = Handle(info.hProcess);
        let expected_pid = GetProcessId(process.0);
        let deadline = Instant::now() + Duration::from_secs(30);
        for file in [&events, &commands] {
            let pipe = file.as_raw_handle();
            loop {
                let connected = ConnectNamedPipe(pipe, null_mut()) != 0
                    || GetLastError() == ERROR_PIPE_CONNECTED;
                if connected {
                    let mut pid = 0;
                    if GetNamedPipeClientProcessId(pipe, &mut pid) == 0 || pid != expected_pid {
                        return Err("Không xác thực được tiến trình quản lý lõi.".into());
                    }
                    let mode = PIPE_READMODE_BYTE | PIPE_WAIT;
                    if SetNamedPipeHandleState(pipe, &mode, null(), null()) == 0 {
                        return Err(last_error());
                    }
                    break;
                }
                if WaitForSingleObject(process.0, 0) == WAIT_OBJECT_0 {
                    return Err("Tiến trình quản lý lõi đã thoát trước khi kết nối.".into());
                }
                if Instant::now() > deadline {
                    return Err("Hết thời gian kết nối tiến trình quản lý lõi.".into());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        Ok(Connection {
            reader: events,
            writer: commands,
        })
    }
}

pub fn open_parent_pipe(nonce: &str, expected_pid: u32) -> Result<Connection, String> {
    if nonce.len() != 32 || !nonce.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid pipe ID".into());
    }
    let open = |suffix| -> Result<File, String> {
        let name = format!(r"\\.\pipe\Umbra-{nonce}-{suffix}");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(name)
            .map_err(|e| e.to_string())?;
        let mut pid = 0;
        if unsafe { GetNamedPipeServerProcessId(file.as_raw_handle(), &mut pid) } == 0
            || pid != expected_pid
        {
            return Err("Unexpected parent".into());
        }
        Ok(file)
    };
    let events = open("events")?;
    let commands = open("commands")?;
    Ok(Connection {
        reader: commands,
        writer: events,
    })
}

pub fn job_for(child: &std::process::Child) -> Result<Handle, String> {
    unsafe {
        let job = Handle(CreateJobObjectW(null(), null()));
        if job.0.is_null() {
            return Err(last_error());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            size_of_val(&limits) as u32,
        ) == 0
        {
            return Err(last_error());
        }
        if AssignProcessToJobObject(job.0, child.as_raw_handle()) == 0 {
            return Err(last_error());
        }
        Ok(job)
    }
}

pub fn singleton() -> Option<Handle> {
    let name = wide("Local\\Umbra-Desktop-v1");
    let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    let already = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let guard = Handle(handle);
    if already { None } else { Some(guard) }
}

pub fn show_event() -> Option<Handle> {
    let handle = unsafe { CreateEventW(null(), 0, 0, wide("Local\\Umbra-Show-v1").as_ptr()) };
    if handle.is_null() {
        None
    } else {
        Some(Handle(handle))
    }
}
pub fn show_requested(event: &Handle) -> bool {
    unsafe { WaitForSingleObject(event.0, 0) == WAIT_OBJECT_0 }
}
pub fn show_existing() {
    unsafe {
        let event = Handle(OpenEventW(
            EVENT_MODIFY_STATE,
            0,
            wide("Local\\Umbra-Show-v1").as_ptr(),
        ));
        if !event.0.is_null() {
            SetEvent(event.0);
        }
    }
}
pub fn quit_event() -> Option<Handle> {
    let handle = unsafe { CreateEventW(null(), 0, 0, wide("Local\\Umbra-Quit-v1").as_ptr()) };
    if handle.is_null() {
        None
    } else {
        Some(Handle(handle))
    }
}
pub fn quit_existing() {
    unsafe {
        let event = Handle(OpenEventW(
            EVENT_MODIFY_STATE,
            0,
            wide("Local\\Umbra-Quit-v1").as_ptr(),
        ));
        if !event.0.is_null() {
            SetEvent(event.0);
        }
    }
}

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
pub fn startup_enabled() -> bool {
    unsafe {
        let mut buffer = [0u16; 2048];
        let mut bytes = (buffer.len() * 2) as u32;
        if RegGetValueW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            wide("Umbra").as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr() as _,
            &mut bytes,
        ) != 0
        {
            return false;
        }
        let length = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        let expected = std::env::current_exe()
            .map(|path| format!("\"{}\" --start-hidden", path.display()))
            .unwrap_or_default();
        String::from_utf16_lossy(&buffer[..length]) == expected
    }
}
pub fn set_startup(enabled: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    unsafe {
        let mut key = null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(RUN_KEY).as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut key,
            null_mut(),
        );
        if status != 0 {
            return Err(std::io::Error::from_raw_os_error(status as i32).to_string());
        }
        let result = if enabled {
            let command = wide(format!("\"{}\" --start-hidden", exe.display()));
            RegSetValueExW(
                key,
                wide("Umbra").as_ptr(),
                0,
                REG_SZ,
                command.as_ptr() as _,
                (command.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key, wide("Umbra").as_ptr())
        };
        RegCloseKey(key);
        if result == 0 || (!enabled && result == ERROR_FILE_NOT_FOUND) {
            Ok(())
        } else {
            Err(std::io::Error::from_raw_os_error(result as i32).to_string())
        }
    }
}
