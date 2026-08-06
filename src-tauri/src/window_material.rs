use tauri::AppHandle;

#[cfg(target_os = "windows")]
use tauri::{Manager, WebviewWindow};

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};

#[cfg(target_os = "windows")]
static CLIP_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "windows")]
static CLIP_PROGRESS: AtomicU32 = AtomicU32::new(0);

#[cfg(target_os = "windows")]
fn apply_desktop_acrylic(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as _,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            (&backdrop as *const _) as _,
            std::mem::size_of_val(&backdrop) as u32,
        )
    };
    if result < 0 {
        return Err(format!(
            "DwmSetWindowAttribute failed with HRESULT {result:#010x}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_rounded_region(window: &WebviewWindow, inset: f64, radius: f64) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    let inset = (inset * scale).round() as i32;
    let diameter = (radius * scale * 2.0).round().max(1.0) as i32;
    let right = size.width as i32 - inset + 1;
    let bottom = size.height as i32 - inset + 1;
    let region = unsafe { CreateRoundRectRgn(inset, inset, right, bottom, diameter, diameter) };
    if region.is_null() {
        return Err("CreateRoundRectRgn failed".to_string());
    }
    let result = unsafe { SetWindowRgn(hwnd.0 as _, region, 1) };
    if result == 0 {
        unsafe { DeleteObject(region as _) };
        return Err("SetWindowRgn failed".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_widget_region(window: &WebviewWindow, progress: f64) -> Result<(), String> {
    let progress = progress.clamp(0.0, 1.0);
    let inset = 10.0 * (1.0 - progress);
    let radius = 28.0 + 10.0 * progress;
    apply_rounded_region(window, inset, radius)
}

#[cfg(target_os = "windows")]
pub fn animate_widget_region(app: AppHandle, expanded: bool) {
    let generation = CLIP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let start = CLIP_PROGRESS.load(Ordering::SeqCst) as f64 / 1000.0;
    let target = if expanded { 1.0 } else { 0.0 };
    let duration_ms = if expanded { 400 } else { 280 };
    let steps = duration_ms / 16;
    tauri::async_runtime::spawn(async move {
        for step in 0..=steps {
            if CLIP_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let linear = step as f64 / steps as f64;
            let eased = linear * linear * (3.0 - 2.0 * linear);
            let progress = start + (target - start) * eased;
            CLIP_PROGRESS.store((progress * 1000.0).round() as u32, Ordering::SeqCst);
            if let Some(window) = app.get_webview_window("widget") {
                if let Err(error) = apply_widget_region(&window, progress) {
                    eprintln!("widget acrylic clipping failed: {error}");
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        }
    });
}

#[cfg(target_os = "windows")]
pub fn apply_window_materials(app: &AppHandle) {
    for label in ["widget", "palette", "palette-editor"] {
        let Some(window) = app.get_webview_window(label) else {
            eprintln!("desktop acrylic skipped for {label}: window missing");
            continue;
        };
        if let Err(error) = apply_desktop_acrylic(&window) {
            eprintln!("desktop acrylic unavailable for {label}: {error}");
        }
        let region_result = if label == "widget" {
            let scale = window.scale_factor().unwrap_or(1.0);
            let expanded = window
                .outer_size()
                .map(|size| size.width as f64 / scale > 120.0)
                .unwrap_or(false);
            let progress = if expanded { 1.0 } else { 0.0 };
            CLIP_PROGRESS.store((progress * 1000.0) as u32, Ordering::SeqCst);
            apply_widget_region(&window, progress)
        } else {
            apply_rounded_region(&window, 0.0, 24.0)
        };
        if let Err(error) = region_result {
            eprintln!("desktop acrylic clipping unavailable for {label}: {error}");
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_window_materials(_app: &AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn animate_widget_region(_app: AppHandle, _expanded: bool) {}
