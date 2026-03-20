use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    AppHandle, Manager, PhysicalSize, Runtime, Size, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::errors::AppResult;

pub const SETTINGS_MENU_ID: &str = "open-settings";

const MAIN_WINDOW_LABEL: &str = "main";
const SETTINGS_WINDOW_LABEL: &str = "settings";
const MAIN_WINDOW_TARGET_WIDTH: u32 = 1180;
const MAIN_WINDOW_TARGET_HEIGHT: u32 = 860;
const MAIN_WINDOW_MIN_WIDTH: u32 = 880;
const MAIN_WINDOW_MIN_HEIGHT: u32 = 640;
const MAIN_WINDOW_WORK_AREA_PADDING: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MainWindowBounds {
    width: u32,
    height: u32,
    min_width: u32,
    min_height: u32,
}

pub fn runtime_source_label() -> &'static str {
    if cfg!(feature = "custom-protocol") {
        "embedded-dist"
    } else {
        "dev-server"
    }
}

pub fn ensure_main_window(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        focus_window(&window)?;
        apply_main_window_bounds(&window)?;
        let _ = window.center();

        match window.url() {
            Ok(url) => log_windowing(
                "main-window-focused",
                &format!(
                    "label={MAIN_WINDOW_LABEL} source={} url={url}",
                    runtime_source_label()
                ),
            ),
            Err(error) => log_windowing(
                "main-window-focused",
                &format!(
                    "label={MAIN_WINDOW_LABEL} source={} url=<unavailable> error={error}",
                    runtime_source_label()
                ),
            ),
        }
    } else {
        log_windowing(
            "main-window-missing",
            &format!(
                "label={MAIN_WINDOW_LABEL} source={}",
                runtime_source_label()
            ),
        );
    }

    Ok(())
}

pub fn open_settings_window(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        focus_window(&window)?;
        log_window_url("settings-window-focused", &window);
        return Ok(());
    }

    let window = WebviewWindowBuilder::new(
        &app,
        SETTINGS_WINDOW_LABEL,
        WebviewUrl::App("settings.html".into()),
    )
    .title("new_stock Settings")
    .inner_size(720.0, 560.0)
    .min_inner_size(640.0, 520.0)
    .build()?;
    focus_window(&window)?;
    log_window_url("settings-window-created", &window);
    Ok(())
}

pub fn close_settings_window(app: AppHandle) -> AppResult<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW_LABEL) {
        window.close()?;
        log_windowing(
            "settings-window-closed",
            &format!("label={SETTINGS_WINDOW_LABEL}"),
        );
    }
    Ok(())
}

pub fn build_app_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let menu = Menu::default(app)?;

    #[cfg(target_os = "macos")]
    {
        let settings_item = MenuItem::with_id(
            app,
            SETTINGS_MENU_ID,
            "Settings…",
            true,
            Some("CmdOrCtrl+Comma"),
        )?;
        let separator = PredefinedMenuItem::separator(app)?;

        for item in menu.items()? {
            if let Some(submenu) = item.as_submenu() {
                if submenu.text()? == "File" {
                    submenu.append_items(&[&separator, &settings_item])?;
                    break;
                }
            }
        }
    }

    Ok(menu)
}

fn focus_window(window: &WebviewWindow) -> AppResult<()> {
    if window.is_minimized()? {
        window.unminimize()?;
    }

    if !window.is_visible()? {
        window.show()?;
    }

    window.set_focus()?;
    Ok(())
}

fn apply_main_window_bounds(window: &WebviewWindow) -> AppResult<()> {
    let monitor = window.current_monitor()?.or(window.primary_monitor()?);
    let Some(monitor) = monitor else {
        return Ok(());
    };

    let work_area = monitor.work_area();
    let bounds = resolve_main_window_bounds(work_area.size.width, work_area.size.height);
    window.set_min_size(Some(Size::Physical(PhysicalSize::new(
        bounds.min_width,
        bounds.min_height,
    ))))?;

    let current_size = window.outer_size()?;
    if current_size.width < bounds.min_width
        || current_size.height < bounds.min_height
        || current_size.width > bounds.width
        || current_size.height > bounds.height
    {
        window.set_size(Size::Physical(PhysicalSize::new(
            bounds.width,
            bounds.height,
        )))?;
    }

    Ok(())
}

fn resolve_main_window_bounds(available_width: u32, available_height: u32) -> MainWindowBounds {
    let (width, min_width) = resolve_dimension_bounds(
        available_width,
        MAIN_WINDOW_TARGET_WIDTH,
        MAIN_WINDOW_MIN_WIDTH,
    );
    let (height, min_height) = resolve_dimension_bounds(
        available_height,
        MAIN_WINDOW_TARGET_HEIGHT,
        MAIN_WINDOW_MIN_HEIGHT,
    );

    MainWindowBounds {
        width,
        height,
        min_width,
        min_height,
    }
}

fn resolve_dimension_bounds(available: u32, target: u32, min: u32) -> (u32, u32) {
    let safe_available = available.saturating_sub(MAIN_WINDOW_WORK_AREA_PADDING * 2);

    if safe_available == 0 {
        let fallback = available.max(1);
        return (fallback, fallback);
    }

    let resolved = target.min(safe_available);
    (resolved, min.min(resolved))
}

fn log_window_url(stage: &str, window: &WebviewWindow) {
    match window.url() {
        Ok(url) => log_windowing(stage, &format!("label={} url={url}", window.label())),
        Err(error) => log_windowing(
            stage,
            &format!("label={} url=<unavailable> error={error}", window.label()),
        ),
    }
}

fn log_windowing(stage: &str, detail: &str) {
    eprintln!("[windowing] {stage} {detail}");
}

#[cfg(test)]
mod tests {
    #[test]
    fn main_window_bounds_shrink_to_small_work_areas() {
        let bounds = super::resolve_main_window_bounds(960, 700);

        assert_eq!(bounds.width, 896);
        assert_eq!(bounds.height, 636);
        assert_eq!(bounds.min_width, super::MAIN_WINDOW_MIN_WIDTH);
        assert_eq!(bounds.min_height, bounds.height);
    }

    #[test]
    fn main_window_bounds_keep_desktop_target_on_large_screens() {
        let bounds = super::resolve_main_window_bounds(1728, 1117);

        assert_eq!(bounds.width, super::MAIN_WINDOW_TARGET_WIDTH);
        assert_eq!(bounds.height, super::MAIN_WINDOW_TARGET_HEIGHT);
        assert_eq!(bounds.min_width, super::MAIN_WINDOW_MIN_WIDTH);
        assert_eq!(bounds.min_height, super::MAIN_WINDOW_MIN_HEIGHT);
    }

    #[test]
    fn runtime_source_defaults_to_embedded_assets_for_cargo_run() {
        assert_eq!(super::runtime_source_label(), "embedded-dist");
    }

    #[test]
    fn settings_menu_identifier_is_stable() {
        assert_eq!(super::SETTINGS_MENU_ID, "open-settings");
    }
}
