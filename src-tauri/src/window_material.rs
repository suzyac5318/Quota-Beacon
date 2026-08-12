use tauri::AppHandle;

#[cfg(target_os = "windows")]
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering},
    Mutex, OnceLock,
};
#[cfg(target_os = "windows")]
use tauri::Manager;
#[cfg(target_os = "windows")]
use windows::{
    Foundation::{IPropertyValue, PropertyValue},
    Graphics::Effects::{
        IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl, IGraphicsEffect_Impl,
    },
    System::DispatcherQueueController,
    Win32::{
        Foundation::HWND as CompositionHwnd,
        Graphics::Direct2D::{
            CLSID_D2D1GaussianBlur, Common::D2D1_BORDER_MODE_HARD,
            D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED,
        },
        System::WinRT::{
            Composition::ICompositorDesktopInterop,
            CreateDispatcherQueueController, DispatcherQueueOptions,
            Graphics::Direct2D::{
                IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl,
                GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
            },
            DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
        },
    },
    UI::Composition::Desktop::DesktopWindowTarget,
    UI::Composition::{
        CompositionBackdropBrush, CompositionEffectBrush, CompositionEffectSourceParameter,
        CompositionGeometricClip, CompositionRoundedRectangleGeometry, Compositor, ContainerVisual,
        SpriteVisual,
    },
};
#[cfg(target_os = "windows")]
use windows_core::{implement, Error as WindowsError, Interface, HRESULT, HSTRING, PCWSTR};
#[cfg(target_os = "windows")]
use windows_numerics::Vector2;
#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_CLASS_ALREADY_EXISTS, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Dwm::{DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_USE_HOSTBACKDROPBRUSH},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowRect, IsWindow, IsWindowVisible,
        RegisterClassW, SetWindowPos, ShowWindow, HTTRANSPARENT, MA_NOACTIVATE, SWP_NOACTIVATE,
        SWP_NOOWNERZORDER, SWP_SHOWWINDOW, SW_HIDE, WM_MOUSEACTIVATE, WM_NCHITTEST, WNDCLASSW,
        WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
    },
};

#[cfg(target_os = "windows")]
const COMPACT_INSET: f32 = 10.0;
#[cfg(target_os = "windows")]
const COMPACT_SIZE: f32 = 80.0;
#[cfg(target_os = "windows")]
const COMPACT_RADIUS: f32 = 28.0;
#[cfg(target_os = "windows")]
const EXPANDED_RADIUS: f32 = 38.0;
#[cfg(target_os = "windows")]
const CONTROL_RADIUS: f32 = 24.0;
#[cfg(target_os = "windows")]
const BLUR_EDGE_INSET: f32 = 1.0;
#[cfg(target_os = "windows")]
const BLUR_AMOUNT: f32 = 4.5;
#[cfg(target_os = "windows")]
const EXPAND_MORPH_MS: u64 = 400;
#[cfg(target_os = "windows")]
const COLLAPSE_MORPH_DELAY_MS: u64 = 90;
#[cfg(target_os = "windows")]
const COLLAPSE_MORPH_MS: u64 = 190;
#[cfg(target_os = "windows")]
const MORPH_FRAME_MS: u64 = 16;
#[cfg(target_os = "windows")]
const CONTROL_CLOSED_SCALE: f32 = 0.985;
#[cfg(target_os = "windows")]
const CONTROL_CLOSED_TRANSLATE_Y: f32 = -10.0;
#[cfg(target_os = "windows")]
const CONTROL_OPEN_FRAME_DELAY_MS: u64 = 32;
#[cfg(target_os = "windows")]
const PALETTE_OPEN_OPACITY_MS: u64 = 200;
#[cfg(target_os = "windows")]
const PALETTE_OPEN_TRANSFORM_MS: u64 = 280;
#[cfg(target_os = "windows")]
const EDITOR_OPEN_DELAY_MS: u64 = 60;
#[cfg(target_os = "windows")]
const PALETTE_CLOSE_DELAY_MS: u64 = 55;
#[cfg(target_os = "windows")]
const PALETTE_CLOSE_OPACITY_MS: u64 = 140;
#[cfg(target_os = "windows")]
const PALETTE_CLOSE_TRANSFORM_MS: u64 = 180;
#[cfg(target_os = "windows")]
const EDITOR_CLOSE_OPACITY_MS: u64 = 130;
#[cfg(target_os = "windows")]
const EDITOR_CLOSE_TRANSFORM_MS: u64 = 170;

