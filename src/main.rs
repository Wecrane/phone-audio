#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod adb;
mod autostart;
mod scrcpy;
mod settings;
mod theme;
mod tools;
mod tray;

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use settings::Settings;

type ChildSlot = Arc<Mutex<Option<Child>>>;

fn main() -> Result<(), eframe::Error> {
    install_panic_hook();

    // Non-interactive toggles used for scripting / tests.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(flag) = args
        .iter()
        .find(|arg| *arg == "--enable-autostart" || *arg == "--disable-autostart")
    {
        let enable = flag == "--enable-autostart";
        match autostart::set_enabled(enable) {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("autostart: {error}");
                std::process::exit(1);
            }
        }
    }

    let mut startup_settings = Settings::load();
    startup_settings.normalize();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([
                startup_settings.window_width,
                startup_settings.window_height,
            ])
            .with_min_inner_size([640.0, 520.0])
            .with_icon(theme::app_icon()),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    let result = eframe::run_native(
        "Phone Audio",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    );

    if let Err(error) = &result {
        write_diagnostic("start-error.log", &format!("eframe 启动失败：{error}"));
        eprintln!("[phone-audio] eframe 启动失败：{error}");
    }

    result
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let text = format!(
            "Phone Audio 崩溃\n{info}\n\nbacktrace（需要 RUST_BACKTRACE=1）：\n{}",
            std::backtrace::Backtrace::force_capture()
        );
        write_diagnostic("crash.log", &text);
        eprintln!("{text}");
    }));
}

/// Persist a diagnostic file next to `settings.json` (the release build has no
/// console, so this is the only trace left after a crash).
fn write_diagnostic(name: &str, text: &str) {
    if let Some(dir) = settings::config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(name), text);
    }
}

/// egui ships Latin fonts only; load a system CJK font so the UI is readable.
/// Returns the font that was loaded, if any.
fn install_fonts(ctx: &egui::Context) -> Option<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets").join("NotoSansSC-Regular.ttf"));
        }
    }
    #[cfg(debug_assertions)]
    if let Some(dir) = option_env!("CARGO_MANIFEST_DIR") {
        candidates.push(
            std::path::Path::new(dir)
                .join("assets")
                .join("NotoSansSC-Regular.ttf"),
        );
    }
    candidates.extend(
        [
            "C:\\Windows\\Fonts\\simhei.ttf",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\msyhbd.ttc",
            "C:\\Windows\\Fonts\\simsun.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
        ]
        .into_iter()
        .map(std::path::PathBuf::from),
    );

    for path in candidates {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };

        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "cjk".to_owned());
        }
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.push("cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return Some(path);
    }
    None
}

#[derive(Clone)]
enum Status {
    Idle,
    Starting,
    Streaming,
    Reconnecting(String),
    Error(String),
}

struct Shared {
    devices: Vec<adb::Device>,
    status: Status,
    log: VecDeque<String>,
    settings: Settings,
    scrcpy: Arc<scrcpy::Info>,
    adb_version: String,
    adb_ok: bool,
    /// True until the first background capability probe finishes.
    detecting: bool,
    output_devices: Vec<String>,
    output_error: Option<String>,
}

enum Cmd {
    Start,
    Stop,
    Refresh,
    RefreshAudio,
    Pair(String, String),
    Connect(String),
    Shutdown,
}

struct App {
    shared: Arc<Mutex<Shared>>,
    tx: Sender<Cmd>,
    settings: Settings,
    selected: Option<String>,
    auto_scroll: bool,
    child: ChildSlot,
    shutdown_done: Receiver<()>,
    shutting_down: Arc<AtomicBool>,
    wireless_open: bool,
    autostart_enabled: bool,
    tray: tray::TrayHandle,
    tray_rx: Receiver<tray::TrayAction>,
    close_prompt_open: bool,
    force_quit: bool,
    last_window_save: Instant,
    last_saved_window: Option<(f32, f32)>,
}

