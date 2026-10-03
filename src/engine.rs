use crate::{
    config,
    config::Settings,
    platform,
    protocol::{self, Event, Request},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

mod bundled_runtime {
    include!(concat!(env!("OUT_DIR"), "/bundled_runtime.rs"));
}

pub const REVISION: &str = "6eb463a6758fb48cd101bc55dfd057e6e9d98af1";
#[derive(Deserialize)]
struct Entry {
    path: String,
    sha256: String,
}
pub fn runtime_dir() -> PathBuf {
    let alongside = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("runtime");
    if alongside.join("winws2.exe").is_file() {
        alongside
    } else {
        config::data_dir().join("runtime").join(REVISION)
    }
}

pub fn ensure_runtime() -> Result<(), String> {
    let root = runtime_dir();
    if root.join("winws2.exe").is_file() {
        return Ok(());
    }
    for (relative, bytes) in [
        ("winws2.exe", bundled_runtime::WINWS2),
        ("cygwin1.dll", bundled_runtime::CYGWIN),
        ("WinDivert.dll", bundled_runtime::WINDIVERT),
        ("WinDivert64.sys", bundled_runtime::WINDIVERT_DRIVER),
        ("lua/zapret-lib.lua", bundled_runtime::ZAPRET_LIB),
        ("lua/zapret-antidpi.lua", bundled_runtime::ZAPRET_ANTIDPI),
    ] {
        write_bundled_file(&root.join(relative), bytes)?;
    }
    verify_runtime(&root)
}

fn write_bundled_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Ok(existing) = fs::read(path)
        && Sha256::digest(&existing).as_slice() == Sha256::digest(bytes).as_slice()
    {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Đường dẫn runtime không hợp lệ.".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("Không tạo được runtime: {e}"))?;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut file =
        fs::File::create(&temporary).map_err(|e| format!("Không ghi được runtime: {e}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("Không ghi được runtime: {e}"))?;
    fs::rename(&temporary, path)
        .or_else(|_| {
            if path.is_file() {
                let _ = fs::remove_file(&temporary);
                Ok(())
            } else {
                Err(std::io::Error::other("không thay thế được runtime"))
            }
        })
        .map_err(|e| format!("Không cài được runtime: {e}"))
}
pub fn verify_runtime(root: &Path) -> Result<(), String> {
    lock_runtime(root).map(|_| ())
}
fn lock_runtime(root: &Path) -> Result<Vec<fs::File>, String> {
    let mut locks = Vec::new();
    // Hold the directories and files against replacement while the elevated core runs.
    for directory in [root.to_path_buf(), root.join("lua")] {
        locks.push(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x02000000)
                .open(directory)
                .map_err(|e| format!("Không mở được thư mục runtime: {e}"))?,
        );
    }
    let manifest: Vec<Entry> = serde_json::from_str(
        include_str!("../engine-manifest.json").trim_start_matches('\u{feff}'),
    )
    .map_err(|e| e.to_string())?;
    for entry in manifest {
        let file = root.join(&entry.path);
        let mut handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&file)
            .map_err(|_| {
                format!(
                    "Thiếu hoặc không đọc được {}. Khôi phục runtime gốc.",
                    entry.path
                )
            })?;
        let mut bytes = Vec::new();
        handle.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        let digest = format!("{:x}", Sha256::digest(bytes));
        if digest != entry.sha256 {
            return Err(format!(
                "{} không khớp bản lõi đã ghim. Hãy khôi phục runtime gốc.",
                entry.path
            ));
        }
        locks.push(handle);
    }
    Ok(locks)
}

pub fn arguments(settings: &Settings) -> Result<Vec<String>, String> {
    let settings = settings.validated()?;
    let mut args: Vec<String> = [
        "--wf-tcp-out=443",
        "--lua-init=@lua/zapret-lib.lua",
        "--lua-init=@lua/zapret-antidpi.lua",
        "--filter-tcp=443",
        "--filter-l7=tls",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    args.push(format!(
        "--hostlist-domains={}",
        settings.domains.replace('\n', ",")
    ));
    if !settings.exclusions.is_empty() {
        args.push(format!(
            "--hostlist-exclude-domains={}",
            settings.exclusions.replace('\n', ",")
        ));
    }
    args.push("--payload=tls_client_hello".into());
    args.push("--out-range=-n8".into());
    match settings.profile {
        0 => args.push("--lua-desync=multisplit:pos=1,midsld".into()),
        1 => args.push("--lua-desync=multidisorder:pos=1,midsld".into()),
        2 => {
            args.push("--lua-desync=fake:blob=fake_default_tls:tcp_md5".into());
            args.push("--lua-desync=multisplit:pos=1,midsld".into());
        }
        _ => unreachable!(),
    }
    Ok(args)
}

struct Running {
    child: Child,
    _job: platform::Handle,
    _files: Vec<fs::File>,
    started: Instant,
    announced: bool,
}
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn pipe_logs(mut reader: impl Read + Send + 'static, tx: mpsc::SyncSender<String>) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 2048];
        let mut pending = Vec::new();
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for byte in &chunk[..n] {
                        if *byte == b'\n' || pending.len() >= 4096 {
                            let line = String::from_utf8_lossy(&pending).trim().to_owned();
                            if !line.is_empty() {
                                let _ = tx.try_send(line);
                            }
                            pending.clear();
                        } else {
                            pending.push(*byte);
                        }
                    }
                }
            }
        }
        if !pending.is_empty() {
            let _ = tx.try_send(String::from_utf8_lossy(&pending).trim().into());
        }
    });
}

