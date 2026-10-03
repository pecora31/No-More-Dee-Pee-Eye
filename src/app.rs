use crate::{
    AppWindow,
    config::{self, PROFILE_NAMES, Settings},
    engine, platform,
    protocol::{self, Event, Request},
};
use slint::ComponentHandle;
use std::{
    cell::RefCell,
    collections::VecDeque,
    fs,
    os::windows::process::CommandExt,
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

enum Update {
    Engine(Event),
    Probe(String),
    Runtime(Result<(), String>),
}
fn read_settings(ui: &AppWindow) -> Result<Settings, String> {
    Settings {
        schema: 1,
        profile: ui.get_profile() as usize,
        domains: ui.get_domains().into(),
        exclusions: ui.get_exclusions().into(),
        probe: ui.get_probe_host().into(),
    }
    .validated()
}
fn populate(ui: &AppWindow, settings: &Settings) {
    ui.set_profile(settings.profile as i32);
    ui.set_domains(settings.domains.clone().into());
    ui.set_exclusions(settings.exclusions.clone().into());
    ui.set_probe_host(settings.probe.clone().into());
}
fn notice(ui: &AppWindow, text: impl Into<slint::SharedString>, error: bool) {
    let text = text.into();
    ui.set_notice(text.clone());
    ui.set_notice_error(error);
    let serial = ui.get_notice_serial().wrapping_add(1);
    ui.set_notice_serial(serial);
    if !error && !text.is_empty() {
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_secs(3), move || {
            if let Some(ui) = weak.upgrade()
                && ui.get_notice_serial() == serial
            {
                ui.set_notice("".into());
            }
        });
    }
}

pub(crate) fn restore_window(ui: &AppWindow) {
    if ui.window().is_minimized() {
        ui.window().set_minimized(false);
    }
    if !ui.window().is_visible() {
        let _ = ui.show();
    }

    // A minimized native surface must be painted again after Windows restores
    // it. Queue one paint after the restore event, without rebuilding the UI.
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::ZERO, move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().request_redraw();
        }
    });
}

fn controller(rx: mpsc::Receiver<Request>, updates: mpsc::SyncSender<Update>) {
    let mut connection: Option<fs::File> = None;
    let mut alive = Arc::new(AtomicBool::new(false));
    for request in rx {
        if !alive.load(Ordering::Acquire) {
            connection = None;
        }
        if connection.is_none() && matches!(request, Request::Start(_)) {
            match platform::connect_helper() {
                Ok(pipe) => {
                    alive = Arc::new(AtomicBool::new(true));
                    let reader_alive = alive.clone();
                    let mut reader = pipe.reader;
                    let tx = updates.clone();
                    std::thread::spawn(move || {
                        while let Ok(event) = protocol::receive::<Event>(&mut reader) {
                            if matches!(event, Event::Log(_)) {
                                let _ = tx.try_send(Update::Engine(event));
                            } else if tx.send(Update::Engine(event)).is_err() {
                                return;
                            }
                        }
                        reader_alive.store(false, Ordering::Release);
                        let _ = tx.send(Update::Engine(Event::Error(
                            "Mất kết nối tiến trình quản lý lõi. Hãy thử bật lại.".into(),
                        )));
                    });
                    connection = Some(pipe.writer);
                }
                Err(e) => {
                    let _ = updates.send(Update::Engine(Event::Error(e)));
                    continue;
                }
            }
        }
        let shutdown = matches!(request, Request::Shutdown);
        if let Some(pipe) = connection.as_mut()
            && let Err(e) = protocol::send(pipe, &request)
        {
            connection = None;
            let _ = updates.send(Update::Engine(Event::Error(format!(
                "Kết nối quản lý lõi đã đóng: {e}. Bấm bật để kết nối lại."
            ))));
        }
        if shutdown {
            break;
        }
    }
}