fn lock_shared(shared: &Arc<Mutex<Shared>>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_child(slot: &ChildSlot) -> MutexGuard<'_, Option<Child>> {
    slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

/// Match the saved serial against the current device list.
///
/// Wireless debugging changes the `ip:port` after a toggle/reboot, but the host
/// stays the same. Exact matches (USB serial / stable mDNS name) win; otherwise
/// we accept the device on the same host so reconnect works without the user
/// re-entering the address.
fn resolve_saved_device<'a>(saved: &str, devices: &'a [adb::Device]) -> Option<&'a adb::Device> {
    if saved.is_empty() {
        return None;
    }
    if let Some(device) = devices.iter().find(|device| device.serial == saved) {
        return Some(device);
    }
    let host = adb::serial_host(saved)?;
    devices
        .iter()
        .find(|device| adb::serial_host(&device.serial) == Some(host))
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut settings = Settings::load();
        settings.normalize();
        let autostart_enabled = autostart::is_enabled().unwrap_or(false);

        // Font installation must happen before the first frame.
        let font = install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx, &settings.theme);

        // Capability probing is done on the engine thread so a slow scrcpy
        // cannot delay the first frame.
        let info = Arc::new(scrcpy::Info::default());

        let mut log: VecDeque<String> = VecDeque::new();
        log.push_back(format!("Phone Audio v{}", env!("CARGO_PKG_VERSION")));
        log.push_back(if autostart_enabled {
            "开机自启动：已开启".to_string()
        } else {
            "开机自启动：未开启".to_string()
        });
        log.push_back("正在后台检测 scrcpy / adb …".to_string());
        match &font {
            Some(path) => log.push_back(format!("中文字体：{}", path.display())),
            None => log.push_back(
                "未找到中文字体：界面可能显示方框（可放 assets/NotoSansSC-Regular.ttf）"
                    .to_string(),
            ),
        }

        let selected = if settings.serial.is_empty() {
            None
        } else {
            Some(settings.serial.clone())
        };

        let shared = Arc::new(Mutex::new(Shared {
            devices: Vec::new(),
            status: Status::Idle,
            log,
            settings: settings.clone(),
            scrcpy: info,
            adb_version: "检测中…".to_string(),
            adb_ok: false,
            detecting: true,
            output_devices: Vec::new(),
            output_error: None,
        }));

        let (tx, rx) = mpsc::channel();
        let (shutdown_tx, shutdown_done) = mpsc::channel();
        let child: ChildSlot = Arc::new(Mutex::new(None));
        let shutting_down = Arc::new(AtomicBool::new(false));

        let ctx = cc.egui_ctx.clone();
        let (tray, tray_rx) = tray::init(ctx.clone());
        let shared_for_engine = shared.clone();
        let child_for_engine = child.clone();
        let shutdown_for_engine = shutting_down.clone();
        thread::spawn(move || {
            engine(
                rx,
                shared_for_engine,
                ctx,
                child_for_engine,
                shutdown_tx,
                shutdown_for_engine,
            );
        });

        Self {
            shared,
            tx,
            settings,
            selected,
            auto_scroll: true,
            child,
            shutdown_done,
            shutting_down,
            wireless_open: false,
            autostart_enabled,
            tray,
            tray_rx,
            close_prompt_open: false,
            force_quit: false,
            last_window_save: Instant::now(),
            last_saved_window: None,
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.settings.save();
        // Tell the engine not to spawn a new scrcpy while we are tearing down.
        self.shutting_down.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Shutdown);

        // Fallback: if the engine is stuck inside a slow adb call, the UI
        // thread still takes scrcpy down instead of leaving an orphan.
        if let Some(mut child) = lock_child(&self.child).take() {
            tools::stop_child(&mut child);
        }
        let _ = self.shutdown_done.recv_timeout(Duration::from_secs(3));
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.request_repaint_after(Duration::from_millis(250));
        if let Some(rect) = ctx.input(|input| input.viewport().inner_rect) {
            if rect.width() >= 320.0 && rect.height() >= 240.0 {
                self.settings.window_width = rect.width();
                self.settings.window_height = rect.height();
                let size = (self.settings.window_width, self.settings.window_height);
                let changed = self.last_saved_window.is_none_or(|(width, height)| {
                    (width - size.0).abs() > 1.0 || (height - size.1).abs() > 1.0
                });
                if changed && self.last_window_save.elapsed() >= Duration::from_millis(800) {
                    self.settings.save();
                    self.last_saved_window = Some(size);
                    self.last_window_save = Instant::now();
                }
            }
        }
        self.handle_tray_and_close(&ctx);

        // Diagnostic aid: `PHONE_AUDIO_DEBUG_UI=1` shows layout metrics in the
        // window title (useful when debugging DPI/panel sizing on a new machine).
        if std::env::var_os("PHONE_AUDIO_DEBUG_UI").is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                "Phone Audio [ppp={:.2}, screen={:.0}x{:.0}]",
                ctx.pixels_per_point(),
                ctx.content_rect().width(),
                ctx.content_rect().height()
            )));
        }

        let (devices, status, log, info, adb_ok, detecting, output_devices, output_error) = {
            let shared = lock_shared(&self.shared);
            (
                shared.devices.clone(),
                shared.status.clone(),
                shared.log.clone(),
                shared.scrcpy.clone(),
                shared.adb_ok,
                shared.detecting,
                shared.output_devices.clone(),
                shared.output_error.clone(),
            )
        };

        // Keep the saved settings compatible with the detected scrcpy build.
        if info.available {
            if self.settings.codec == "flac" && !info.flac_ok() {
                self.settings.codec = "opus".to_string();
            }
            if self.settings.source == "playback" && !info.playback_ok() {
                self.settings.source = "output".to_string();
                self.settings.audio_dup = false;
            }
        }

        let running = matches!(
            &status,
            Status::Starting | Status::Streaming | Status::Reconnecting(_)
        );

        // Resolve a saved wireless serial after its `ip:port` changed (adb
        // wireless debugging picks a new port on every toggle/reboot). This runs
        // while idle and while streaming: the engine may have rediscovered the
        // device by host before the UI saw it, and we want the corrected serial
        // displayed and persisted instead of the stale one.
        if !devices.is_empty() {
            let saved = self.settings.serial.clone();
            let selected_present = self
                .selected
                .as_ref()
                .map(|serial| devices.iter().any(|device| &device.serial == serial))
                .unwrap_or(false);
            let resolved = resolve_saved_device(&saved, &devices)
                .or_else(|| resolve_saved_device(self.selected.as_deref().unwrap_or(""), &devices))
                .cloned();

            if !selected_present {
                self.selected = resolved
                    .as_ref()
                    .map(|device| device.serial.clone())
                    .or_else(|| {
                        devices
                            .iter()
                            .find(|device| device.state == adb::DeviceState::Device)
                            .map(|device| device.serial.clone())
                    });
            }
            if let Some(serial) = &self.selected {
                if self.settings.serial != *serial {
                    self.settings.serial = serial.clone();
                    self.settings.save();
                }
            }
        }

        let selected_device = self
            .selected
            .as_ref()
            .and_then(|serial| devices.iter().find(|device| &device.serial == serial))
            .cloned();

        let can_start = adb_ok
            && info.audio_ok()
            && selected_device
                .as_ref()
                .map(|device| device.state == adb::DeviceState::Device)
                .unwrap_or(false);

        let palette = theme::Palette::for_ui(ui);

        // Ctrl+Enter = start/stop.
        if ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter)) {
            if running {
                let _ = self.tx.send(Cmd::Stop);
            } else if can_start {
                self.prepare_start();
                let _ = self.tx.send(Cmd::Start);
            }
        }

        self.header(ui, &palette, &status);
        self.log_panel(ui, &palette, &log, &info);

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(palette.bg)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        self.device_card(
                            ui,
                            &palette,
                            &devices,
                            selected_device.as_ref(),
                            running,
                            adb_ok,
                            detecting,
                            can_start,
                        );
                        ui.add_space(12.0);

                        if ui.available_width() >= 760.0 {
                            // Fixed heights, chosen just above the natural
                            // content heights: both columns end on the same line.
                            const COLUMN_HEIGHT: f32 = 278.4;
                            const OUTPUT_HEIGHT: f32 = 152.0;
                            const CARD_GAP: f32 = 8.0;
                            const SOURCE_HEIGHT: f32 = COLUMN_HEIGHT - OUTPUT_HEIGHT - CARD_GAP;

                            ui.columns(2, |cols| {
                                self.audio_card(
                                    &mut cols[0],
                                    &palette,
                                    &info,
                                    running,
                                    COLUMN_HEIGHT,
                                );
                                self.output_card(
                                    &mut cols[1],
                                    &palette,
                                    &info,
                                    running,
                                    &output_devices,
                                    &output_error,
                                );
                                self.source_card(&mut cols[1], &info, running, SOURCE_HEIGHT);
                            });
                        } else {
                            self.audio_card(ui, &palette, &info, running, 0.0);
                            ui.add_space(12.0);
                            self.output_card(
                                ui,
                                &palette,
                                &info,
                                running,
                                &output_devices,
                                &output_error,
                            );
                            ui.add_space(12.0);
                            self.source_card(ui, &info, running, 0.0);
                        }
                        self.wireless_card(ui, &palette, running);
                        ui.add_space(4.0);
                    });
            });

        self.close_prompt(&ctx);

        // Single source of truth: the engine always sees what the UI shows.
        lock_shared(&self.shared).settings = self.settings.clone();
    }
}

fn theme_label(value: &str) -> &'static str {
    match value {
        "light" => "浅色",
        "dark" => "深色",
        _ => "跟随系统",
    }
}

fn status_meta(status: &Status, palette: &theme::Palette) -> (String, egui::Color32, bool) {
    match status {
        Status::Idle => ("待机".to_string(), palette.text_dim, false),
        Status::Starting => ("正在启动".to_string(), palette.warning, true),
        Status::Streaming => ("正在串流".to_string(), palette.success, false),
        Status::Reconnecting(_) => ("正在重连".to_string(), palette.warning, true),
        Status::Error(_) => ("出现错误".to_string(), palette.danger, false),
    }
}