// Mirrors --morph-spring in src/styles.css. Keeping the native blur surface on
// the card's existing curve prevents a second, visibly faster morph.
#[cfg(target_os = "windows")]
const MORPH_KEYFRAMES: [(f64, f64); 15] = [
    (0.0, 0.0),
    (0.05, 0.02),
    (0.10, 0.08),
    (0.15, 0.18),
    (0.20, 0.32),
    (0.25, 0.48),
    (0.30, 0.62),
    (0.35, 0.74),
    (0.40, 0.84),
    (0.45, 0.92),
    (0.50, 0.98),
    (0.60, 1.018),
    (0.72, 0.996),
    (0.82, 1.006),
    (1.0, 1.0),
];

#[cfg(target_os = "windows")]
static CLIP_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "windows")]
static CLIP_PROGRESS: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static PALETTE_MATERIAL_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "windows")]
static PALETTE_TRANSFORM_PROGRESS: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static PALETTE_OPACITY_PROGRESS: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static EDITOR_TRANSFORM_PROGRESS: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static EDITOR_OPACITY_PROGRESS: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static WEBVIEW_SCALE_BITS: AtomicU32 = AtomicU32::new(0);
#[cfg(target_os = "windows")]
static WEBVIEW_SCALE_VERIFIED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct GaussianBlurEffect {
    blur_amount: f32,
    source: IGraphicsEffectSource,
}

#[cfg(target_os = "windows")]
impl IGraphicsEffectSource_Impl for GaussianBlurEffect_Impl {}

#[cfg(target_os = "windows")]
impl IGraphicsEffect_Impl for GaussianBlurEffect_Impl {
    fn Name(&self) -> windows_core::Result<HSTRING> {
        Ok(HSTRING::from("QuotaBeaconLightBlur"))
    }

