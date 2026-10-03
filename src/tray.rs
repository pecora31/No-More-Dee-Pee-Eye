use crate::AppWindow;
use slint::ComponentHandle;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

pub fn create(ui: &AppWindow) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    let menu = Menu::new();
    let open = MenuItem::new("Mở No More Dee Pee Eye", true, None);
    let toggle = MenuItem::new("Bật / tắt Zapret2", true, None);
    let quit = MenuItem::new("Thoát và dừng lõi", true, None);
    menu.append_items(&[&open, &toggle, &PredefinedMenuItem::separator(), &quit])?;
    let (open_id, toggle_id, quit_id) = (open.id().clone(), toggle.id().clone(), quit.id().clone());
    let weak = ui.as_weak();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let weak = weak.clone();
        if event.id == open_id {
            let _ = weak.upgrade_in_event_loop(|ui| {
                crate::app::restore_window(&ui);
            });
        } else if event.id == toggle_id {
            let _ = weak.upgrade_in_event_loop(|ui| {
                if !ui.get_busy() && ui.get_engine_ready() {
                    ui.invoke_toggle();
                }
                crate::app::restore_window(&ui);
            });
        } else if event.id == quit_id {
            let _ = slint::quit_event_loop();
        }
    }));
    let weak = ui.as_weak();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if matches!(event, TrayIconEvent::DoubleClick { .. }) {
            let _ = weak.clone().upgrade_in_event_loop(|ui| {
                crate::app::restore_window(&ui);
            });
        }
    }));
    let mut pixels = Vec::with_capacity(32 * 32 * 4);
    for y in 0i32..32 {
        for x in 0i32..32 {
            let diamond = (x - 16).abs() + (y - 16).abs() < 14;
            pixels.extend_from_slice(if !diamond {
                &[0, 0, 0, 0]
            } else if x < 16 {
                &[91, 157, 255, 255]
            } else {
                &[50, 111, 203, 255]
            });
        }
    }
    Ok(TrayIconBuilder::new()
        .with_icon(Icon::from_rgba(pixels, 32, 32)?)
        .with_menu(Box::new(menu))
        .with_tooltip("No More Dee Pee Eye · Zapret2")
        .build()?)
}