impl App {
    fn handle_tray_and_close(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray_rx.try_recv() {
            match action {
                tray::TrayAction::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                }
                tray::TrayAction::Quit => {
                    self.force_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        if !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.force_quit || !self.tray.available() {
            // Let eframe close normally.
            return;
        }
        if !self.settings.tray_choice_made {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_prompt_open = true;
            ctx.request_repaint();
        } else if self.settings.close_to_tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_to_tray(ctx);
        }
    }

    fn hide_to_tray(&mut self, ctx: &egui::Context) {
        self.close_prompt_open = false;
        self.settings.save();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        append_log(&self.shared, ctx, "已最小化到系统托盘，双击托盘图标可恢复");
    }

    fn close_prompt(&mut self, ctx: &egui::Context) {
        if !self.close_prompt_open {
            return;
        }
        let palette = if ctx.theme() == egui::Theme::Dark {
            theme::Palette::dark()
        } else {
            theme::Palette::light()
        };
        let mut choice: Option<bool> = None;
        #[allow(deprecated)]
        let modal_id = egui::Id::new("close_prompt_modal");
        egui::Modal::new(modal_id)
            .backdrop_color(egui::Color32::from_black_alpha(140))
            .frame(
                egui::Frame::NONE
                    .fill(palette.card)
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .corner_radius(egui::CornerRadius::same(18))
                    .inner_margin(egui::Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(380.0);
                ui.horizontal(|ui| {
                    let (icon, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(42.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(icon, egui::CornerRadius::same(12), palette.accent);
                    let center = icon.center();
                    let stroke = egui::Stroke::new(2.2, theme::with_alpha(palette.on_accent, 240));
                    ui.painter().line_segment(
                        [
                            egui::pos2(center.x, center.y - 9.0),
                            egui::pos2(center.x, center.y + 4.0),
                        ],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(center.x - 5.0, center.y - 1.0),
                            egui::pos2(center.x, center.y + 4.0),
                        ],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(center.x + 5.0, center.y - 1.0),
                            egui::pos2(center.x, center.y + 4.0),
                        ],
                        stroke,
                    );
                    ui.painter().line_segment(
                        [
                            egui::pos2(center.x - 8.0, center.y + 9.0),
                            egui::pos2(center.x + 8.0, center.y + 9.0),
                        ],
                        stroke,
                    );
                    ui.add_space(12.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("关闭 Phone Audio")
                                .size(19.0)
                                .strong()
                                .color(palette.text),
                        );
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new("串流可以继续在系统托盘中运行")
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                    });
                });

                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new("点击窗口关闭按钮后，你希望：")
                        .size(12.5)
                        .color(palette.text_dim),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "最小化到托盘", 168.0, true, false).clicked()
                    {
                        choice = Some(true);
                    }
                    ui.add_space(8.0);
                    let exit = ui.add(
                        egui::Button::new(
                            egui::RichText::new("退出应用")
                                .size(13.0)
                                .color(palette.text_dim),
                        )
                        .fill(palette.bg)
                        .stroke(egui::Stroke::new(1.0, palette.border))
                        .corner_radius(egui::CornerRadius::same(11))
                        .min_size(egui::vec2(120.0, 40.0)),
                    );
                    if exit.clicked() {
                        choice = Some(false);
                    }
                });
                ui.add_space(12.0);
                theme::subtle_label(ui, "之后可在顶部「设置」里随时修改");
            });
        match choice {
            Some(true) => {
                self.settings.close_to_tray = true;
                self.settings.tray_choice_made = true;
                self.settings.save();
                self.close_prompt_open = false;
                self.hide_to_tray(ctx);
            }
            Some(false) => {
                self.settings.close_to_tray = false;
                self.settings.tray_choice_made = true;
                self.settings.save();
                self.close_prompt_open = false;
                self.force_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }

    fn prepare_start(&mut self) {
        self.settings.serial = self.selected.clone().unwrap_or_default();
        self.settings.normalize();
        self.settings.save();
        lock_shared(&self.shared).settings = self.settings.clone();
    }

    fn header(&mut self, ui: &mut egui::Ui, palette: &theme::Palette, status: &Status) {
        let (status_text, status_color, pulse) = status_meta(status, palette);
        let mut new_theme = self.settings.theme.clone();
        let mut new_autostart = self.autostart_enabled;
        let mut new_close_to_tray = self.settings.close_to_tray;

        egui::Panel::top("header")
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(18, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Logo: accent square with a small white waveform.
                    let (logo, _) =
                        ui.allocate_exact_size(egui::Vec2::splat(36.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(logo, egui::CornerRadius::same(10), palette.accent);
                    for (index, height) in [8.0, 14.0, 19.0, 12.0].iter().enumerate() {
                        let x = logo.left() + 9.0 + index as f32 * 5.6;
                        let bar = egui::Rect::from_center_size(
                            egui::pos2(x, logo.center().y),
                            egui::vec2(2.6, *height),
                        );
                        ui.painter().rect_filled(
                            bar,
                            egui::CornerRadius::same(2),
                            theme::with_alpha(palette.on_accent, 235),
                        );
                    }
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("Phone Audio")
                            .size(21.0)
                            .strong()
                            .color(palette.text),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::ComboBox::from_id_salt("theme")
                            .width(104.0)
                            .selected_text(
                                egui::RichText::new(theme_label(&self.settings.theme))
                                    .size(11.5)
                                    .color(palette.text_dim),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut new_theme,
                                    "system".to_string(),
                                    "跟随系统",
                                );
                                ui.selectable_value(&mut new_theme, "dark".to_string(), "深色");
                                ui.selectable_value(&mut new_theme, "light".to_string(), "浅色");
                            });
                        ui.menu_button("设置", |ui| {
                            ui.checkbox(&mut new_autostart, "开机自启动");
                            ui.label(
                                egui::RichText::new("随系统登录自动启动 Phone Audio")
                                    .size(10.5)
                                    .color(palette.text_faint),
                            );
                            ui.separator();
                            if self.tray.available() {
                                ui.checkbox(&mut new_close_to_tray, "关闭时最小化到托盘");
                                ui.label(
                                    egui::RichText::new("点击窗口关闭按钮后继续在后台运行")
                                        .size(10.5)
                                        .color(palette.text_faint),
                                );
                            } else {
                                ui.label(
                                    egui::RichText::new("系统托盘不可用")
                                        .size(10.5)
                                        .color(palette.text_faint),
                                );
                            }
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!(
                                    "Phone Audio v{}",
                                    env!("CARGO_PKG_VERSION")
                                ))
                                .size(10.5)
                                .color(palette.text_faint),
                            );
                        });
                        ui.add_space(10.0);
                        let pill = theme::status_pill(ui, &status_text, status_color, pulse);
                        if let Status::Reconnecting(message) | Status::Error(message) = status {
                            pill.on_hover_text(message);
                        }
                    });
                });
            });

        if new_theme != self.settings.theme {
            self.settings.theme = new_theme;
            theme::apply(ui.ctx(), &self.settings.theme);
            self.settings.save();
        }

        if new_autostart != self.autostart_enabled {
            let ctx = ui.ctx().clone();
            match autostart::set_enabled(new_autostart) {
                Ok(()) => {
                    self.autostart_enabled = new_autostart;
                    append_log(
                        &self.shared,
                        &ctx,
                        if new_autostart {
                            "开机自启动：已开启"
                        } else {
                            "开机自启动：已关闭"
                        },
                    );
                }
                Err(error) => {
                    append_log(&self.shared, &ctx, format!("开机自启动设置失败：{error}"))
                }
            }
        }

        if new_close_to_tray != self.settings.close_to_tray {
            self.settings.close_to_tray = new_close_to_tray;
            self.settings.tray_choice_made = true;
            self.settings.save();
            append_log(
                &self.shared,
                ui.ctx(),
                if new_close_to_tray {
                    "关闭窗口：最小化到系统托盘"
                } else {
                    "关闭窗口：退出应用"
                },
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn device_card(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
        devices: &[adb::Device],
        selected_device: Option<&adb::Device>,
        running: bool,
        adb_ok: bool,
        detecting: bool,
        can_start: bool,
    ) {
        theme::card(ui, |ui| {
            // Everything on one row, vertically centered.
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 40.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    theme::section_title(ui, "设备", None);
                    if let Some(device) = selected_device {
                        let color = match device.state {
                            adb::DeviceState::Device => palette.success,
                            adb::DeviceState::Unauthorized => palette.warning,
                            adb::DeviceState::Offline => palette.text_dim,
                            adb::DeviceState::NoPermissions => palette.danger,
                            adb::DeviceState::Unknown => palette.text_dim,
                        };
                        ui.add_space(2.0);
                        theme::status_pill_compact(ui, device.state.label(), color);
                    }
                    ui.add_space(6.0);

                    let label = selected_device
                        .map(|device| format!("{}   ·   {}", device.model, device.serial))
                        .unwrap_or_else(|| {
                            if detecting {
                                "正在检测设备…".to_string()
                            } else {
                                "选择设备".to_string()
                            }
                        });

                    let reserved = 70.0 + 150.0 + 168.0 + 40.0;
                    let combo_width = (ui.available_width() - reserved).max(180.0);
                    let mut chosen = self.selected.clone();
                    egui::ComboBox::from_id_salt("device")
                        .width(combo_width)
                        .selected_text(egui::RichText::new(label).size(13.0).color(palette.text))
                        .show_ui(ui, |ui| {
                            for device in devices {
                                ui.selectable_value(
                                    &mut chosen,
                                    Some(device.serial.clone()),
                                    format!(
                                        "{}   ·   {}   ·   {}",
                                        device.model,
                                        device.serial,
                                        device.state.label()
                                    ),
                                );
                            }
                        });
                    if chosen != self.selected {
                        self.selected = chosen;
                        if let Some(serial) = &self.selected {
                            self.settings.serial = serial.clone();
                        }
                    }

                    if ui
                        .add_enabled(!running, egui::Button::new("刷新").small())
                        .clicked()
                    {
                        let _ = self.tx.send(Cmd::Refresh);
                    }
                    theme::switch(ui, &mut self.settings.auto_connect, "启动后自动串流");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, danger) = if running {
                            ("停止串流", true)
                        } else {
                            ("开始串流", false)
                        };
                        let response =
                            theme::primary_button(ui, label, 168.0, running || can_start, danger);
                        if response.clicked() {
                            if running {
                                let _ = self.tx.send(Cmd::Stop);
                            } else {
                                self.prepare_start();
                                let _ = self.tx.send(Cmd::Start);
                            }
                        }
                    });
                },
            );

            if devices.is_empty() {
                ui.add_space(6.0);
                theme::subtle_label(
                    ui,
                    if detecting {
                        "正在检测 scrcpy / adb …"
                    } else if !adb_ok {
                        "未找到 adb：请检查 bin/ 或 PATH"
                    } else {
                        "没有发现设备：请开启 USB 调试后插线，或使用「无线连接」"
                    },
                );
            }
        });
    }
    fn audio_card(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
        info: &scrcpy::Info,
        running: bool,
        min_height: f32,
    ) {
        theme::card_min_height(ui, min_height, |ui| {
            ui.horizontal(|ui| {
                theme::section_title(ui, "音频质量", None);
                if running {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::subtle_label(ui, "停止后可修改");
                    });
                }
            });
            ui.add_space(8.0);

            ui.add_enabled_ui(!running, |ui| {
                // Buffer delay: value + slider + presets.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("缓冲延迟")
                            .size(12.5)
                            .color(palette.text_dim),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{} ms", self.settings.buffer_ms))
                                .size(13.0)
                                .strong()
                                .color(palette.accent),
                        );
                    });
                });
                ui.spacing_mut().slider_width = ui.available_width();
                ui.add(egui::Slider::new(&mut self.settings.buffer_ms, 20..=200).show_value(false));
                let selected = if self.settings.buffer_ms <= 35 {
                    Some(0)
                } else if self.settings.buffer_ms <= 80 {
                    Some(1)
                } else {
                    Some(2)
                };
                if let Some(index) = theme::segmented(
                    ui,
                    &[
                        ("低延迟 35ms", selected == Some(0)),
                        (
                            "均衡 60ms",
                            selected == Some(1) && self.settings.buffer_ms != 35,
                        ),
                        (
                            "稳定 120ms",
                            selected == Some(2) && self.settings.buffer_ms != 60,
                        ),
                    ],
                ) {
                    self.settings.buffer_ms = [35, 60, 120][index];
                }

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("编码")
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                        let codec = self.settings.codec.clone();
                        let mut chosen = codec.clone();
                        egui::ComboBox::from_id_salt("codec")
                            .width(132.0)
                            .selected_text(codec_label(&codec))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut chosen,
                                    "opus".to_string(),
                                    "Opus（推荐）",
                                );
                                ui.selectable_value(&mut chosen, "aac".to_string(), "AAC（兼容）");
                                if info.flac_ok() {
                                    ui.selectable_value(
                                        &mut chosen,
                                        "flac".to_string(),
                                        "FLAC（无损）",
                                    );
                                }
                                ui.selectable_value(
                                    &mut chosen,
                                    "raw".to_string(),
                                    "PCM（仅 USB）",
                                );
                            });
                        if chosen != codec {
                            self.settings.codec = chosen;
                        }
                    });
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("码率")
                                .size(12.0)
                                .color(palette.text_dim),
                        );
                        if self.settings.codec == "raw" {
                            theme::subtle_label(ui, "PCM 不使用码率");
                        } else {
                            let mut kbps = self.settings.bitrate_kbps;
                            egui::ComboBox::from_id_salt("bitrate")
                                .width(112.0)
                                .selected_text(format!("{kbps} kbps"))
                                .show_ui(ui, |ui| {
                                    for value in [64u32, 96, 128, 192, 256] {
                                        ui.selectable_value(
                                            &mut kbps,
                                            value,
                                            format!("{value} kbps"),
                                        );
                                    }
                                });
                            self.settings.bitrate_kbps = kbps;
                        }
                    });
                });
            });
        });
    }

    fn source_card(
        &mut self,
        ui: &mut egui::Ui,
        info: &scrcpy::Info,
        running: bool,
        min_height: f32,
    ) {
        theme::card_min_height(ui, min_height, |ui| {
            theme::section_title(ui, "音源", Some("Android 11+"));
            ui.add_space(8.0);

            ui.add_enabled_ui(!running, |ui| {
                let source = self.settings.source.clone();
                let mut chosen = source.clone();
                let playback = self.settings.source == "playback";

                ui.horizontal(|ui| {
                    if playback && info.playback_ok() {
                        let switch_width = 190.0;
                        let width = (ui.available_width() - switch_width - 8.0).max(150.0);
                        egui::ComboBox::from_id_salt("source")
                            .width(width)
                            .selected_text(source_label(&source))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut chosen,
                                    "output".to_string(),
                                    "整机输出（手机静音）",
                                );
                                ui.add_enabled_ui(info.playback_ok(), |ui| {
                                    ui.selectable_value(
                                        &mut chosen,
                                        "playback".to_string(),
                                        "播放捕获（Android 13+）",
                                    );
                                });
                            });
                        theme::switch(ui, &mut self.settings.audio_dup, "保留手机外放");
                    } else {
                        let width = ui.available_width().max(150.0);
                        egui::ComboBox::from_id_salt("source")
                            .width(width)
                            .selected_text(source_label(&source))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut chosen,
                                    "output".to_string(),
                                    "整机输出（手机静音）",
                                );
                                ui.add_enabled_ui(info.playback_ok(), |ui| {
                                    ui.selectable_value(
                                        &mut chosen,
                                        "playback".to_string(),
                                        "播放捕获（Android 13+）",
                                    );
                                });
                            });
                    }
                });
                if chosen != source {
                    self.settings.source = chosen;
                }

                if self.settings.source == "playback" && !info.playback_ok() {
                    ui.add_space(6.0);
                    theme::subtle_label(ui, "「播放捕获」需要 scrcpy 2.6+ 且 Android 13+");
                }
            });
        });
    }
    fn output_card(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
        info: &scrcpy::Info,
        running: bool,
        output_devices: &[String],
        output_error: &Option<String>,
    ) {
        theme::card_min_height(ui, 152.0, |ui| {
            theme::section_title(ui, "电脑输出", None);
            ui.add_space(8.0);

            ui.add_enabled_ui(!running, |ui| {
                let current = self.settings.output_device.clone();
                let mut chosen = current.clone();
                let label = if current.is_empty() {
                    "跟随系统默认".to_string()
                } else {
                    current.clone()
                };
                let selectable = info.output_device_ok() || cfg!(target_os = "linux");
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("输出设备")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    let width = (ui.available_width() - 76.0).max(120.0);
                    ui.add_enabled_ui(selectable, |ui| {
                        egui::ComboBox::from_id_salt("output_device")
                            .width(width)
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut chosen, String::new(), "跟随系统默认");
                                for name in output_devices {
                                    ui.selectable_value(&mut chosen, name.clone(), name);
                                }
                            });
                    });
                    if ui
                        .add_enabled(!running && selectable, egui::Button::new("刷新").small())
                        .clicked()
                    {
                        let _ = self.tx.send(Cmd::RefreshAudio);
                    }
                });
                if chosen != current {
                    self.settings.output_device = chosen;
                }
            });

            if !info.output_device_ok() && cfg!(not(target_os = "linux")) {
                theme::subtle_label(ui, "当前 scrcpy 只能跟随系统默认输出（补丁版可任意选择）");
            } else if let Some(error) = output_error {
                ui.label(egui::RichText::new(error).size(11.0).color(palette.warning));
            }

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                theme::switch(ui, &mut self.settings.auto_reconnect, "断线自动重连");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::subtle_label(
                        ui,
                        if self.settings.auto_reconnect {
                            "1–10 秒退避"
                        } else {
                            "断开后不重试"
                        },
                    );
                });
            });
        });
    }

    fn wireless_card(&mut self, ui: &mut egui::Ui, palette: &theme::Palette, running: bool) {
        theme::card(ui, |ui| {
            let header = ui.horizontal(|ui| {
                theme::section_title(ui, "无线连接", Some("Android 11+"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if self.wireless_open {
                        "收起"
                    } else {
                        "展开"
                    };
                    ui.add(
                        egui::Button::new(
                            egui::RichText::new(text).size(11.5).color(palette.text_dim),
                        )
                        .frame(false),
                    )
                    .clicked()
                })
                .inner
            });
            if header.inner {
                self.wireless_open = !self.wireless_open;
            }

            if !self.wireless_open {
                return;
            }

            ui.add_space(8.0);
            ui.add_enabled_ui(!running, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("配对地址")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.pair_addr)
                            .hint_text("192.168.1.5:37123")
                            .desired_width(200.0),
                    );
                    ui.label(
                        egui::RichText::new("配对码")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.pair_code)
                            .hint_text("6 位数字")
                            .desired_width(96.0),
                    );
                    if ui.button("配对").clicked() {
                        self.settings.save();
                        let _ = self.tx.send(Cmd::Pair(
                            self.settings.pair_addr.clone(),
                            self.settings.pair_code.clone(),
                        ));
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("连接地址")
                            .size(12.0)
                            .color(palette.text_dim),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings.connect_addr)
                            .hint_text("192.168.1.5:5555")
                            .desired_width(200.0),
                    );
                    if ui.button("连接").clicked() {
                        self.settings.save();
                        let _ = self
                            .tx
                            .send(Cmd::Connect(self.settings.connect_addr.clone()));
                    }
                });
            });
            theme::subtle_label(
                ui,
                "手机：开发者选项 → 无线调试 → 使用配对码配对；配对后自动走 mDNS 连接",
            );
        });
    }

    fn log_panel(
        &mut self,
        ui: &mut egui::Ui,
        palette: &theme::Palette,
        log: &std::collections::VecDeque<String>,
        info: &scrcpy::Info,
    ) {
        egui::Panel::bottom("log")
            .resizable(true)
            .default_size(150.0)
            .min_size(110.0)
            .frame(
                egui::Frame::NONE
                    .fill(palette.panel)
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("运行日志")
                            .size(13.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(4.0);
                    if ui.small_button("清空").clicked() {
                        lock_shared(&self.shared).log.clear();
                    }
                    if ui.small_button("复制").clicked() {
                        let text = log.iter().cloned().collect::<Vec<_>>().join("\n");
                        ui.ctx().copy_text(text);
                    }
                    ui.add_space(4.0);
                    theme::switch(ui, &mut self.auto_scroll, "自动滚动");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("诊断", |ui| {
                            if ui.button("打开配置目录").clicked() {
                                open_config_dir();
                                ui.close();
                            }
                            if ui.button("声音设置").clicked() {
                                open_sound_settings();
                                ui.close();
                            }
                            if ui.button("复制诊断信息").clicked() {
                                let diagnostics = build_diagnostics(&self.shared, &self.settings);
                                ui.ctx().copy_text(diagnostics);
                                let ctx = ui.ctx().clone();
                                append_log(&self.shared, &ctx, "诊断信息已复制到剪贴板");
                                ui.close();
                            }
                        });
                        ui.add_space(8.0);
                        theme::subtle_label(
                            ui,
                            format!("scrcpy {}.{}.{}", info.major, info.minor, info.patch),
                        );
                    });
                });

                ui.add_space(6.0);
                theme::inset(ui, palette.log_bg, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(self.auto_scroll)
                        .show(ui, |ui| {
                            if log.is_empty() {
                                theme::subtle_label(ui, "暂无日志");
                            }
                            for line in log {
                                ui.label(
                                    egui::RichText::new(line)
                                        .monospace()
                                        .size(11.5)
                                        .color(theme::log_color(line, palette)),
                                );
                            }
                        });
                });
            });
    }
}

