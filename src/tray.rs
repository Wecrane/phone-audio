//! System tray icon and menu (Windows only).
//!
//! The tray is created on the UI thread; menu/click events are forwarded from
//! worker threads through a channel and wake egui with `request_repaint`.

use eframe::egui;
use std::sync::mpsc::{channel, Receiver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Quit,
}

pub struct TrayHandle {
    available: bool,
    #[cfg(windows)]
    _icon: Option<tray_icon::TrayIcon>,
}

impl TrayHandle {
    pub fn available(&self) -> bool {
        self.available
    }
}

/// Create the tray icon and return an event receiver. On unsupported platforms
/// this is a no-op (the app keeps normal window behaviour).
pub fn init(ctx: egui::Context) -> (TrayHandle, Receiver<TrayAction>) {
    #[cfg(windows)]
    {
        let (tx, rx) = channel();
        match build_tray(tx, ctx) {
            Ok(icon) => (
                TrayHandle {
                    available: true,
                    _icon: Some(icon),
                },
                rx,
            ),
            Err(error) => {
                eprintln!("tray: {error}");
                (
                    TrayHandle {
                        available: false,
                        _icon: None,
                    },
                    rx,
                )
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = ctx;
        let (_tx, rx) = channel();
        (
            TrayHandle {
                available: false,
                #[cfg(windows)]
                _icon: None,
            },
            rx,
        )
    }
}

#[cfg(windows)]
fn build_tray(
    tx: std::sync::mpsc::Sender<TrayAction>,
    ctx: egui::Context,
) -> Result<tray_icon::TrayIcon, String> {
    use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show_id = MenuId::new("show");
    let quit_id = MenuId::new("quit");
    let show = MenuItem::with_id(show_id.clone(), "显示主窗口", true, None);
    let quit = MenuItem::with_id(quit_id.clone(), "退出 Phone Audio", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show, &quit])
        .map_err(|error| format!("无法创建托盘菜单：{error}"))?;

    let icon_data = crate::theme::app_icon();
    let icon = tray_icon::Icon::from_rgba(icon_data.rgba, icon_data.width, icon_data.height)
        .map_err(|error| format!("无法创建托盘图标：{error}"))?;

    let tray = TrayIconBuilder::new()
        .with_tooltip("Phone Audio")
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("无法创建托盘：{error}"))?;

    // Menu thread.
    let menu_tx = tx.clone();
    let menu_ctx = ctx.clone();
    std::thread::spawn(move || loop {
        let Ok(event) = MenuEvent::receiver().recv() else {
            return;
        };
        let action = if event.id == show_id {
            TrayAction::Show
        } else if event.id == quit_id {
            TrayAction::Quit
        } else {
            continue;
        };
        let _ = menu_tx.send(action);
        menu_ctx.request_repaint();
    });

    // Left click / double click on the icon shows the window.
    let click_tx = tx;
    let click_ctx = ctx;
    std::thread::spawn(move || loop {
        let Ok(event) = TrayIconEvent::receiver().recv() else {
            return;
        };
        let show = matches!(
            event,
            TrayIconEvent::DoubleClick { .. }
                | TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
        );
        if show {
            let _ = click_tx.send(TrayAction::Show);
            click_ctx.request_repaint();
        }
    });

    Ok(tray)
}
