use crate::{
    config::Settings,
    platform,
    protocol::{self, Event, Request},
};
use std::{
    fs, io,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

// End-to-end smoke test: a reserved example domain, never the user's saved list.
pub fn engine_smoke(report_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let connection = platform::connect_helper()?;
    let mut reader = connection.reader;
    let mut writer = connection.writer;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        while let Ok(event) = protocol::receive::<Event>(&mut reader) {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    let settings = Settings {
        domains: "example.com".into(),
        exclusions: String::new(),
        probe: "example.com".into(),
        ..Default::default()
    };
    let mut report = String::new();
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        protocol::send(&mut writer, &Request::Start(settings.clone()))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut pid = 0;
        while Instant::now() < deadline {
            let event = rx.recv_timeout(Duration::from_secs(3))?;
            report.push_str(&format!("{event:?}\n"));
            match event {
                Event::Running(value) => {
                    pid = value;
                    break;
                }
                Event::Error(error) => return Err(error.into()),
                _ => {}
            }
        }
        if pid == 0 {
            return Err("Engine did not enter Running".into());
        }
        protocol::send(&mut writer, &Request::Stop)?;
        loop {
            let event = rx.recv_timeout(Duration::from_secs(10))?;
            report.push_str(&format!("{event:?}\n"));
            match event {
                Event::Stopped => break,
                Event::Error(error) => return Err(error.into()),
                _ => {}
            }
        }
        // A second start verifies that Stop released driver/process state.
        protocol::send(&mut writer, &Request::Start(settings))?;
        loop {
            let event = rx.recv_timeout(Duration::from_secs(10))?;
            report.push_str(&format!("{event:?}\n"));
            match event {
                Event::Running(_) => break,
                Event::Error(error) => return Err(error.into()),
                _ => {}
            }
        }
        report.push_str("PASS: start, running PID, stop acknowledgment, restart. Closing command channel while running.\n");
        Ok(())
    })();
    // Closing the command pipe simulates parent loss; worker must stop the engine.
    drop(writer);
    let cleanup_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match rx.recv_timeout(cleanup_deadline.saturating_duration_since(Instant::now())) {
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                report.push_str(
                    "PASS: helper closed events after parent command channel disappeared.\n",
                );
                break;
            }
            Ok(event) => {
                report.push_str(&format!("Final event: {event:?}\n"));
            }
            Err(_) => {
                report.push_str("FAIL: helper did not disconnect.\n");
                fs::write(report_path, &report)?;
                return Err(
                    io::Error::new(io::ErrorKind::TimedOut, "helper cleanup timeout").into(),
                );
            }
        }
    }
    fs::write(report_path, &report)?;
    println!("{report}");
    result
}