fn codec_label(codec: &str) -> &'static str {
    match codec {
        "aac" => "AAC（兼容）",
        "flac" => "FLAC（无损，码率高）",
        "raw" => "PCM（仅 USB，最低延迟）",
        _ => "Opus（推荐）",
    }
}

fn source_label(source: &str) -> &'static str {
    match source {
        "playback" => "播放捕获（Android 13+/scrcpy 2.6+）",
        _ => "整机输出（手机静音）",
    }
}

fn open_config_dir() {
    if let Some(dir) = settings::config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        #[cfg(target_os = "windows")]
        {
            let _ = std::process::Command::new("explorer").arg(&dir).spawn();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
        }
    }
}

fn open_sound_settings() {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg("ms-settings:sound")
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let candidates: &[(&str, &[&str])] = &[
            ("pavucontrol", &[]),
            ("gnome-control-center", &["sound"]),
            ("systemsettings", &["kcm_pulseaudio"]),
        ];
        for &(program, args) in candidates {
            if std::process::Command::new(program)
                .args(args)
                .spawn()
                .is_ok()
            {
                break;
            }
        }
    }
}

fn build_diagnostics(shared: &Arc<Mutex<Shared>>, settings: &Settings) -> String {
    let shared = lock_shared(shared);
    let mut out = String::new();
    out.push_str(&format!("Phone Audio v{}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!(
        "OS: {} / {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!("scrcpy: {}\n", shared.scrcpy.version_line));
    out.push_str(&format!(
        "scrcpy caps: audio_ok={} playback_ok={} flac_ok={} output_device_ok={}\n",
        shared.scrcpy.audio_ok(),
        shared.scrcpy.playback_ok(),
        shared.scrcpy.flac_ok(),
        shared.scrcpy.output_device_ok()
    ));
    out.push_str(&format!("adb: {}\n", shared.adb_version));
    out.push_str(&format!("bin dir: {:?}\n", tools::bin_dir()));
    out.push_str(&format!(
        "settings: {}\n",
        serde_json::to_string_pretty(settings).unwrap_or_default()
    ));
    out.push_str(&format!("output devices: {:?}\n", shared.output_devices));
    out.push_str("devices:\n");
    for device in &shared.devices {
        out.push_str(&format!(
            "  {}  {}  {}\n",
            device.serial,
            device.model,
            device.state.label()
        ));
    }
    out.push_str("log:\n");
    for line in &shared.log {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn append_log(shared: &Arc<Mutex<Shared>>, ctx: &egui::Context, line: impl Into<String>) {
    {
        let mut shared = lock_shared(shared);
        shared.log.push_back(line.into());
        while shared.log.len() > 800 {
            shared.log.pop_front();
        }
    }
    ctx.request_repaint();
}

fn set_status(shared: &Arc<Mutex<Shared>>, ctx: &egui::Context, status: Status) {
    lock_shared(shared).status = status;
    ctx.request_repaint();
}

fn last_error_line(shared: &Arc<Mutex<Shared>>) -> Option<String> {
    let shared = lock_shared(shared);
    shared
        .log
        .iter()
        .rev()
        .find(|line| line.contains("ERROR") || line.contains("WARN"))
        .map(|line| {
            let text = line.trim();
            if text.chars().count() > 140 {
                let short: String = text.chars().take(140).collect();
                format!("{short}…")
            } else {
                text.to_string()
            }
        })
}

/// A log line proving that scrcpy really started the audio stream (as opposed
/// to the process merely being alive).
fn looks_like_audio_ready(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("audio")
        && !lower.contains("error")
        && !lower.contains("warn")
        && !lower.contains("fail")
        && !lower.contains("disabled")
}

fn spawn_log_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
    audio_seen: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer);
                    let line = line.trim_end_matches(['\r', '\n']).to_string();
                    if looks_like_audio_ready(&line) {
                        audio_seen.store(true, Ordering::Relaxed);
                    }
                    append_log(&shared, &ctx, line);
                }
                Err(_) => break,
            }
        }
    });
}