    fn SetName(&self, _name: &HSTRING) -> windows_core::Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl IGraphicsEffectD2D1Interop_Impl for GaussianBlurEffect_Impl {
    fn GetEffectId(&self) -> windows_core::Result<windows_core::GUID> {
        Ok(CLSID_D2D1GaussianBlur)
    }

    fn GetNamedPropertyMapping(
        &self,
        _name: &PCWSTR,
        index: *mut u32,
        mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> windows_core::Result<()> {
        if index.is_null() || mapping.is_null() {
            return Err(invalid_argument());
        }
        unsafe {
            index.write(0);
            mapping.write(GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT);
        }
        Ok(())
    }

    fn GetPropertyCount(&self) -> windows_core::Result<u32> {
        Ok(3)
    }

    fn GetProperty(&self, index: u32) -> windows_core::Result<IPropertyValue> {
        let value = match index {
            0 => PropertyValue::CreateSingle(self.blur_amount)?,
            1 => PropertyValue::CreateUInt32(D2D1_GAUSSIANBLUR_OPTIMIZATION_BALANCED.0 as u32)?,
            2 => PropertyValue::CreateUInt32(D2D1_BORDER_MODE_HARD.0 as u32)?,
            _ => return Err(invalid_argument()),
        };
        value.cast()
    }

    fn GetSource(&self, index: u32) -> windows_core::Result<IGraphicsEffectSource> {
        if index != 0 {
            return Err(invalid_argument());
        }
        Ok(self.source.clone())
    }

    fn GetSourceCount(&self) -> windows_core::Result<u32> {
        Ok(1)
    }
}

#[cfg(target_os = "windows")]
fn invalid_argument() -> WindowsError {
    WindowsError::from_hresult(HRESULT(0x8007_0057_u32 as i32))
}

#[cfg(target_os = "windows")]
struct CompositionSurface {
    _compositor: Compositor,
    _target: DesktopWindowTarget,
    _root: ContainerVisual,
    sprite: SpriteVisual,
    geometry: CompositionRoundedRectangleGeometry,
    _clip: CompositionGeometricClip,
    _effect_brush: CompositionEffectBrush,
    _backdrop: CompositionBackdropBrush,
}

#[cfg(target_os = "windows")]
struct BlurWindow {
    label: &'static str,
    kind: BlurWindowKind,
    parent: isize,
    surface: isize,
    sync_verified: AtomicBool,
    composition: CompositionSurface,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlurWindowKind {
    Widget,
    Palette,
    PaletteEditor,
    Control,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, PartialEq)]
struct SurfaceGeometry {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    radius: f32,
}

#[cfg(target_os = "windows")]
static BLUR_WINDOWS: OnceLock<Mutex<Vec<BlurWindow>>> = OnceLock::new();
#[cfg(target_os = "windows")]
static WINDOW_CLASS: OnceLock<Result<(), String>> = OnceLock::new();
#[cfg(target_os = "windows")]
static DISPATCHER_QUEUE: OnceLock<Result<DispatcherQueueController, String>> = OnceLock::new();

#[cfg(target_os = "windows")]
unsafe extern "system" fn blur_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(target_os = "windows")]
fn class_name() -> Vec<u16> {
    "QuotaBeaconLightBlur\0".encode_utf16().collect()
}

#[cfg(target_os = "windows")]
fn ensure_window_class() -> Result<(), String> {
    WINDOW_CLASS
        .get_or_init(|| {
            let name = class_name();
            let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
            if instance.is_null() {
                return Err("GetModuleHandleW failed".to_string());
            }
            let class = WNDCLASSW {
                lpfnWndProc: Some(blur_window_proc),
                hInstance: instance,
                lpszClassName: name.as_ptr(),
                ..Default::default()
            };
            if unsafe { RegisterClassW(&class) } == 0 {
                let error = unsafe { GetLastError() };
                if error != ERROR_CLASS_ALREADY_EXISTS {
                    return Err(format!("RegisterClassW failed with error {error}"));
                }
            }
            Ok(())
        })
        .clone()
}

#[cfg(target_os = "windows")]
fn ensure_dispatcher_queue() -> Result<(), String> {
    DISPATCHER_QUEUE
        .get_or_init(|| {
            let options = DispatcherQueueOptions {
                dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_STA,
            };
            unsafe { CreateDispatcherQueueController(options) }.map_err(|error| error.to_string())
        })
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}

#[cfg(target_os = "windows")]
fn enable_host_backdrop(hwnd: HWND) -> Result<(), String> {
    let enabled = 1i32;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH as u32,
            (&enabled as *const i32).cast(),
            std::mem::size_of_val(&enabled) as u32,
        )
    };
    if result < 0 {
        return Err(format!(
            "DWMWA_USE_HOSTBACKDROPBRUSH failed with HRESULT {result:#010x}"
        ));
    }
    let mut actual = 0i32;
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_USE_HOSTBACKDROPBRUSH as u32,
            (&mut actual as *mut i32).cast(),
            std::mem::size_of_val(&actual) as u32,
        )
    };
    if result >= 0 && actual == 0 {
        return Err(format!(
            "DWMWA_USE_HOSTBACKDROPBRUSH readback returned disabled: value {actual}"
        ));
    }
    if result < 0 {
        eprintln!("host backdrop readback unavailable: HRESULT {result:#010x}; set call succeeded");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn create_composition_surface(surface: HWND) -> Result<CompositionSurface, String> {
    enable_host_backdrop(surface)?;
    ensure_dispatcher_queue()?;
    let compositor = Compositor::new().map_err(|error| format!("create compositor: {error}"))?;
    let interop: ICompositorDesktopInterop = compositor
        .cast()
        .map_err(|error| format!("get desktop compositor interop: {error}"))?;
    let target =
        unsafe { interop.CreateDesktopWindowTarget(CompositionHwnd(surface.cast()), false) }
            .map_err(|error| format!("create desktop window target: {error}"))?;
    let root = compositor
        .CreateContainerVisual()
        .map_err(|error| format!("create root visual: {error}"))?;
    let sprite = compositor
        .CreateSpriteVisual()
        .map_err(|error| format!("create sprite visual: {error}"))?;
    let source_name = HSTRING::from("backdrop");
    let source_parameter = CompositionEffectSourceParameter::Create(&source_name)
        .map_err(|error| format!("create effect source parameter: {error}"))?;
    let effect_source: IGraphicsEffectSource = source_parameter
        .cast()
        .map_err(|error| format!("cast effect source parameter: {error}"))?;
    let effect: IGraphicsEffect = GaussianBlurEffect {
        blur_amount: BLUR_AMOUNT,
        source: effect_source,
    }
    .into();
    let effect_factory = compositor
        .CreateEffectFactory(&effect)
        .map_err(|error| format!("create blur effect factory: {error}"))?;
    let effect_brush = effect_factory
        .CreateBrush()
        .map_err(|error| format!("create blur effect brush: {error}"))?;
    let backdrop = compositor
        .CreateHostBackdropBrush()
        .map_err(|error| format!("create host backdrop brush: {error}"))?;
    effect_brush
        .SetSourceParameter(&source_name, &backdrop)
        .map_err(|error| format!("bind host backdrop brush: {error}"))?;
    sprite
        .SetBrush(&effect_brush)
        .map_err(|error| format!("set sprite brush: {error}"))?;
    let geometry = compositor
        .CreateRoundedRectangleGeometry()
        .map_err(|error| format!("create rounded geometry: {error}"))?;
    let clip = compositor
        .CreateGeometricClipWithGeometry(&geometry)
        .map_err(|error| format!("create rounded clip: {error}"))?;
    sprite
        .SetClip(&clip)
        .map_err(|error| format!("set rounded clip: {error}"))?;
    root.Children()
        .and_then(|children| children.InsertAtTop(&sprite))
        .map_err(|error| format!("insert blur visual: {error}"))?;
    target
        .SetRoot(&root)
        .map_err(|error| format!("set desktop target root: {error}"))?;
    Ok(CompositionSurface {
        _compositor: compositor,
        _target: target,
        _root: root,
        sprite,
        geometry,
        _clip: clip,
        _effect_brush: effect_brush,
        _backdrop: backdrop,
    })
}

#[cfg(target_os = "windows")]
fn create_blur_window() -> Result<(HWND, CompositionSurface), String> {
    ensure_window_class()?;
    let name = class_name();
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    if instance.is_null() {
        return Err("GetModuleHandleW failed".to_string());
    }
    let surface = unsafe {
        CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
            name.as_ptr(),
            name.as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if surface.is_null() {
        return Err("CreateWindowExW failed".to_string());
    }
    match create_composition_surface(surface) {
        Ok(composition) => Ok((surface, composition)),
        Err(error) => {
            unsafe { DestroyWindow(surface) };
            Err(error)
        }
    }
}

#[cfg(target_os = "windows")]
fn morph_curve(linear: f64) -> f64 {
    let linear = linear.clamp(0.0, 1.0);
    for pair in MORPH_KEYFRAMES.windows(2) {
        let (start_time, start_value) = pair[0];
        let (end_time, end_value) = pair[1];
        if linear <= end_time {
            let segment = (linear - start_time) / (end_time - start_time);
            return start_value + (end_value - start_value) * segment;
        }
    }
    1.0
}

#[cfg(target_os = "windows")]
fn widget_geometry(parent: RECT, scale: f32, progress: f32) -> SurfaceGeometry {
    let progress = progress.clamp(-0.05, 1.05);
    let outer_x = (COMPACT_INSET * scale * (1.0 - progress)).ceil().max(0.0) as i32;
    let outer_y = outer_x;
    let parent_width = parent.right - parent.left;
    let parent_height = parent.bottom - parent.top;
    let compact_size = COMPACT_SIZE * scale;
    let outer_width = (compact_size + (parent_width as f32 - compact_size) * progress)
        .floor()
        .max(1.0) as i32;
    let outer_height = (compact_size + (parent_height as f32 - compact_size) * progress)
        .floor()
        .max(1.0) as i32;
    let outer_width = outer_width.min((parent_width - outer_x).max(1));
    let outer_height = outer_height.min((parent_height - outer_y).max(1));

    // The WebView owns the antialiased 1px border. Keeping the native blur
    // inside that border prevents two rounded clips from forming bright ears
    // where their independently rasterized curves meet on scaled displays.
    let edge_inset = (BLUR_EDGE_INSET * scale).ceil().max(1.0) as i32;
    let x = outer_x + edge_inset;
    let y = outer_y + edge_inset;
    SurfaceGeometry {
        x,
        y,
        width: (outer_width - edge_inset * 2).max(1),
        height: (outer_height - edge_inset * 2).max(1),
        radius: ((COMPACT_RADIUS + (EXPANDED_RADIUS - COMPACT_RADIUS) * progress
            - BLUR_EDGE_INSET)
            * scale)
            .max(0.0),
    }
}

#[cfg(target_os = "windows")]
fn control_geometry(parent: RECT, scale: f32, progress: f32) -> SurfaceGeometry {
    let progress = progress.clamp(0.0, 1.0);
    let parent_width = parent.right - parent.left;
    let parent_height = parent.bottom - parent.top;
    let shell_scale = CONTROL_CLOSED_SCALE + (1.0 - CONTROL_CLOSED_SCALE) * progress;
    let transformed_width = parent_width as f32 * shell_scale;
    let transformed_height = parent_height as f32 * shell_scale;
    let translate_y = CONTROL_CLOSED_TRANSLATE_Y * scale * (1.0 - progress);
    let outer_x = ((parent_width as f32 - transformed_width) / 2.0)
        .round()
        .max(0.0) as i32;
    // The WebView clips the translated shell at the transparent window edge.
    // Matching that visible bound prevents the separate native blur HWND from
    // appearing before the CSS shell or lingering outside it while closing.
    let outer_y = translate_y.max(0.0).round() as i32;
    let visible_bottom = (translate_y + transformed_height)
        .min(parent_height as f32)
        .max(1.0);
    let outer_width = transformed_width.floor().max(1.0) as i32;
    let outer_height = (visible_bottom - outer_y as f32).floor().max(1.0) as i32;
    let edge_inset = (BLUR_EDGE_INSET * scale).ceil().max(1.0) as i32;
    SurfaceGeometry {
        x: outer_x + edge_inset,
        y: outer_y + edge_inset,
        width: (outer_width.min(parent_width - outer_x) - edge_inset * 2).max(1),
        height: (outer_height.min(parent_height - outer_y) - edge_inset * 2).max(1),
        radius: ((CONTROL_RADIUS * shell_scale - BLUR_EDGE_INSET) * scale).max(0.0),
    }
}

#[cfg(target_os = "windows")]
fn cubic_bezier_value(linear: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= f64::EPSILON {
        return 0.0;
    }
    if 1.0 - linear <= f64::EPSILON {
        return 1.0;
    }
    let sample = |time: f64, first: f64, second: f64| {
        let inverse = 1.0 - time;
        3.0 * inverse * inverse * time * first
            + 3.0 * inverse * time * time * second
            + time * time * time
    };
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..14 {
        let time = (lower + upper) / 2.0;
        if sample(time, x1, x2) < linear {
            lower = time;
        } else {
            upper = time;
        }
    }
    sample((lower + upper) / 2.0, y1, y2)
}

#[cfg(target_os = "windows")]
fn transition_progress(
    elapsed_ms: f64,
    start: f64,
    target: f64,
    delay_ms: u64,
    duration_ms: u64,
    curve: (f64, f64, f64, f64),
) -> f64 {
    if elapsed_ms <= delay_ms as f64 {
        return start;
    }
    let linear = ((elapsed_ms - delay_ms as f64) / duration_ms as f64).clamp(0.0, 1.0);
    let eased = cubic_bezier_value(linear, curve.0, curve.1, curve.2, curve.3);
    start + (target - start) * eased
}

#[cfg(target_os = "windows")]
fn active_window_scale(app: &AppHandle, label: &str) -> f32 {
    if label == "widget" {
        let stored = f32::from_bits(WEBVIEW_SCALE_BITS.load(Ordering::SeqCst));
        if stored.is_finite() && (0.5..=5.0).contains(&stored) {
            return stored;
        }
    }
    app.get_webview_window(label)
        .and_then(|window| window.scale_factor().ok())
        .unwrap_or(1.0) as f32
}

#[cfg(target_os = "windows")]
fn sync_blur_window(window: &BlurWindow, app: &AppHandle) -> Result<(), String> {
    let parent = window.parent as HWND;
    let surface = window.surface as HWND;
    if unsafe { IsWindowVisible(parent) } == 0 {
        unsafe { ShowWindow(surface, SW_HIDE) };
        return Ok(());
    }
    let mut parent_rect = RECT::default();
    if unsafe { GetWindowRect(parent, &mut parent_rect) } == 0 {
        return Err("GetWindowRect failed".to_string());
    }
    let scale = active_window_scale(app, window.label);
    let (geometry, opacity) = match window.kind {
        BlurWindowKind::Widget => {
            let progress = CLIP_PROGRESS.load(Ordering::SeqCst) as f32 / 1000.0;
            (widget_geometry(parent_rect, scale, progress), 1.0)
        }
        BlurWindowKind::Palette => {
            let progress = PALETTE_TRANSFORM_PROGRESS.load(Ordering::SeqCst) as f32 / 1000.0;
            let opacity = PALETTE_OPACITY_PROGRESS.load(Ordering::SeqCst) as f32 / 1000.0;
            (control_geometry(parent_rect, scale, progress), opacity)
        }
        BlurWindowKind::PaletteEditor => {
            let progress = EDITOR_TRANSFORM_PROGRESS.load(Ordering::SeqCst) as f32 / 1000.0;
            let opacity = EDITOR_OPACITY_PROGRESS.load(Ordering::SeqCst) as f32 / 1000.0;
            (control_geometry(parent_rect, scale, progress), opacity)
        }
        BlurWindowKind::Control => (control_geometry(parent_rect, scale, 1.0), 1.0),
    };
    let size = Vector2 {
        X: geometry.width as f32,
        Y: geometry.height as f32,
    };
    let radius = Vector2 {
        X: geometry.radius,
        Y: geometry.radius,
    };
    window
        .composition
        .sprite
        .SetOpacity(opacity.clamp(0.0, 1.0))
        .map_err(|error| format!("set blur opacity: {error}"))?;
    window
        .composition
        .sprite
        .SetSize(size)
        .map_err(|error| format!("set blur size: {error}"))?;
    window
        .composition
        .geometry
        .SetSize(size)
        .map_err(|error| format!("set clip size: {error}"))?;
    window
        .composition
        .geometry
        .SetCornerRadius(radius)
        .map_err(|error| format!("set clip radius: {error}"))?;
    let result = unsafe {
        SetWindowPos(
            surface,
            parent,
            parent_rect.left + geometry.x,
            parent_rect.top + geometry.y,
            geometry.width,
            geometry.height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
        )
    };
    if result == 0 {
        return Err("SetWindowPos failed".to_string());
    }
    let expected = RECT {
        left: parent_rect.left + geometry.x,
        top: parent_rect.top + geometry.y,
        right: parent_rect.left + geometry.x + geometry.width,
        bottom: parent_rect.top + geometry.y + geometry.height,
    };
    let mut actual = RECT::default();
    if unsafe { GetWindowRect(surface, &mut actual) } == 0 {
        unsafe { ShowWindow(surface, SW_HIDE) };
        return Err("blur surface GetWindowRect failed".to_string());
    }
    if actual.left != expected.left
        || actual.top != expected.top
        || actual.right != expected.right
        || actual.bottom != expected.bottom
    {
        unsafe { ShowWindow(surface, SW_HIDE) };
        return Err(format!(
            "blur surface geometry mismatch: expected ({}, {}, {}, {}), got ({}, {}, {}, {})",
            expected.left,
            expected.top,
            expected.right,
            expected.bottom,
            actual.left,
            actual.top,
            actual.right,
            actual.bottom
        ));
    }
    if !window.sync_verified.swap(true, Ordering::SeqCst) {
        eprintln!(
            "{} light blur verified: host backdrop enabled, parent {}x{}, surface {}x{} at ({}, {})",
            window.label,
            parent_rect.right - parent_rect.left,
            parent_rect.bottom - parent_rect.top,
            geometry.width,
            geometry.height,
            actual.left,
            actual.top
        );
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn sync_window_material(app: &AppHandle) {
    let Some(slot) = BLUR_WINDOWS.get() else {
        return;
    };
    let Ok(slot) = slot.lock() else {
        return;
    };
    for window in slot.iter() {
        if let Err(error) = sync_blur_window(window, app) {
            eprintln!("{} light blur sync failed: {error}", window.label);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn set_widget_css_scale(app: &AppHandle, scale: f32) {
    if !scale.is_finite() || !(0.5..=5.0).contains(&scale) {
        return;
    }
    WEBVIEW_SCALE_BITS.store(scale.to_bits(), Ordering::SeqCst);
    if !WEBVIEW_SCALE_VERIFIED.swap(true, Ordering::SeqCst) {
        let native_scale = app
            .get_webview_window("widget")
            .and_then(|window| window.scale_factor().ok())
            .unwrap_or(1.0);
        eprintln!("webview pixel scale synchronized: css {scale:.4}, native {native_scale:.4}");
    }
    sync_window_material(app);
}

#[cfg(target_os = "windows")]
pub fn hide_window_material() {
    let Some(slot) = BLUR_WINDOWS.get() else {
        return;
    };
    let Ok(slot) = slot.lock() else {
        return;
    };
    for window in slot.iter() {
        unsafe { ShowWindow(window.surface as HWND, SW_HIDE) };
    }
}

#[cfg(target_os = "windows")]
pub fn animate_widget_region(app: AppHandle, expanded: bool) {
    let generation = CLIP_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let start = CLIP_PROGRESS.load(Ordering::SeqCst) as f64 / 1000.0;
    let target = if expanded { 1.0 } else { 0.0 };
    tauri::async_runtime::spawn(async move {
        let distance = (target - start).abs().min(1.0);
        if distance <= f64::EPSILON {
            return;
        }

        if !expanded {
            let delay = std::time::Duration::from_millis(COLLAPSE_MORPH_DELAY_MS);
            let delay_started = tokio::time::Instant::now();
            while delay_started.elapsed() < delay {
                if CLIP_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                let remaining = delay.saturating_sub(delay_started.elapsed());
                tokio::time::sleep(remaining.min(std::time::Duration::from_millis(MORPH_FRAME_MS)))
                    .await;
            }
        }

        let full_duration_ms = if expanded {
            EXPAND_MORPH_MS
        } else {
            COLLAPSE_MORPH_MS
        };
        let duration = std::time::Duration::from_secs_f64(
            std::time::Duration::from_millis(full_duration_ms).as_secs_f64() * distance,
        );
        let started = tokio::time::Instant::now();
        loop {
            if CLIP_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let linear = (started.elapsed().as_secs_f64() / duration.as_secs_f64()).min(1.0);
            let eased = morph_curve(linear);
            let progress = start + (target - start) * eased;
            CLIP_PROGRESS.store((progress * 1000.0).round() as i32, Ordering::SeqCst);
            if let Some(window) = app.get_webview_window("widget") {
                let sync_app = app.clone();
                let _ = window.run_on_main_thread(move || sync_window_material(&sync_app));
            }
            if linear >= 1.0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(MORPH_FRAME_MS)).await;
        }
    });
}

#[cfg(target_os = "windows")]
pub fn reset_palette_material() {
    PALETTE_MATERIAL_GENERATION.fetch_add(1, Ordering::SeqCst);
    PALETTE_TRANSFORM_PROGRESS.store(0, Ordering::SeqCst);
    PALETTE_OPACITY_PROGRESS.store(0, Ordering::SeqCst);
    EDITOR_TRANSFORM_PROGRESS.store(0, Ordering::SeqCst);
    EDITOR_OPACITY_PROGRESS.store(0, Ordering::SeqCst);
}

#[cfg(target_os = "windows")]
pub fn animate_palette_material(app: AppHandle, opening: bool) {
    let generation = PALETTE_MATERIAL_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let palette_transform_start = PALETTE_TRANSFORM_PROGRESS.load(Ordering::SeqCst) as f64 / 1000.0;
    let palette_opacity_start = PALETTE_OPACITY_PROGRESS.load(Ordering::SeqCst) as f64 / 1000.0;
    let editor_transform_start = EDITOR_TRANSFORM_PROGRESS.load(Ordering::SeqCst) as f64 / 1000.0;
    let editor_opacity_start = EDITOR_OPACITY_PROGRESS.load(Ordering::SeqCst) as f64 / 1000.0;
    let target = if opening { 1.0 } else { 0.0 };

    tauri::async_runtime::spawn(async move {
        let started = tokio::time::Instant::now();
        let transform_curve = (0.2, 0.82, 0.2, 1.08);
        let opacity_curve = (0.25, 0.1, 0.25, 1.0);
        let (
            palette_delay,
            editor_delay,
            palette_opacity_ms,
            palette_transform_ms,
            editor_opacity_ms,
            editor_transform_ms,
        ) = if opening {
            (
                CONTROL_OPEN_FRAME_DELAY_MS,
                CONTROL_OPEN_FRAME_DELAY_MS + EDITOR_OPEN_DELAY_MS,
                PALETTE_OPEN_OPACITY_MS,
                PALETTE_OPEN_TRANSFORM_MS,
                PALETTE_OPEN_OPACITY_MS,
                PALETTE_OPEN_TRANSFORM_MS,
            )
        } else {
            (
                PALETTE_CLOSE_DELAY_MS,
                0,
                PALETTE_CLOSE_OPACITY_MS,
                PALETTE_CLOSE_TRANSFORM_MS,
                EDITOR_CLOSE_OPACITY_MS,
                EDITOR_CLOSE_TRANSFORM_MS,
            )
        };
        let total_ms =
            (palette_delay + palette_transform_ms).max(editor_delay + editor_transform_ms);

        loop {
            if PALETTE_MATERIAL_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            let palette_transform = transition_progress(
                elapsed_ms,
                palette_transform_start,
                target,
                palette_delay,
                palette_transform_ms,
                transform_curve,
            );
            let palette_opacity = transition_progress(
                elapsed_ms,
                palette_opacity_start,
                target,
                palette_delay,
                palette_opacity_ms,
                opacity_curve,
            );
            let editor_transform = transition_progress(
                elapsed_ms,
                editor_transform_start,
                target,
                editor_delay,
                editor_transform_ms,
                transform_curve,
            );
            let editor_opacity = transition_progress(
                elapsed_ms,
                editor_opacity_start,
                target,
                editor_delay,
                editor_opacity_ms,
                opacity_curve,
            );
            PALETTE_TRANSFORM_PROGRESS.store(
                (palette_transform * 1000.0).round() as i32,
                Ordering::SeqCst,
            );
            PALETTE_OPACITY_PROGRESS
                .store((palette_opacity * 1000.0).round() as i32, Ordering::SeqCst);
            EDITOR_TRANSFORM_PROGRESS
                .store((editor_transform * 1000.0).round() as i32, Ordering::SeqCst);
            EDITOR_OPACITY_PROGRESS
                .store((editor_opacity * 1000.0).round() as i32, Ordering::SeqCst);

            if let Some(window) = app.get_webview_window("palette") {
                let sync_app = app.clone();
                let _ = window.run_on_main_thread(move || sync_window_material(&sync_app));
            }
            if elapsed_ms >= total_ms as f64 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(MORPH_FRAME_MS)).await;
        }
    });
}

#[cfg(target_os = "windows")]
pub fn apply_window_materials(app: &AppHandle) {
    let Some(widget) = app.get_webview_window("widget") else {
        eprintln!("light blur skipped: widget window missing");
        return;
    };
    let widget_scale = active_window_scale(app, "widget");
    let expanded = widget
        .outer_size()
        .map(|size| size.width as f64 / widget_scale as f64 > 120.0)
        .unwrap_or(false);
    CLIP_PROGRESS.store(if expanded { 1000 } else { 0 }, Ordering::SeqCst);
    let slot = BLUR_WINDOWS.get_or_init(|| Mutex::new(Vec::new()));
    let Ok(mut slot) = slot.lock() else {
        return;
    };
    for (label, kind) in [
        ("widget", BlurWindowKind::Widget),
        ("palette", BlurWindowKind::Palette),
        ("palette-editor", BlurWindowKind::PaletteEditor),
        ("account-switcher", BlurWindowKind::Control),
    ] {
        if slot.iter().any(|window| window.label == label) {
            continue;
        }
        let Some(parent_window) = app.get_webview_window(label) else {
            eprintln!("{label} light blur skipped: window missing");
            continue;
        };
        let Ok(parent) = parent_window.hwnd() else {
            eprintln!("{label} light blur skipped: HWND unavailable");
            continue;
        };
        match create_blur_window() {
            Ok((surface, composition)) => {
                slot.push(BlurWindow {
                    label,
                    kind,
                    parent: parent.0 as isize,
                    surface: surface as isize,
                    sync_verified: AtomicBool::new(false),
                    composition,
                });
                eprintln!("{label} light rounded host-backdrop blur attached");
            }
            Err(error) => {
                eprintln!("{label} light blur unavailable: {error}");
            }
        }
    }
    for window in slot.iter() {
        if let Err(error) = sync_blur_window(window, app) {
            eprintln!("{} light blur initial sync failed: {error}", window.label);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn destroy_window_materials() {
    let Some(slot) = BLUR_WINDOWS.get() else {
        return;
    };
    let Ok(mut slot) = slot.lock() else {
        return;
    };
    for window in slot.drain(..) {
        if unsafe { IsWindow(window.surface as HWND) } != 0 {
            unsafe { DestroyWindow(window.surface as HWND) };
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    fn rect(width: i32, height: i32) -> RECT {
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    }

    #[test]
    fn compact_blur_stays_inside_css_border() {
        assert_eq!(
            widget_geometry(rect(160, 160), 1.6, 0.0),
            SurfaceGeometry {
                x: 18,
                y: 18,
                width: 124,
                height: 124,
                radius: 43.2,
            }
        );
    }

    #[test]
    fn expanded_blur_stays_inside_css_border() {
        assert_eq!(
            widget_geometry(rect(512, 512), 1.6, 1.0),
            SurfaceGeometry {
                x: 2,
                y: 2,
                width: 508,
                height: 508,
                radius: 59.2,
            }
        );
    }

    #[test]
    fn control_blur_stays_inside_internal_stroke() {
        assert_eq!(
            control_geometry(rect(512, 166), 1.6, 1.0),
            SurfaceGeometry {
                x: 2,
                y: 2,
                width: 508,
                height: 162,
                radius: 36.8,
            }
        );
    }

    #[test]
    fn closed_palette_blur_matches_the_transformed_css_shell() {
        assert_eq!(
            control_geometry(rect(512, 166), 1.6, 0.0),
            SurfaceGeometry {
                x: 6,
                y: 2,
                width: 500,
                height: 143,
                radius: 36.224,
            }
        );
    }

    #[test]
    fn control_transition_curves_keep_exact_endpoints() {
        let curve = (0.2, 0.82, 0.2, 1.08);
        assert!((transition_progress(0.0, 0.0, 1.0, 32, 280, curve) - 0.0).abs() < 0.000_001);
        assert!((transition_progress(312.0, 0.0, 1.0, 32, 280, curve) - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn compact_surface_stays_eighty_pixels_after_parent_expands() {
        assert_eq!(
            widget_geometry(rect(512, 512), 1.6, 0.0),
            SurfaceGeometry {
                x: 18,
                y: 18,
                width: 124,
                height: 124,
                radius: 43.2,
            }
        );
    }

    #[test]
    fn halfway_surface_uses_the_card_morph_geometry() {
        assert_eq!(
            widget_geometry(rect(512, 512), 1.6, 0.5),
            SurfaceGeometry {
                x: 10,
                y: 10,
                width: 316,
                height: 316,
                radius: 51.2,
            }
        );
    }

    #[test]
    fn native_morph_curve_matches_css_keyframes() {
        for (time, value) in MORPH_KEYFRAMES {
            assert!((morph_curve(time) - value).abs() < f64::EPSILON);
        }
        assert!((morph_curve(0.575) - 1.0085).abs() < 0.000_001);
    }

    #[test]
    fn compact_surface_tracks_webview_pixel_scale_without_corner_overhang() {
        assert_eq!(
            widget_geometry(rect(160, 160), 1.75, 0.0),
            SurfaceGeometry {
                x: 20,
                y: 20,
                width: 136,
                height: 136,
                radius: 47.25,
            }
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_window_materials(_app: &AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn sync_window_material(_app: &AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn set_widget_css_scale(_app: &AppHandle, _scale: f32) {}

#[cfg(not(target_os = "windows"))]
pub fn hide_window_material() {}

#[cfg(not(target_os = "windows"))]
pub fn animate_widget_region(_app: AppHandle, _expanded: bool) {}

#[cfg(not(target_os = "windows"))]
pub fn reset_palette_material() {}

#[cfg(not(target_os = "windows"))]
pub fn animate_palette_material(_app: AppHandle, _opening: bool) {}

#[cfg(not(target_os = "windows"))]
pub fn destroy_window_materials() {}