fn probe(host: &str, active: bool) -> String {
    let host = match config::normalize_domains(host, false) {
        Ok(s) if s.lines().count() == 1 => s,
        _ => return "Tên miền kiểm tra không hợp lệ.".into(),
    };
    let curl = std::env::var_os("SystemRoot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "C:/Windows".into())
        .join("System32/curl.exe");
    let result = Command::new(curl)
        .args([
            "--disable",
            "--silent",
            "--show-error",
            "--head",
            "--location",
            "--max-redirs",
            "3",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--noproxy",
            "*",
            "--connect-timeout",
            "5",
            "--max-time",
            "12",
            "--output",
            "NUL",
            "--write-out",
            "%{http_code}|%{time_total}",
            &format!("https://{host}/"),
        ])
        .stdin(Stdio::null())
        .creation_flags(0x08000000)
        .output();
    let mode = if active { "lõi bật" } else { "lõi tắt" };
    let time = timestamp();
    match result {
        Ok(output) if output.status.success() => {
            let output = String::from_utf8_lossy(&output.stdout);
            let fields: Vec<_> = output.trim().split('|').collect();
            let code = fields.first().copied().unwrap_or("?");
            let elapsed = fields.get(1).copied().unwrap_or("?");
            format!("{host} · HTTP {code} · {elapsed}s · {mode} · {time} UTC")
        }
        Ok(output) => {
            let reason = match output.status.code() {
                Some(6) => "không phân giải được DNS",
                Some(7) => "không kết nối được máy chủ",
                Some(28) => "hết thời gian 12 giây",
                Some(35) | Some(60) => "lỗi TLS/chứng chỉ",
                _ => "yêu cầu HTTPS thất bại",
            };
            format!("{host} · {reason} · {mode} · {time} UTC.")
        }
        Err(e) => format!("Không chạy được curl của Windows: {e}"),
    }
}
fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        % 86400;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

pub fn run(
    screenshot: Option<std::path::PathBuf>,
    start_hidden: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let ui = AppWindow::new()?;
    let weak = ui.as_weak();
    ui.on_dismiss_notice(move || {
        if let Some(ui) = weak.upgrade() {
            notice(&ui, "", false);
        }
    });
    let tray = match crate::tray::create(&ui) {
        Ok(tray) => Some(tray),
        Err(error) => {
            notice(&ui, format!("Không tạo được khay hệ thống: {error}"), true);
            None
        }
    };
    ui.set_tray_available(tray.is_some());
    ui.set_startup_enabled(platform::startup_enabled());
    let tray_available = tray.is_some();
    ui.window().on_close_requested(move || {
        if !tray_available {
            let _ = slint::quit_event_loop();
        }
        slint::CloseRequestResponse::HideWindow
    });
    ui.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
    let weak = ui.as_weak();
    ui.on_hide_to_tray(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = ui.hide();
        }
    });
    let weak = ui.as_weak();
    ui.on_startup_changed(move |enabled| {
        if let Some(ui) = weak.upgrade() {
            match platform::set_startup(enabled) {
                Ok(()) => notice(&ui, "", false),
                Err(e) => {
                    ui.set_startup_enabled(platform::startup_enabled());
                    notice(&ui, e, true);
                }
            }
        }
    });
    let data = config::data_dir();
    let path = data.join("settings.toml");
    ui.set_data_path(data.display().to_string().into());
    match config::load(&path) {
        Ok(settings) => populate(&ui, &settings),
        Err(e) => {
            populate(&ui, &Settings::default());
            notice(
                &ui,
                format!("{e} File gốc được giữ nguyên đến khi bạn lưu."),
                true,
            );
        }
    }
    let (request_tx, request_rx) = mpsc::channel();
    let (update_tx, update_rx) = mpsc::sync_channel(512);
    let controller_tx = update_tx.clone();
    std::thread::spawn(move || controller(request_rx, controller_tx));
    let runtime_tx = update_tx.clone();
    std::thread::spawn(move || {
        let _ = runtime_tx.send(Update::Runtime(engine::verify_runtime(
            &engine::runtime_dir(),
        )));
    });
    let logs = Rc::new(RefCell::new(VecDeque::<String>::from([format!(
        "{} UTC  No More Dee Pee Eye · Phiên mới. Lõi chưa được bật.",
        timestamp()
    )])));

    let weak = ui.as_weak();
    let tx = request_tx.clone();
    let settings_path = path.clone();
    ui.on_toggle(move || {
        if let Some(ui) = weak.upgrade() {
            if ui.get_busy() {
                return;
            }
            let request = if ui.get_running() {
                Request::Stop
            } else {
                match read_settings(&ui).and_then(|settings| {
                    config::save(&settings_path, &settings)?;
                    Ok(settings)
                }) {
                    Ok(settings) => Request::Start(settings),
                    Err(e) => {
                        notice(&ui, e, true);
                        return;
                    }
                }
            };
            ui.set_busy(true);
            ui.set_state_title(
                if ui.get_running() {
                    "Đang ngắt kết nối…"
                } else {
                    "Đang kết nối…"
                }
                .into(),
            );
            notice(&ui, "", false);
            if tx.send(request).is_err() {
                ui.set_busy(false);
                notice(&ui, "Bộ điều khiển đã đóng. Hãy mở lại ứng dụng.", true);
            }
        }
    });
    let weak = ui.as_weak();
    let settings_path = path.clone();
    ui.on_save(move || {
        if let Some(ui) = weak.upgrade() {
            match read_settings(&ui).and_then(|s| {
                config::save(&settings_path, &s)?;
                Ok(s)
            }) {
                Ok(s) => {
                    populate(&ui, &s);
                    notice(&ui, "Đã lưu.", false);
                }
                Err(e) => notice(&ui, e, true),
            }
        }
    });
    let weak = ui.as_weak();
    let tx = update_tx.clone();
    ui.on_probe(move || {
        if let Some(ui) = weak.upgrade() {
            if ui.get_probing() {
                return;
            }
            let host: String = ui.get_probe_host().into();
            if config::normalize_domains(&host, false).is_err() || host.lines().count() != 1 {
                notice(&ui, "Nhập một domain hợp lệ trong Chẩn đoán.", true);
                return;
            }
            let active = ui.get_running();
            ui.set_probing(true);
            ui.set_probe_result("Đang kiểm tra…".into());
            let tx = tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Update::Probe(probe(&host, active)));
            });
        }
    });
    let weak = ui.as_weak();
    let log_data = logs.clone();
    ui.on_clear_logs(move || {
        log_data.borrow_mut().clear();
        if let Some(ui) = weak.upgrade() {
            ui.set_logs("".into());
        }
    });
    let weak = ui.as_weak();
    let log_data = logs.clone();
    let export_dir = data.clone();
    ui.on_export_logs(move || {
        if let Some(ui) = weak.upgrade() {
            let output = export_dir.join("diagnostics.txt");
            let text = format!(
                "No More Dee Pee Eye 0.1.2\nEngine bundle: {}\nPreset: {}\n{}\n\n{}\n",
                engine::REVISION,
                ui.get_profile_name(),
                ui.get_probe_result(),
                log_data
                    .borrow()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let result = fs::create_dir_all(&export_dir).and_then(|_| fs::write(&output, text));
            match result {
                Ok(()) => notice(&ui, format!("Đã xuất: {}", output.display()), false),
                Err(e) => notice(&ui, e.to_string(), true),
            }
        }
    });
    let folder = data.clone();
    let weak = ui.as_weak();
    ui.on_open_folder(move || {
        let result = fs::create_dir_all(&folder)
            .and_then(|_| Command::new("explorer.exe").arg(&folder).spawn());
        if let (Err(e), Some(ui)) = (result, weak.upgrade()) {
            notice(&ui, e.to_string(), true);
        }
    });
    let weak = ui.as_weak();
    ui.on_reset(move || {
        if let Some(ui) = weak.upgrade() {
            populate(&ui, &Settings::default());
            notice(&ui, "Đã đặt mặc định. Bấm Lưu để áp dụng.", false);
        }
    });

    let session_started = Rc::new(RefCell::new(None::<Instant>));
    let show_event = platform::show_event();
    let quit_event = platform::quit_event();
    let timer = slint::Timer::default();
    let was_minimized = Rc::new(RefCell::new(ui.window().is_minimized()));
    let last_profile = Rc::new(RefCell::new(-1));
    let last_uptime_second = Rc::new(RefCell::new(None::<u64>));
    let weak = ui.as_weak();
    let log_data = logs.clone();
    let minimize_state = was_minimized.clone();
    let profile_state = last_profile.clone();
    let uptime_state = last_uptime_second.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(250),
        move || {
            if let Some(ui) = weak.upgrade() {
                if show_event.as_ref().is_some_and(platform::show_requested) {
                    restore_window(&ui);
                }
                if quit_event.as_ref().is_some_and(platform::show_requested) {
                    let _ = slint::quit_event_loop();
                }
                let minimized = ui.window().is_minimized();
                if *minimize_state.borrow() && !minimized {
                    restore_window(&ui);
                }
                *minimize_state.borrow_mut() = minimized;
                let profile = ui.get_profile();
                if *profile_state.borrow() != profile {
                    ui.set_profile_name(
                        PROFILE_NAMES
                            .get(profile as usize)
                            .unwrap_or(&PROFILE_NAMES[0])
                            .to_string()
                            .into(),
                    );
                    *profile_state.borrow_mut() = profile;
                }
                let mut log_changed = false;
                for update in update_rx.try_iter().take(64) {
                    match update {
                        Update::Runtime(result) => match result {
                            Ok(()) => {
                                ui.set_engine_ready(true);
                                ui.set_engine_label("Zapret2 1.0.5.2".into());
                            }
                            Err(e) => {
                                ui.set_engine_ready(false);
                                ui.set_engine_label("Thiếu hoặc sai runtime".into());
                                notice(&ui, e, true);
                            }
                        },
                        Update::Probe(result) => {
                            ui.set_probing(false);
                            ui.set_probe_result(result.clone().into());
                            log_data
                                .borrow_mut()
                                .push_back(format!("{} UTC  {result}", timestamp()));
                            log_changed = true;
                        }
                        Update::Engine(event) => {
                            let log = match event {
                                Event::Ready => "Tiến trình quản lý lõi đã kết nối.".into(),
                                Event::Validated => "Cấu hình và Lua hợp lệ.".into(),
                                Event::Starting => {
                                    ui.set_state_title("Đang kết nối…".into());
                                    "Đang chờ Zapret2 khởi tạo.".into()
                                }
                                Event::Running(pid) => {
                                    ui.set_busy(false);
                                    ui.set_running(true);
                                    ui.set_state_title("Đã kết nối".into());
                                    ui.set_state_detail(
                                        "Xử lý TLS cho danh sách tên miền đã chọn.".into(),
                                    );
                                    ui.set_process_id(pid.to_string().into());
                                    *session_started.borrow_mut() = Some(Instant::now());
                                    notice(&ui, "", false);
                                    format!("Zapret2 đang chạy, PID {pid}.")
                                }
                                Event::Stopped => {
                                    ui.set_busy(false);
                                    ui.set_running(false);
                                    ui.set_state_title("Đã ngắt kết nối".into());
                                    ui.set_state_detail(
                                        "Lưu lượng mạng đang hoạt động bình thường.".into(),
                                    );
                                    ui.set_process_id("—".into());
                                    *session_started.borrow_mut() = None;
                                    notice(&ui, "", false);
                                    "Đã dừng tiến trình lõi.".into()
                                }
                                Event::Error(error) => {
                                    ui.set_busy(false);
                                    ui.set_running(false);
                                    ui.set_state_title("Cần kiểm tra".into());
                                    ui.set_state_detail(
                                        "Lõi chưa sẵn sàng. Xem Chẩn đoán để biết chi tiết.".into(),
                                    );
                                    ui.set_process_id("—".into());
                                    *session_started.borrow_mut() = None;
                                    notice(&ui, error.clone(), true);
                                    error
                                }
                                Event::Log(log) => log,
                            };
                            log_data
                                .borrow_mut()
                                .push_back(format!("{} UTC  {log}", timestamp()));
                            log_changed = true;
                        }
                    }
                }
                if log_changed || ui.get_logs().is_empty() {
                    let mut lines = log_data.borrow_mut();
                    while lines.len() > 300 {
                        lines.pop_front();
                    }
                    ui.set_logs(lines.iter().cloned().collect::<Vec<_>>().join("\n").into());
                }
                let uptime_seconds = session_started
                    .borrow()
                    .as_ref()
                    .map(|start| start.elapsed().as_secs());
                if *uptime_state.borrow() != uptime_seconds {
                    let uptime = uptime_seconds.map_or_else(
                        || "—".to_string(),
                        |secs| format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
                    );
                    ui.set_uptime(uptime.into());
                    *uptime_state.borrow_mut() = uptime_seconds;
                }
            }
        },
    );

    if let Some(folder) = screenshot {
        fs::create_dir_all(&folder)?;
        let weak = ui.as_weak();
        let index = Rc::new(RefCell::new(0));
        let screenshot_timer = slint::Timer::default();
        screenshot_timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(650),
            move || {
                if let Some(ui) = weak.upgrade() {
                    let mut page = index.borrow_mut();
                    if *page > 0
                        && *page <= 4
                        && let Ok(buffer) = ui.window().take_snapshot()
                    {
                        let file = folder.join(format!("page-{}.png", *page - 1));
                        let _ = image::save_buffer(
                            file,
                            buffer.as_bytes(),
                            buffer.width(),
                            buffer.height(),
                            image::ColorType::Rgba8,
                        );
                    }
                    if *page == 6 {
                        let _ = slint::quit_event_loop();
                    } else if *page == 4 {
                        assert!(ui.get_tray_available(), "Tray creation failed");
                        ui.invoke_hide_to_tray();
                        assert!(!ui.window().is_visible());
                        *page += 1;
                    } else if *page == 5 {
                        restore_window(&ui);
                        assert!(ui.window().is_visible());
                        fs::write(
                            folder.join("tray-smoke.txt"),
                            "PASS: tray created; restore queued one redraw and preserved the event loop.\n",
                        )
                        .expect("write tray report");
                        *page += 1;
                    } else {
                        ui.set_page(*page);
                        *page += 1;
                    }
                }
            },
        );
        ui.show()?;
        slint::run_event_loop_until_quit()?;
    } else {
        ui.show()?;
        if start_hidden && tray_available {
            // Hiding before run_event_loop_until_quit sets the backend's default
            // last-window exit flag. Wait until the persistent loop is active.
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::ZERO, move || {
                if let Some(ui) = weak.upgrade() {
                    let _ = ui.hide();
                }
            });
        }
        slint::run_event_loop_until_quit()?;
    }
    let _ = request_tx.send(Request::Shutdown);
    Ok(())
}