fn start_child(
    shared: &Arc<Mutex<Shared>>,
    ctx: &egui::Context,
    audio_seen: Arc<AtomicBool>,
    shutting_down: &Arc<AtomicBool>,
) -> Result<Child, String> {
    if shutting_down.load(Ordering::SeqCst) {
        return Err("应用正在退出".to_string());
    }
    let (settings, info) = {
        let shared = lock_shared(shared);
        (shared.settings.clone(), shared.scrcpy.clone())
    };
    if settings.serial.trim().is_empty() {
        return Err("没有选择设备".to_string());
    }

    let scrcpy_path = tools::find_scrcpy();
    let adb = tools::find_adb();

    if scrcpy_path.components().count() > 1 && !scrcpy_path.exists() {
        return Err(format!("找不到 scrcpy：{}", scrcpy_path.display()));
    }
    if adb.components().count() > 1 && !adb.exists() {
        return Err(format!("找不到 adb：{}", adb.display()));
    }

    // Cheap Android version pre-flight; failure to read it is not fatal.
    match adb::shell_getprop(&adb, &settings.serial, "ro.build.version.sdk") {
        Ok(value) => {
            if let Ok(sdk) = value.trim().parse::<u32>() {
                if sdk < 30 {
                    return Err(
                        "手机需要 Android 11+：scrcpy 的音频转发从 Android 11 开始".to_string()
                    );
                }
                if settings.source == "playback" && sdk < 33 {
                    return Err("「播放捕获」需要 Android 13+，请改用「整机输出」".to_string());
                }
            }
        }
        Err(error) => append_log(
            shared,
            ctx,
            format!("读取 Android 版本失败（继续尝试）：{error}"),
        ),
    }

    let args = scrcpy::build_args(&settings, info.as_ref())?;

    let mut cmd = tools::command(&scrcpy_path);
    cmd.args(&args)
        .env("ADB", &adb)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(server) = tools::find_server() {
        cmd.env("SCRCPY_SERVER_PATH", server);
    }
    for (key, value) in scrcpy::child_env(&settings, info.as_ref()) {
        cmd.env(key, value);
    }

    if shutting_down.load(Ordering::SeqCst) {
        return Err("应用正在退出".to_string());
    }

    append_log(
        shared,
        ctx,
        format!("$ {} {}", scrcpy_path.display(), args.join(" ")),
    );

    let mut child = cmd
        .spawn()
        .map_err(|error| format!("无法启动 scrcpy（{}）：{error}", scrcpy_path.display()))?;

    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, shared.clone(), ctx.clone(), audio_seen.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, shared.clone(), ctx.clone(), audio_seen);
    }

    Ok(child)
}