fn spawn_engine(
    settings: &Settings,
    check_only: bool,
    logs: mpsc::SyncSender<String>,
) -> Result<Running, String> {
    let settings = settings.validated()?;
    let runtime = runtime_dir();
    let files = lock_runtime(&runtime)?;
    let mut args = arguments(&settings)?;
    if check_only {
        args.push("--intercept=0".into());
    }
    let mut child = Command::new(runtime.join("winws2.exe"))
        .args(args)
        .current_dir(&runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("Không khởi động được Zapret2: {e}"))?;
    let job = match platform::job_for(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Không tạo được nhóm tiến trình: {e}"));
        }
    };
    if let Some(stdout) = child.stdout.take() {
        pipe_logs(stdout, logs.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        pipe_logs(stderr, logs);
    }
    Ok(Running {
        child,
        _job: job,
        _files: files,
        started: Instant::now(),
        announced: false,
    })
}

fn validate_engine(settings: &Settings, logs: mpsc::SyncSender<String>) -> Result<(), String> {
    let mut check = spawn_engine(settings, true, logs)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match check.child.try_wait().map_err(|e| e.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                return Err(format!(
                    "Kiểm tra cấu hình/Lua thất bại ({status}). Xem nhật ký."
                ));
            }
            None if Instant::now() >= deadline => {
                return Err("Kiểm tra lõi quá 10 giây; đã dừng tiến trình kiểm tra.".into());
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

pub fn worker(nonce: &str, parent: u32) -> Result<(), String> {
    let connection = platform::open_parent_pipe(nonce, parent)?;
    let mut pipe = connection.writer;
    let mut reader = connection.reader;
    let (tx, rx) = mpsc::sync_channel(8);
    std::thread::spawn(move || {
        while let Ok(request) = protocol::receive::<Request>(&mut reader) {
            if tx.send(request).is_err() {
                break;
            }
        }
    });
    let (log_tx, log_rx) = mpsc::sync_channel(256);
    let mut running: Option<Running> = None;
    protocol::send(&mut pipe, &Event::Ready).map_err(|e| e.to_string())?;
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Request::Validate(settings)) => {
                let event = match validate_engine(&settings, log_tx.clone()) {
                    Ok(()) => Event::Validated,
                    Err(e) => Event::Error(e),
                };
                protocol::send(&mut pipe, &event).map_err(|e| e.to_string())?;
            }
            Ok(Request::Start(settings)) => {
                if running.is_some() {
                    protocol::send(
                        &mut pipe,
                        &Event::Error("Hãy tắt lõi trước khi đổi cấu hình.".into()),
                    )
                    .map_err(|e| e.to_string())?;
                    continue;
                }
                let event = match validate_engine(&settings, log_tx.clone())
                    .and_then(|_| spawn_engine(&settings, false, log_tx.clone()))
                {
                    Ok(process) => {
                        running = Some(process);
                        Event::Starting
                    }
                    Err(e) => Event::Error(e),
                };
                protocol::send(&mut pipe, &event).map_err(|e| e.to_string())?;
            }
            Ok(Request::Stop) => {
                running.take();
                protocol::send(&mut pipe, &Event::Stopped).map_err(|e| e.to_string())?;
            }
            Ok(Request::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        for log in log_rx.try_iter().take(32) {
            protocol::send(&mut pipe, &Event::Log(log)).map_err(|e| e.to_string())?;
        }
        if let Some(process) = running.as_mut() {
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    running.take();
                    protocol::send(
                        &mut pipe,
                        &Event::Error(format!(
                            "Zapret2 đã thoát ({status}). Xem nhật ký để biết nguyên nhân."
                        )),
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(None)
                    if !process.announced
                        && process.started.elapsed() >= Duration::from_secs(2) =>
                {
                    process.announced = true;
                    protocol::send(&mut pipe, &Event::Running(process.child.id()))
                        .map_err(|e| e.to_string())?;
                }
                Err(e) => {
                    running.take();
                    protocol::send(&mut pipe, &Event::Error(e.to_string()))
                        .map_err(|e| e.to_string())?;
                }
                _ => {}
            }
        }
    }
    drop(running);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_preset_is_scoped_and_uses_lua() {
        for profile in 0..3 {
            let s = Settings {
                profile,
                ..Settings::default()
            };
            let args = arguments(&s).unwrap();
            assert!(
                args.contains(&"--hostlist-domains=youtube.com,googlevideo.com,ytimg.com".into())
            );
            assert!(args.iter().any(|a| a.starts_with("--lua-desync=")));
            assert!(!args.iter().any(|a| a.starts_with("--wf-udp")));
        }
    }
    #[test]
    fn exclusions_are_preserved() {
        let s = Settings {
            exclusions: "example.com".into(),
            ..Settings::default()
        };
        assert!(
            arguments(&s)
                .unwrap()
                .contains(&"--hostlist-exclude-domains=example.com".into())
        );
    }
    #[test]
    fn missing_runtime_is_an_error() {
        assert!(verify_runtime(Path::new("Z:/umbra-does-not-exist")).is_err());
    }
}
