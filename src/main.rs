#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod config;
mod engine;
mod platform;
mod protocol;
mod tray;
mod verification;
slint::include_modules!();
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--quit") {
        platform::quit_existing();
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--smoke-engine") {
        return verification::engine_smoke(std::path::Path::new(
            args.get(2).ok_or("missing report path")?,
        ));
    }
    if args.get(1).map(String::as_str) == Some("--engine-worker") {
        engine::worker(
            args.get(2).ok_or("missing nonce")?,
            args.get(3).ok_or("missing parent")?.parse()?,
        )?;
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--ipc-self-test") {
        let mut pipe = platform::connect_helper_mode(false)?;
        assert!(matches!(
            protocol::receive::<protocol::Event>(&mut pipe.reader)?,
            protocol::Event::Ready
        ));
        protocol::send(
            &mut pipe.writer,
            &protocol::Request::Start(config::Settings {
                schema: 999,
                ..Default::default()
            }),
        )?;
        assert!(matches!(
            protocol::receive::<protocol::Event>(&mut pipe.reader)?,
            protocol::Event::Error(_)
        ));
        protocol::send(&mut pipe.writer, &protocol::Request::Stop)?;
        assert!(matches!(
            protocol::receive::<protocol::Event>(&mut pipe.reader)?,
            protocol::Event::Stopped
        ));
        protocol::send(&mut pipe.writer, &protocol::Request::Shutdown)?;
        println!(
            "PASS: named pipe ACL, peer PID, framed messages, invalid config, stop/shutdown. No interception."
        );
        return Ok(());
    }
    if args.get(1).map(String::as_str) == Some("--check-engine") {
        let mut pipe = platform::connect_helper()?;
        let mut report = String::new();
        for profile in 0..3 {
            protocol::send(
                &mut pipe.writer,
                &protocol::Request::Validate(config::Settings {
                    profile,
                    ..Default::default()
                }),
            )?;
            loop {
                let event = protocol::receive::<protocol::Event>(&mut pipe.reader)?;
                report.push_str(&format!("{event:?}\n"));
                match event {
                    protocol::Event::Validated => break,
                    protocol::Event::Error(_) => {
                        protocol::send(&mut pipe.writer, &protocol::Request::Shutdown)?;
                        if let Some(path) = args.get(2) {
                            std::fs::write(path, &report)?;
                        }
                        return Err(report.into());
                    }
                    _ => {}
                }
            }
        }
        protocol::send(&mut pipe.writer, &protocol::Request::Shutdown)?;
        if let Some(path) = args.get(2) {
            std::fs::write(path, &report)?;
        }
        println!("{report}");
        return Ok(());
    }
    let Some(_instance) = platform::singleton() else {
        platform::show_existing();
        return Ok(());
    };
    let screenshot = if args.get(1).map(String::as_str) == Some("--screenshots") {
        Some(std::path::PathBuf::from(
            args.get(2).ok_or("missing output path")?,
        ))
    } else {
        None
    };
    app::run(
        screenshot,
        args.get(1).map(String::as_str) == Some("--start-hidden"),
    )?;
    Ok(())
}