fn refresh_environment(shared: &Arc<Mutex<Shared>>, ctx: &egui::Context) {
    let info = scrcpy::Info::detect();
    let summary = info.summary();
    let (adb_version, adb_ok) =
        match tools::run_timeout(&tools::find_adb(), &["version"], Duration::from_secs(5)) {
            Ok(text) => (first_line(&text), true),
            Err(error) => (first_line(&error), false),
        };
    {
        let mut shared = lock_shared(shared);
        shared.scrcpy = Arc::new(info);
        shared.adb_version = adb_version;
        shared.adb_ok = adb_ok;
        shared.detecting = false;
    }
    append_log(shared, ctx, format!("scrcpy / adb 检测完成：{summary}"));
    refresh_audio_devices(shared, ctx);
}

fn refresh_audio_devices(shared: &Arc<Mutex<Shared>>, ctx: &egui::Context) {
    let info = lock_shared(shared).scrcpy.clone();
    match scrcpy::list_audio_output_devices(&info) {
        Ok(devices) => {
            let mut shared = lock_shared(shared);
            shared.output_devices = devices;
            shared.output_error = None;
        }
        Err(error) => {
            let mut shared = lock_shared(shared);
            shared.output_devices.clear();
            shared.output_error = Some(error);
        }
    }
    ctx.request_repaint();
}

