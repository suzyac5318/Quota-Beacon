use tauri::AppHandle;

// Native Windows backdrop and window-region APIs operate on the rectangular
// HWND. Quota Beacon instead relies on Tauri's transparent window plus the
// rounded React surface so pixels outside the card stay fully transparent.
pub fn apply_window_materials(_app: &AppHandle) {}

pub fn animate_widget_region(_app: AppHandle, _expanded: bool) {}