fn handle_pair(
    shared: &Arc<Mutex<Shared>>,
    ctx: &egui::Context,
    child_slot: &ChildSlot,
    addr: &str,
    code: &str,
) {
    let addr = addr.trim();
    let code = code.trim();
    if addr.is_empty() || code.is_empty() {
        append_log(shared, ctx, "配对地址或配对码为空");
        return;
    }
    if lock_child(child_slot).is_some() {
        append_log(shared, ctx, "串流中请先停止再配对");
        return;
    }

    let adb_path = tools::find_adb();
    append_log(shared, ctx, format!("正在配对 {addr} …"));
    match adb::pair(&adb_path, addr, code) {
        Ok(output) => {
            append_log(shared, ctx, output);
            let expect_ip = addr.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(addr);
            let mut connected = false;
            for _ in 0..6 {
                if let Some(service) = adb::find_connect_service(&adb_path, Some(expect_ip)) {
                    match adb::connect(&adb_path, &service) {
                        Ok(output) => append_log(shared, ctx, output),
                        Err(error) => append_log(shared, ctx, error),
                    }
                    connected = true;
                    break;
                }
                thread::sleep(Duration::from_millis(500));
            }
            if !connected {
                append_log(
                    shared,
                    ctx,
                    "配对成功；未查到 mDNS 服务，请用「连接地址」手动连接",
                );
            }
        }
        Err(error) => append_log(shared, ctx, format!("配对失败：{error}")),
    }
}

fn handle_connect(
    shared: &Arc<Mutex<Shared>>,
    ctx: &egui::Context,
    child_slot: &ChildSlot,
    addr: &str,
) {
    let addr = addr.trim();
    if addr.is_empty() {
        append_log(shared, ctx, "连接地址为空");
        return;
    }
    if lock_child(child_slot).is_some() {
        append_log(shared, ctx, "串流中请先停止再连接");
        return;
    }
    match adb::connect(&tools::find_adb(), addr) {
        Ok(output) => append_log(shared, ctx, output),
        Err(error) => append_log(shared, ctx, format!("连接失败：{error}")),
    }
}

fn engine(
    rx: Receiver<Cmd>,
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
    child_slot: ChildSlot,
    shutdown_tx: Sender<()>,
    shutting_down: Arc<AtomicBool>,
) {
    let mut retry_at: Option<Instant> = None;
    let mut retry_delay = 1u64;
    let mut starting_since: Option<Instant> = None;
    let mut audio_seen = Arc::new(AtomicBool::new(false));
    let mut manual_stop = false;
    let mut start_requested = false;
    let mut next_poll = Instant::now();
    let mut auto_connect_attempted = false;
    let mut last_mdns_attempt: Option<Instant> = None;
    let mut refresh_env = true;

    loop {
        // Drain commands first so a Stop always wins over an earlier Start.
        loop {
            match rx.try_recv() {
                Ok(Cmd::Start) => {
                    auto_connect_attempted = true;
                    if lock_child(&child_slot).is_some() {
                        append_log(&shared, &ctx, "已在串流中，忽略重复的「开始」");
                    } else {
                        start_requested = true;
                    }
                }
                Ok(Cmd::Stop) => {
                    start_requested = false;
                    auto_connect_attempted = true;
                    manual_stop = true;
                    retry_at = None;
                    retry_delay = 1;
                    starting_since = None;
                    if let Some(mut child) = lock_child(&child_slot).take() {
                        tools::stop_child(&mut child);
                    }
                    set_status(&shared, &ctx, Status::Idle);
                    append_log(&shared, &ctx, "已停止");
                }
                Ok(Cmd::Refresh) => {
                    refresh_env = true;
                    next_poll = Instant::now();
                }
                Ok(Cmd::RefreshAudio) => refresh_audio_devices(&shared, &ctx),
                Ok(Cmd::Pair(addr, code)) => {
                    handle_pair(&shared, &ctx, &child_slot, &addr, &code);
                    next_poll = Instant::now();
                }
                Ok(Cmd::Connect(addr)) => {
                    handle_connect(&shared, &ctx, &child_slot, &addr);
                    next_poll = Instant::now();
                }
                Ok(Cmd::Shutdown) => {
                    if let Some(mut child) = lock_child(&child_slot).take() {
                        tools::stop_child(&mut child);
                    }
                    set_status(&shared, &ctx, Status::Idle);
                    let _ = shutdown_tx.send(());
                    return;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if let Some(mut child) = lock_child(&child_slot).take() {
                        tools::stop_child(&mut child);
                    }
                    let _ = shutdown_tx.send(());
                    return;
                }
            }
        }

        if refresh_env {
            refresh_env = false;
            refresh_environment(&shared, &ctx);
        }

        if Instant::now() >= next_poll {
            let adb_path = tools::find_adb();
            let listed = adb::list_devices(&adb_path);
            if let Err(error) = &listed {
                append_log(&shared, &ctx, format!("adb devices 失败：{error}"));
            }
            let mut devices = listed.unwrap_or_else(|_| lock_shared(&shared).devices.clone());

            // Wireless debugging gets a new port after a reboot / toggle, so the
            // previously saved ip:port can be stale. adb mDNS still advertises
            // the service name and current port: reconnect to it automatically.
            let saved = lock_shared(&shared).settings.serial.clone();
            let missing = !saved.is_empty() && resolve_saved_device(&saved, &devices).is_none();
            let discovery_due = last_mdns_attempt
                .is_none_or(|attempt| attempt.elapsed() >= Duration::from_secs(10));
            if (devices.is_empty() || missing) && discovery_due {
                last_mdns_attempt = Some(Instant::now());
                let endpoints = adb::connect_mdns_services(&adb_path);
                if !endpoints.is_empty() {
                    append_log(
                        &shared,
                        &ctx,
                        format!("无线调试发现并尝试连接：{}", endpoints.join(", ")),
                    );
                    if let Ok(refreshed) = adb::list_devices(&adb_path) {
                        devices = refreshed;
                    }
                }
            }
            lock_shared(&shared).devices = devices.clone();

            if !auto_connect_attempted {
                let auto_connect = lock_shared(&shared).settings.auto_connect;
                if auto_connect && lock_child(&child_slot).is_none() {
                    if let Some(device) = resolve_saved_device(&saved, &devices)
                        .filter(|device| device.state == adb::DeviceState::Device)
                        .cloned()
                    {
                        auto_connect_attempted = true;
                        if device.serial != saved {
                            lock_shared(&shared).settings.serial = device.serial.clone();
                        }
                        append_log(&shared, &ctx, format!("自动连接上次设备 {}", device.serial));
                        start_requested = true;
                    }
                }
            }
            next_poll = Instant::now() + Duration::from_secs(2);
            ctx.request_repaint();
        }

        if start_requested
            && lock_child(&child_slot).is_none()
            && !shutting_down.load(Ordering::SeqCst)
        {
            {
                let devices = lock_shared(&shared).devices.clone();
                let saved = lock_shared(&shared).settings.serial.clone();
                if let Some(device) = resolve_saved_device(&saved, &devices) {
                    if device.serial != saved {
                        lock_shared(&shared).settings.serial = device.serial.clone();
                    }
                }
            }
            start_requested = false;
            manual_stop = false;
            retry_at = None;
            retry_delay = 1;
            audio_seen = Arc::new(AtomicBool::new(false));
            set_status(&shared, &ctx, Status::Starting);
            match start_child(&shared, &ctx, audio_seen.clone(), &shutting_down) {
                Ok(process) => {
                    *lock_child(&child_slot) = Some(process);
                    starting_since = Some(Instant::now());
                }
                Err(error) => {
                    append_log(&shared, &ctx, format!("启动失败：{error}"));
                    set_status(&shared, &ctx, Status::Error(error));
                    starting_since = None;
                }
            }
        }

        // Child exit detection.
        let mut exit_code: Option<Option<i32>> = None;
        let mut wait_error: Option<String> = None;
        {
            let mut slot = lock_child(&child_slot);
            if let Some(child) = slot.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => exit_code = Some(status.code()),
                    Ok(None) => {}
                    Err(error) => wait_error = Some(error.to_string()),
                }
            }
        }

        if let Some(error) = wait_error {
            if let Some(mut child) = lock_child(&child_slot).take() {
                tools::stop_child(&mut child);
            }
            starting_since = None;
            set_status(
                &shared,
                &ctx,
                Status::Error(format!("等待 scrcpy 失败：{error}")),
            );
        }

        if let Some(code) = exit_code {
            lock_child(&child_slot).take();
            starting_since = None;
            append_log(&shared, &ctx, format!("scrcpy 已退出（code={code:?}）"));
            if manual_stop {
                manual_stop = false;
                set_status(&shared, &ctx, Status::Idle);
            } else if code == Some(2) {
                let auto_reconnect = lock_shared(&shared).settings.auto_reconnect;
                if auto_reconnect {
                    set_status(
                        &shared,
                        &ctx,
                        Status::Reconnecting(format!("设备断开，{retry_delay} 秒后重试")),
                    );
                    retry_at = Some(Instant::now() + Duration::from_secs(retry_delay));
                    retry_delay = (retry_delay * 2).min(10);
                } else {
                    set_status(
                        &shared,
                        &ctx,
                        Status::Error("设备已断开（自动重连已关闭）".to_string()),
                    );
                }
            } else if code == Some(0) {
                set_status(&shared, &ctx, Status::Idle);
            } else {
                let summary = last_error_line(&shared).unwrap_or_else(|| "查看日志".to_string());
                set_status(
                    &shared,
                    &ctx,
                    Status::Error(format!(
                        "scrcpy 启动/连接失败（退出码 {code:?}）：{summary}"
                    )),
                );
            }
        }

        // Starting -> Streaming once the audio stream is confirmed (or after a
        // short grace period, for builds whose logs differ).
        if let Some(since) = starting_since {
            if lock_child(&child_slot).is_some() {
                let confirmed = audio_seen.load(Ordering::Relaxed);
                if confirmed || since.elapsed() >= Duration::from_secs(4) {
                    starting_since = None;
                    append_log(
                        &shared,
                        &ctx,
                        if confirmed {
                            "音频已开始"
                        } else {
                            "未捕获到音频启动日志，按运行中处理；若无声音请查看日志"
                        },
                    );
                    set_status(&shared, &ctx, Status::Streaming);
                }
            } else {
                starting_since = None;
            }
        }

        // Automatic reconnect.
        if let Some(at) = retry_at {
            if Instant::now() >= at {
                let auto_reconnect = lock_shared(&shared).settings.auto_reconnect;
                if !auto_reconnect {
                    retry_at = None;
                    set_status(
                        &shared,
                        &ctx,
                        Status::Error("设备已断开（自动重连已关闭）".to_string()),
                    );
                } else {
                    let serial = lock_shared(&shared).settings.serial.clone();
                    let adb_path = tools::find_adb();
                    let mut devices = adb::list_devices(&adb_path).unwrap_or_default();
                    let mut resolved = resolve_saved_device(&serial, &devices)
                        .filter(|device| device.state == adb::DeviceState::Device)
                        .cloned();

                    if resolved.is_none() {
                        let discovery_due = last_mdns_attempt
                            .is_none_or(|attempt| attempt.elapsed() >= Duration::from_secs(10));
                        if discovery_due {
                            last_mdns_attempt = Some(Instant::now());
                            let endpoints = adb::connect_mdns_services(&adb_path);
                            if !endpoints.is_empty() {
                                append_log(
                                    &shared,
                                    &ctx,
                                    format!("无线调试重新发现：{}", endpoints.join(", ")),
                                );
                                if let Ok(refreshed) = adb::list_devices(&adb_path) {
                                    devices = refreshed;
                                }
                            }
                        }
                        resolved = resolve_saved_device(&serial, &devices)
                            .filter(|device| device.state == adb::DeviceState::Device)
                            .cloned();
                    }

                    if let Some(device) = resolved {
                        if device.serial != serial {
                            lock_shared(&shared).settings.serial = device.serial.clone();
                        }
                        audio_seen = Arc::new(AtomicBool::new(false));
                        set_status(&shared, &ctx, Status::Starting);
                        match start_child(&shared, &ctx, audio_seen.clone(), &shutting_down) {
                            Ok(process) => {
                                *lock_child(&child_slot) = Some(process);
                                starting_since = Some(Instant::now());
                                retry_at = None;
                                retry_delay = 1;
                            }
                            Err(error) => {
                                append_log(&shared, &ctx, format!("重连失败：{error}"));
                                retry_delay = (retry_delay * 2).min(10);
                                retry_at = Some(Instant::now() + Duration::from_secs(retry_delay));
                                set_status(
                                    &shared,
                                    &ctx,
                                    Status::Reconnecting(format!("{retry_delay} 秒后重试")),
                                );
                            }
                        }
                    } else {
                        append_log(&shared, &ctx, format!("等待设备 {serial} 重新出现…"));
                        retry_at = Some(Instant::now() + Duration::from_secs(2));
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(120));
    }
}
