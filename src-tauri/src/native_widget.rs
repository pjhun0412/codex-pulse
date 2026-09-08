#![cfg(windows)]

use std::{
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Utc};
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
        DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetPixel, InvalidateRect,
        ReleaseDC, ScreenToClient, SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY,
        DEFAULT_CHARSET, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FW_NORMAL, PAINTSTRUCT,
        TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{
            GetDpiForWindow, SetThreadDpiAwarenessContext,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, GetParent,
            GetWindowLongPtrW, GetWindowRect, KillTimer, LoadCursorW, PostMessageW,
            PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetForegroundWindow,
            SetParent, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TrackPopupMenuEx,
            TranslateMessage, UpdateLayeredWindow, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW,
            GWLP_USERDATA, GWL_EXSTYLE, GWL_STYLE, HTCLIENT, HWND_TOP, IDC_ARROW, MA_NOACTIVATE,
            MF_CHECKED, MF_SEPARATOR, MF_STRING, MSG, SWP_FRAMECHANGED, SWP_NOACTIVATE,
            SWP_NOOWNERZORDER, SW_HIDE, SW_SHOWNOACTIVATE, TPM_RETURNCMD, TPM_RIGHTBUTTON,
            ULW_OPAQUE, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_DESTROY, WM_DISPLAYCHANGE,
            WM_DPICHANGED, WM_DWMCOMPOSITIONCHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT,
            WM_SETTINGCHANGE, WM_TIMER, WNDCLASSEXW, WS_CHILD, WS_EX_LAYERED, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
        },
    },
};

use crate::{
    model::{CodexSnapshot, SystemSnapshot},
    taskbar::{self, Bounds},
};

const CLASS_NAME: &str = "CodexPulseNativeTaskbarWidget";
const WINDOW_TITLE: &str = "Codex Pulse Native Widget";
const LOGICAL_WIDTH: i32 = 248;
const LOGICAL_HEIGHT: i32 = 48;
const POSITION_TIMER_ID: usize = 1;
const POSITION_TIMER_MS: u32 = 100;

const MSG_REFRESH: u32 = WM_APP + 1;
const MSG_SHOW_TASKBAR: u32 = WM_APP + 2;
const MSG_HIDE_WIDGET: u32 = WM_APP + 3;
const MSG_SHUTDOWN: u32 = WM_APP + 4;

const MENU_DETAIL: usize = 1001;
const MENU_REFRESH: usize = 1002;
const MENU_OVERLAY: usize = 1003;
const MENU_THEME: usize = 1004;
const MENU_AUTOSTART: usize = 1005;
const MENU_HIDE: usize = 1006;
const MENU_QUIT: usize = 1007;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WidgetTheme {
    #[default]
    Dark,
    Light,
}

impl WidgetTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }
}

#[derive(Clone, Debug)]
pub enum NativeWidgetEvent {
    ToggleDetail,
    Refresh,
    SwitchToOverlay,
    SetTheme(WidgetTheme),
    ToggleAutostart,
    Hide,
    Quit,
}

#[derive(Default)]
struct NativeWidgetModel {
    codex_remaining: Option<f64>,
    resets_at: Option<String>,
    cpu_percent: Option<f32>,
    memory_percent: Option<f32>,
    detail_visible: bool,
    theme: WidgetTheme,
    autostart_enabled: bool,
}

#[derive(Default)]
struct DragState {
    pressed: bool,
    moved: bool,
    start_cursor: POINT,
    start_window: RECT,
}

struct NativeWidgetShared {
    hwnd: AtomicIsize,
    visible: AtomicBool,
    shutting_down: AtomicBool,
    menu_open: AtomicBool,
    last_interaction_ms: AtomicU64,
    timer_ticks: AtomicU32,
    taskbar_color: AtomicU32,
    dock_available: AtomicBool,
    last_bounds: Mutex<Option<Bounds>>,
    model: Mutex<NativeWidgetModel>,
    drag: Mutex<DragState>,
    events: Sender<NativeWidgetEvent>,
}

pub struct NativeWidgetController {
    shared: Arc<NativeWidgetShared>,
}

impl NativeWidgetController {
    pub fn start(events: Sender<NativeWidgetEvent>) -> Result<Arc<NativeWidgetController>, String> {
        let shared = Arc::new(NativeWidgetShared {
            hwnd: AtomicIsize::new(0),
            visible: AtomicBool::new(true),
            shutting_down: AtomicBool::new(false),
            menu_open: AtomicBool::new(false),
            last_interaction_ms: AtomicU64::new(0),
            timer_ticks: AtomicU32::new(0),
            taskbar_color: AtomicU32::new(u32::MAX),
            dock_available: AtomicBool::new(true),
            last_bounds: Mutex::new(None),
            model: Mutex::new(NativeWidgetModel::default()),
            drag: Mutex::new(DragState::default()),
            events,
        });
        let controller = Arc::new(Self {
            shared: shared.clone(),
        });
        let (ready_tx, ready_rx) = mpsc::channel();

        std::thread::Builder::new()
            .name("codex-pulse-native-widget".to_string())
            .spawn(move || {
                let result = unsafe { run_window_thread(shared, &ready_tx) };
                if let Err(error) = &result {
                    crate::logging::write(format!("native widget thread stopped: {error}"));
                    let _ = ready_tx.send(Err(error.clone()));
                }
            })
            .map_err(|error| error.to_string())?;

        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(controller),
            Ok(Err(error)) => Err(error),
            Err(_) => {
                if controller.shared.hwnd.load(Ordering::Acquire) != 0 {
                    Ok(controller)
                } else {
                    Err("네이티브 작업표시줄 위젯 시작 시간이 초과되었습니다.".to_string())
                }
            }
        }
    }

    pub fn update_codex(&self, snapshot: &CodexSnapshot) {
        if let Ok(mut model) = self.shared.model.lock() {
            model.codex_remaining = snapshot
                .primary_limit
                .as_ref()
                .map(|limit| limit.remaining_percent);
            model.resets_at = snapshot
                .primary_limit
                .as_ref()
                .and_then(|limit| limit.resets_at.clone());
        }
        self.request_refresh();
    }

    pub fn update_system(&self, snapshot: &SystemSnapshot) {
        if let Ok(mut model) = self.shared.model.lock() {
            model.cpu_percent = snapshot.available.then_some(snapshot.cpu_percent);
            model.memory_percent = snapshot.available.then_some(snapshot.memory_percent);
        }
        self.request_refresh();
    }

    pub fn set_detail_visible(&self, visible: bool) {
        if let Ok(mut model) = self.shared.model.lock() {
            model.detail_visible = visible;
        }
    }

    pub fn set_theme(&self, theme: WidgetTheme) {
        if let Ok(mut model) = self.shared.model.lock() {
            model.theme = theme;
        }
        self.request_refresh();
    }

    pub fn set_autostart_enabled(&self, enabled: bool) {
        if let Ok(mut model) = self.shared.model.lock() {
            model.autostart_enabled = enabled;
        }
    }

    pub fn show_taskbar(&self) {
        self.shared.visible.store(true, Ordering::Release);
        self.post(MSG_SHOW_TASKBAR);
    }

    pub fn hide(&self) {
        // Preserve the user's choice even while Explorer is recreating the HWND.
        self.shared.visible.store(false, Ordering::Release);
        self.post(MSG_HIDE_WIDGET);
    }

    pub fn shutdown(&self) {
        self.shared.shutting_down.store(true, Ordering::Release);
        self.post(MSG_SHUTDOWN);
    }

    pub fn bounds(&self) -> Option<Bounds> {
        let hwnd = self.hwnd();
        if hwnd.is_null() {
            return None;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return None;
        }
        Some(Bounds {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    pub fn scale_factor(&self) -> f64 {
        let hwnd = self.hwnd();
        if hwnd.is_null() {
            return 1.0;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi == 0 {
            1.0
        } else {
            f64::from(dpi) / 96.0
        }
    }

    pub fn should_suppress_detail_blur(&self) -> bool {
        if self.shared.menu_open.load(Ordering::Acquire) {
            return true;
        }
        let now = unix_millis();
        let last = self.shared.last_interaction_ms.load(Ordering::Acquire);
        now.saturating_sub(last) < 350
    }

    fn request_refresh(&self) {
        self.post(MSG_REFRESH);
    }

    fn post(&self, message: u32) {
        let hwnd = self.hwnd();
        if !hwnd.is_null() {
            unsafe {
                PostMessageW(hwnd, message, 0, 0);
            }
        }
    }

    fn hwnd(&self) -> HWND {
        self.shared.hwnd.load(Ordering::Acquire) as HWND
    }
}

impl Drop for NativeWidgetController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

unsafe fn run_window_thread(
    shared: Arc<NativeWidgetShared>,
    ready: &Sender<Result<(), String>>,
) -> Result<(), String> {
    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

    let instance = GetModuleHandleW(null());
    if instance.is_null() {
        return Err(format!("GetModuleHandleW 실패: {}", GetLastError()));
    }

    let class_name = wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: null_mut(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: null_mut(),
    };
    if RegisterClassExW(&class) == 0 {
        return Err(format!("RegisterClassExW 실패: {}", GetLastError()));
    }

    create_widget_window(&shared)?;
    // A thread timer survives destruction of Explorer's taskbar and its children.
    // Child windows do not receive the TaskbarCreated top-level broadcast.
    let timer = SetTimer(null_mut(), 0, POSITION_TIMER_MS, None);
    if timer == 0 {
        DestroyWindow(shared.hwnd.load(Ordering::Acquire) as HWND);
        return Err(format!("위젯 복구 타이머 생성 실패: {}", GetLastError()));
    }
    let _ = ready.send(Ok(()));

    let mut message = MSG::default();
    while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
        if shared.shutting_down.load(Ordering::Acquire) {
            break;
        }
        if message.hwnd.is_null() && message.message == WM_TIMER && message.wParam == timer {
            let mut hwnd = shared.hwnd.load(Ordering::Acquire) as HWND;
            if hwnd.is_null() {
                if taskbar::taskbar_windows_all().is_empty() {
                    continue;
                }
                hwnd = match create_widget_window(&shared) {
                    Ok(hwnd) => hwnd,
                    Err(error) => {
                        crate::logging::write(error);
                        continue;
                    }
                };
            }
            window_proc(hwnd, WM_TIMER, POSITION_TIMER_ID, 0);
        } else {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    KillTimer(null_mut(), timer);
    let hwnd = shared.hwnd.load(Ordering::Acquire) as HWND;
    if !hwnd.is_null() {
        DestroyWindow(hwnd);
    }
    Ok(())
}

unsafe fn create_widget_window(shared: &Arc<NativeWidgetShared>) -> Result<HWND, String> {
    let instance = GetModuleHandleW(null());
    let class_name = wide(CLASS_NAME);
    let title = wide(WINDOW_TITLE);
    let raw_shared = Arc::into_raw(shared.clone());
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class_name.as_ptr(),
        title.as_ptr(),
        WS_POPUP,
        0,
        0,
        LOGICAL_WIDTH,
        LOGICAL_HEIGHT,
        null_mut(),
        null_mut(),
        instance,
        raw_shared.cast(),
    );
    if hwnd.is_null() {
        return Err(format!("CreateWindowExW 실패: {}", GetLastError()));
    }

    shared.hwnd.store(hwnd as isize, Ordering::Release);
    position_over_taskbar(hwnd, shared);

    let style = GetWindowLongPtrW(hwnd, windows_sys::Win32::UI::WindowsAndMessaging::GWL_STYLE);
    let ex_style = GetWindowLongPtrW(
        hwnd,
        windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE,
    );
    crate::logging::write(format!(
        "native taskbar widget ready; hwnd={hwnd:?}, style={style:#x}, ex_style={ex_style:#x}"
    ));
    Ok(hwnd)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        if !create.is_null() {
            let shared = (*create).lpCreateParams as *const NativeWidgetShared;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, shared as isize);
        }
    }

    let shared_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const NativeWidgetShared;
    let shared = shared_ptr.as_ref();
    let taskbar_created = taskbar_created_message();

    if message == taskbar_created && taskbar_created != 0 {
        if let Some(shared) = shared {
            position_over_taskbar(hwnd, shared);
        }
        return 0;
    }

    match message {
        WM_PAINT => {
            if let Some(shared) = shared {
                paint_widget(hwnd, shared);
            }
            0
        }
        WM_ERASEBKGND => 1,
        WM_NCHITTEST => HTCLIENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN => {
            if let Some(shared) = shared {
                record_interaction(shared);
                if let Ok(mut drag) = shared.drag.lock() {
                    drag.pressed = true;
                    drag.moved = false;
                    GetCursorPos(&mut drag.start_cursor);
                    GetWindowRect(hwnd, &mut drag.start_window);
                }
                SetCapture(hwnd);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some(shared) = shared {
                if let Ok(mut drag) = shared.drag.lock() {
                    if drag.pressed {
                        let mut cursor = POINT::default();
                        if GetCursorPos(&mut cursor) != 0 {
                            let dx = cursor.x - drag.start_cursor.x;
                            let dy = cursor.y - drag.start_cursor.y;
                            if dx.abs() > 4 || dy.abs() > 4 {
                                drag.moved = true;
                            }
                        }
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            if let Some(shared) = shared {
                record_interaction(shared);
                let moved = shared
                    .drag
                    .lock()
                    .map(|mut drag| {
                        drag.pressed = false;
                        drag.moved
                    })
                    .unwrap_or(false);
                if !moved {
                    let _ = shared.events.send(NativeWidgetEvent::ToggleDetail);
                }
            }
            0
        }
        WM_CONTEXTMENU | windows_sys::Win32::UI::WindowsAndMessaging::WM_RBUTTONUP => {
            if let Some(shared) = shared {
                record_interaction(shared);
                show_context_menu(hwnd, shared);
            }
            0
        }
        WM_DWMCOMPOSITIONCHANGED => {
            if let Err(error) = reset_widget_layer(hwnd) {
                crate::logging::write(format!("native widget layer reset failed: {error}"));
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_DPICHANGED => {
            // SetParent may change DPI synchronously. Let the timer reposition
            // after reparenting completes instead of entering it recursively.
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => {
            if let Some(shared) = shared {
                position_over_taskbar(hwnd, shared);
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_TIMER if wparam == POSITION_TIMER_ID => {
            if let Some(shared) = shared {
                let tick = shared.timer_ticks.fetch_add(1, Ordering::AcqRel) + 1;
                if shared.visible.load(Ordering::Acquire) {
                    position_over_taskbar(hwnd, shared);
                }
                if tick % 10 == 0 {
                    InvalidateRect(hwnd, null(), 0);
                }
            }
            0
        }
        MSG_REFRESH => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        MSG_SHOW_TASKBAR => {
            if let Some(shared) = shared {
                shared.visible.store(true, Ordering::Release);
                position_over_taskbar(hwnd, shared);
                InvalidateRect(hwnd, null(), 0);
                crate::logging::write("native widget visibility: shown");
            }
            0
        }
        MSG_HIDE_WIDGET => {
            if let Some(shared) = shared {
                shared.visible.store(false, Ordering::Release);
            }
            ShowWindow(hwnd, SW_HIDE);
            crate::logging::write("native widget visibility: hidden");
            0
        }
        MSG_SHUTDOWN | WM_CLOSE => {
            if let Some(shared) = shared {
                shared.shutting_down.store(true, Ordering::Release);
            }
            DestroyWindow(hwnd);
            PostQuitMessage(0);
            0
        }
        WM_DESTROY => {
            crate::logging::write("native taskbar widget destroyed");
            0
        }
        WM_NCDESTROY => {
            if let Some(shared) = shared {
                shared.hwnd.store(0, Ordering::Release);
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let result = DefWindowProcW(hwnd, message, wparam, lparam);
            if !shared_ptr.is_null() {
                drop(Arc::from_raw(shared_ptr));
            }
            result
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn taskbar_background_color(hwnd: HWND) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, WindowFromPoint, GA_ROOT};
    let mut bounds = RECT::default();
    if GetWindowRect(hwnd, &mut bounds) == 0 {
        return None;
    }

    let sample = POINT {
        x: bounds.right + 2,
        y: bounds.top + 2,
    };
    // Menus, detail panels and fullscreen windows can cover this point. Never
    // derive taskbar theme colors from their pixels.
    let hit = WindowFromPoint(sample);
    if hit.is_null() || GetAncestor(hit, GA_ROOT) != GetAncestor(hwnd, GA_ROOT) {
        return None;
    }
    let dc = GetDC(null_mut());
    if dc.is_null() {
        return None;
    }
    // Sample outside the opaque child instead of sampling our own paint.
    let color = GetPixel(dc, sample.x, sample.y);
    ReleaseDC(null_mut(), dc);
    if color == u32::MAX {
        return None;
    }

    Some(color)
}

unsafe fn reset_widget_layer(hwnd: HWND) -> Result<(), u32> {
    // Reparenting invalidates the old redirected surface even though GDI blits
    // and hit tests still succeed. Recreate layering AFTER SetParent.
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (style & !WS_EX_LAYERED) as isize);
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (style | WS_EX_LAYERED) as isize);
    if GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_LAYERED == 0 {
        return Err(GetLastError());
    }
    InvalidateRect(hwnd, null(), 0);
    Ok(())
}

unsafe fn attach_widget_window(hwnd: HWND, parent: HWND) -> Result<(), u32> {
    let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
    SetWindowLongPtrW(hwnd, GWL_STYLE, ((style & !WS_POPUP) | WS_CHILD) as isize);
    SetParent(hwnd, parent);
    // A null return can also mean successful reparenting from the desktop.
    if GetParent(hwnd) != parent {
        let error = GetLastError();
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        return Err(error);
    }
    Ok(())
}

unsafe fn position_over_taskbar(hwnd: HWND, shared: &NativeWidgetShared) {
    let taskbars = taskbar::taskbar_windows_all();
    if taskbars.is_empty() {
        ShowWindow(hwnd, SW_HIDE);
        if shared.dock_available.swap(false, Ordering::AcqRel) {
            crate::logging::write("native widget: Windows 작업표시줄을 찾을 수 없습니다.");
        }
        return;
    }

    let mut current = RECT::default();
    let has_current = GetWindowRect(hwnd, &mut current) != 0;
    let center_x = if has_current {
        current.left + (current.right - current.left) / 2
    } else {
        0
    };
    let center_y = if has_current {
        current.top + (current.bottom - current.top) / 2
    } else {
        0
    };
    let taskbar = taskbars
        .into_iter()
        .min_by_key(|window| distance_to_bounds(center_x, center_y, window.bounds))
        .unwrap();
    let taskbar_bounds = taskbar.bounds;
    // Keep the layered widget directly under the taskbar. The XAML composition
    // bridge can paint children but does not route native mouse hits to them.
    let parent = taskbar.hwnd as HWND;
    let parent_changed = GetParent(hwnd) != parent;
    if parent_changed {
        if let Err(error) = attach_widget_window(hwnd, parent) {
            ShowWindow(hwnd, SW_HIDE);
            if shared.dock_available.swap(false, Ordering::AcqRel) {
                crate::logging::write(format!("native widget SetParent failed: {error}"));
            }
            return;
        }
        if let Err(error) = reset_widget_layer(hwnd) {
            ShowWindow(hwnd, SW_HIDE);
            crate::logging::write(format!("native widget layer setup failed: {error}"));
            return;
        }
        crate::logging::write(format!(
            "native widget attached as taskbar child; taskbar_hwnd={:#x}",
            taskbar.hwnd
        ));
    }
    if !shared.dock_available.swap(true, Ordering::AcqRel) {
        crate::logging::write("native widget: taskbar docking recovered");
    }

    let dpi = GetDpiForWindow(parent).max(96);
    let scale = dpi as f64 / 96.0;
    let offset = (8.0 * scale).round() as i32;
    let taskbar_width = (taskbar_bounds.right - taskbar_bounds.left).max(1);
    let taskbar_height = (taskbar_bounds.bottom - taskbar_bounds.top).max(1);

    let (x, y, width, height) = if taskbar_width >= taskbar_height {
        let width = ((LOGICAL_WIDTH as f64 * scale).round() as i32).min(taskbar_width);
        let x = (taskbar_bounds.left + offset).clamp(
            taskbar_bounds.left,
            (taskbar_bounds.right - width).max(taskbar_bounds.left),
        );
        (x, taskbar_bounds.top, width, taskbar_height)
    } else {
        let height = ((LOGICAL_HEIGHT as f64 * scale).round() as i32).min(taskbar_height);
        let y = (taskbar_bounds.top + offset).clamp(
            taskbar_bounds.top,
            (taskbar_bounds.bottom - height).max(taskbar_bounds.top),
        );
        (taskbar_bounds.left, y, taskbar_width, height)
    };

    let desired = Bounds {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    };
    let mut bounds_changed = true;
    if let Ok(mut previous) = shared.last_bounds.lock() {
        let changed = previous
            .map(|bounds| {
                bounds.left != desired.left
                    || bounds.top != desired.top
                    || bounds.right != desired.right
                    || bounds.bottom != desired.bottom
            })
            .unwrap_or(true);
        if changed {
            crate::logging::write(format!(
                "native widget positioned; x={}, y={}, width={}, height={}",
                desired.left, desired.top, width, height
            ));
            *previous = Some(desired);
        }
        bounds_changed = changed;
    }

    if parent_changed || bounds_changed {
        // GetWindowRect and popup anchors use screen coordinates; child window
        // placement uses the taskbar's client coordinates (also on other monitors).
        let mut origin = POINT { x, y };
        if ScreenToClient(parent, &mut origin) == 0 {
            ShowWindow(hwnd, SW_HIDE);
            if let Ok(mut previous) = shared.last_bounds.lock() {
                *previous = None;
            }
            return;
        }
        if SetWindowPos(
            hwnd,
            HWND_TOP,
            origin.x,
            origin.y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
        ) == 0
        {
            ShowWindow(hwnd, SW_HIDE);
            if let Ok(mut previous) = shared.last_bounds.lock() {
                *previous = None;
            }
            return;
        }
    }
    // Check this window's own visibility bit, not IsWindowVisible: the latter
    // includes parent visibility and must not undo taskbar auto-hide/fullscreen.
    if shared.visible.load(Ordering::Acquire)
        && GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_VISIBLE == 0
    {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

unsafe fn paint_widget(hwnd: HWND, shared: &NativeWidgetShared) {
    static PAINT_COUNT: AtomicU32 = AtomicU32::new(0);
    let first_paint = PAINT_COUNT.fetch_add(1, Ordering::Relaxed) == 0;
    let mut paint = PAINTSTRUCT::default();
    let target_dc = BeginPaint(hwnd, &mut paint);
    if target_dc.is_null() {
        return;
    }

    let mut client = RECT::default();
    GetClientRect(hwnd, &mut client);
    let width = (client.right - client.left).max(1);
    let height = (client.bottom - client.top).max(1);

    let memory_dc = CreateCompatibleDC(target_dc);
    let bitmap = CreateCompatibleBitmap(target_dc, width, height);
    if memory_dc.is_null() || bitmap.is_null() {
        EndPaint(hwnd, &paint);
        return;
    }
    let previous_bitmap = SelectObject(memory_dc, bitmap);

    let dpi = GetDpiForWindow(hwnd).max(96);
    let scale = dpi as f64 / 96.0;
    let layout_height = (LOGICAL_HEIGHT as f64 * scale).round() as i32;
    let y_offset = ((height - layout_height) / 2).max(0);
    let sx = |value: i32| (value as f64 * scale).round() as i32;

    let model = shared.model.lock().ok();
    let theme = model.as_ref().map(|model| model.theme).unwrap_or_default();
    if !shared.menu_open.load(Ordering::Acquire)
        && !model
            .as_ref()
            .map(|model| model.detail_visible)
            .unwrap_or(false)
    {
        if let Some(color) = taskbar_background_color(hwnd) {
            shared.taskbar_color.store(color, Ordering::Release);
        }
    }
    let cached_color = shared.taskbar_color.load(Ordering::Acquire);
    let background_color = if cached_color != u32::MAX {
        cached_color
    } else if theme == WidgetTheme::Light {
        rgb(235, 238, 242)
    } else {
        rgb(28, 34, 44)
    };
    let background = CreateSolidBrush(background_color);
    FillRect(memory_dc, &client, background);
    DeleteObject(background);
    let red = background_color & 0xff;
    let green = (background_color >> 8) & 0xff;
    let blue = (background_color >> 16) & 0xff;
    let background_is_light = (red * 299 + green * 587 + blue * 114) / 1000 >= 145;
    let (label_color, value_color, secondary_color, separator_color, codex_accent) =
        if background_is_light {
            (
                rgb(44, 52, 66),
                rgb(17, 24, 39),
                rgb(83, 95, 113),
                rgb(154, 164, 178),
                rgb(0, 143, 122),
            )
        } else {
            (
                rgb(215, 224, 237),
                rgb(247, 249, 252),
                rgb(151, 166, 187),
                rgb(55, 67, 83),
                rgb(85, 215, 186),
            )
        };
    let codex = model
        .as_ref()
        .and_then(|model| model.codex_remaining)
        .map(percent_text)
        .unwrap_or_else(|| "-".to_string());
    let reset = model
        .as_ref()
        .and_then(|model| model.resets_at.as_deref())
        .map(countdown_text)
        .unwrap_or_else(|| "-".to_string());
    let cpu = model
        .as_ref()
        .and_then(|model| model.cpu_percent)
        .map(percent_text_f32)
        .unwrap_or_else(|| "-".to_string());
    let memory = model
        .as_ref()
        .and_then(|model| model.memory_percent)
        .map(percent_text_f32)
        .unwrap_or_else(|| "-".to_string());

    let label_size = sx(12);
    let small_size = sx(11);
    let value_size = sx(12);
    let normal_weight = FW_NORMAL as i32;
    let label_font = create_font(label_size, normal_weight);
    let small_font = create_font(small_size, normal_weight);
    let value_font = create_font(value_size, normal_weight);
    SetBkMode(memory_dc, TRANSPARENT as i32);
    let direct_text = crate::direct_text::DirectTextCanvas::begin(
        memory_dc,
        client.left,
        client.top,
        client.right,
        client.bottom,
    )
    .ok();

    draw_text(
        memory_dc,
        direct_text.as_ref(),
        "CODEX",
        sx(20),
        y_offset + sx(4),
        sx(48),
        sx(23),
        label_color,
        label_font,
        label_size,
        normal_weight,
    );
    draw_text(
        memory_dc,
        direct_text.as_ref(),
        &codex,
        sx(70),
        y_offset + sx(4),
        sx(54),
        sx(23),
        value_color,
        value_font,
        value_size,
        normal_weight,
    );
    draw_text(
        memory_dc,
        direct_text.as_ref(),
        "RESET",
        sx(20),
        y_offset + sx(25),
        sx(36),
        sx(18),
        secondary_color,
        small_font,
        small_size,
        normal_weight,
    );
    draw_text(
        memory_dc,
        direct_text.as_ref(),
        &reset,
        sx(57),
        y_offset + sx(25),
        sx(72),
        sx(18),
        codex_accent,
        small_font,
        small_size,
        normal_weight,
    );

    draw_text(
        memory_dc,
        direct_text.as_ref(),
        "CPU",
        sx(155),
        y_offset + sx(3),
        sx(35),
        sx(22),
        label_color,
        label_font,
        label_size,
        normal_weight,
    );
    draw_text(
        memory_dc,
        direct_text.as_ref(),
        &cpu,
        sx(190),
        y_offset + sx(3),
        sx(48),
        sx(22),
        value_color,
        value_font,
        value_size,
        normal_weight,
    );

    draw_text(
        memory_dc,
        direct_text.as_ref(),
        "MEM",
        sx(155),
        y_offset + sx(23),
        sx(35),
        sx(22),
        label_color,
        label_font,
        label_size,
        normal_weight,
    );
    draw_text(
        memory_dc,
        direct_text.as_ref(),
        &memory,
        sx(190),
        y_offset + sx(23),
        sx(48),
        sx(22),
        value_color,
        value_font,
        value_size,
        normal_weight,
    );

    if let Some(canvas) = direct_text.as_ref() {
        let _ = canvas.finish();
    }
    fill(
        memory_dc,
        sx(8),
        y_offset + sx(8),
        sx(3),
        sx(17),
        codex_accent,
    );
    fill(
        memory_dc,
        sx(135),
        y_offset + sx(6),
        1,
        sx(36),
        separator_color,
    );
    fill(
        memory_dc,
        sx(145),
        y_offset + sx(7),
        sx(3),
        sx(15),
        rgb(245, 185, 30),
    );
    fill(
        memory_dc,
        sx(145),
        y_offset + sx(27),
        sx(3),
        sx(15),
        rgb(155, 104, 232),
    );
    // Supply the opaque bitmap directly. SetLayeredWindowAttributes on layered
    // child windows can let clicks pass through despite a visible image.
    let size = SIZE {
        cx: width,
        cy: height,
    };
    let source = POINT { x: 0, y: 0 };
    let painted = UpdateLayeredWindow(
        hwnd,
        null_mut(),
        null(),
        &size,
        memory_dc,
        &source,
        0,
        null(),
        ULW_OPAQUE,
    );
    if first_paint {
        let mut clip = RECT::default();
        let clip_type = windows_sys::Win32::Graphics::Gdi::GetClipBox(target_dc, &mut clip);
        crate::logging::write(format!("native widget first paint: {width}x{height}, copied={painted}, clip={clip_type} ({},{},{},{})", clip.left, clip.top, clip.right, clip.bottom));
    }

    SelectObject(memory_dc, previous_bitmap);
    DeleteObject(bitmap);
    DeleteObject(label_font);
    DeleteObject(small_font);
    DeleteObject(value_font);
    DeleteDC(memory_dc);
    EndPaint(hwnd, &paint);
}

unsafe fn show_context_menu(hwnd: HWND, shared: &NativeWidgetShared) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }

    let (detail_visible, theme, autostart_enabled) = shared
        .model
        .lock()
        .map(|model| (model.detail_visible, model.theme, model.autostart_enabled))
        .unwrap_or((false, WidgetTheme::Dark, false));
    append_menu(
        menu,
        MENU_DETAIL,
        if detail_visible {
            "상세 정보 닫기"
        } else {
            "상세 정보 열기"
        },
    );
    append_menu(menu, MENU_REFRESH, "사용량 새로고침");
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append_menu(menu, MENU_OVERLAY, "자유 배치");
    append_menu(
        menu,
        MENU_THEME,
        if theme == WidgetTheme::Dark {
            "라이트 모드"
        } else {
            "다크 모드"
        },
    );
    let autostart_label = wide("Windows 시작 시 자동 실행");
    AppendMenuW(
        menu,
        MF_STRING | if autostart_enabled { MF_CHECKED } else { 0 },
        MENU_AUTOSTART,
        autostart_label.as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    append_menu(menu, MENU_HIDE, "위젯 숨기기");
    append_menu(menu, MENU_QUIT, "앱 종료");

    let mut cursor = POINT::default();
    GetCursorPos(&mut cursor);
    shared.menu_open.store(true, Ordering::Release);
    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenuEx(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        cursor.x,
        cursor.y,
        hwnd,
        null(),
    ) as usize;
    shared.menu_open.store(false, Ordering::Release);
    record_interaction(shared);
    DestroyMenu(menu);

    let event = match selected {
        MENU_DETAIL => Some(NativeWidgetEvent::ToggleDetail),
        MENU_REFRESH => Some(NativeWidgetEvent::Refresh),
        MENU_OVERLAY => Some(NativeWidgetEvent::SwitchToOverlay),
        MENU_THEME => Some(NativeWidgetEvent::SetTheme(theme.toggled())),
        MENU_AUTOSTART => Some(NativeWidgetEvent::ToggleAutostart),
        MENU_HIDE => Some(NativeWidgetEvent::Hide),
        MENU_QUIT => Some(NativeWidgetEvent::Quit),
        _ => None,
    };
    if let Some(event) = event {
        let _ = shared.events.send(event);
    }
}

unsafe fn append_menu(menu: *mut core::ffi::c_void, id: usize, label: &str) {
    let label = wide(label);
    AppendMenuW(menu, MF_STRING, id, label.as_ptr());
}

unsafe fn create_font(height: i32, weight: i32) -> *mut core::ffi::c_void {
    let face = wide("Malgun Gothic");
    CreateFontW(
        -height.max(1),
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        ANTIALIASED_QUALITY as u32,
        0,
        face.as_ptr(),
    )
}

#[allow(clippy::too_many_arguments)]
unsafe fn draw_text(
    dc: *mut core::ffi::c_void,
    direct_text: Option<&crate::direct_text::DirectTextCanvas>,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: u32,
    font: *mut core::ffi::c_void,
    font_size: i32,
    font_weight: i32,
) {
    if direct_text
        .map(|canvas| {
            canvas
                .draw(text, x, y, width, height, color, font_size, font_weight)
                .is_ok()
        })
        .unwrap_or(false)
    {
        return;
    }
    let previous = SelectObject(dc, font);
    SetTextColor(dc, color);
    let text = text.encode_utf16().collect::<Vec<_>>();
    let mut rect = RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    };
    DrawTextW(
        dc,
        text.as_ptr(),
        text.len() as i32,
        &mut rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(dc, previous);
}

unsafe fn fill(dc: *mut core::ffi::c_void, x: i32, y: i32, width: i32, height: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let rect = RECT {
        left: x,
        top: y,
        right: x + width.max(1),
        bottom: y + height.max(1),
    };
    FillRect(dc, &rect, brush);
    DeleteObject(brush);
}

fn countdown_text(value: &str) -> String {
    let Ok(reset) = DateTime::parse_from_rfc3339(value) else {
        return "-".to_string();
    };
    let remaining = reset.with_timezone(&Utc) - Utc::now();
    let total_minutes = remaining.num_minutes().max(0);
    let total_hours = total_minutes / 60;
    let days = total_hours / 24;
    let hours = total_hours % 24;
    let minutes = total_minutes % 60;
    if days > 0 {
        format!("{days}일 {hours}시간")
    } else {
        format!("{hours:02}:{minutes:02}")
    }
}

fn percent_text(value: f64) -> String {
    format!("{:.0} %", value.clamp(0.0, 100.0))
}

fn percent_text_f32(value: f32) -> String {
    format!("{:.0} %", value.clamp(0.0, 100.0))
}

fn distance_to_bounds(x: i32, y: i32, bounds: Bounds) -> i64 {
    let dx = if x < bounds.left {
        bounds.left - x
    } else if x > bounds.right {
        x - bounds.right
    } else {
        0
    };
    let dy = if y < bounds.top {
        bounds.top - y
    } else if y > bounds.bottom {
        y - bounds.bottom
    } else {
        0
    };
    i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)
}

fn taskbar_created_message() -> u32 {
    static MESSAGE: AtomicU32 = AtomicU32::new(0);
    let current = MESSAGE.load(Ordering::Acquire);
    if current != 0 {
        return current;
    }
    let name = wide("TaskbarCreated");
    let registered = unsafe { RegisterWindowMessageW(name.as_ptr()) };
    if registered != 0 {
        MESSAGE.store(registered, Ordering::Release);
    }
    registered
}

fn record_interaction(shared: &NativeWidgetShared) {
    shared
        .last_interaction_ms
        .store(unix_millis(), Ordering::Release);
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod docking_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsWindow, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOPMOST,
    };

    struct TestWindow(HWND);

    impl TestWindow {
        unsafe fn new() -> Self {
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                wide("STATIC").as_ptr(),
                wide("Codex Pulse hidden docking test").as_ptr(),
                WS_POPUP,
                100,
                200,
                500,
                48,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null());
            Self(hwnd)
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                if IsWindow(self.0) != 0 {
                    DestroyWindow(self.0);
                }
            }
        }
    }

    #[test]
    fn docked_window_inherits_parent_visibility_and_lifetime() {
        unsafe {
            // Use hidden test windows: do not hide or restart the user's Explorer.
            let parent = TestWindow::new();
            let widget = TestWindow::new();
            attach_widget_window(widget.0, parent.0).unwrap();
            assert_eq!(GetParent(widget.0), parent.0);
            let style = GetWindowLongPtrW(widget.0, GWL_STYLE) as u32;
            assert_ne!(style & WS_CHILD, 0);
            assert_eq!(style & WS_POPUP, 0);
            assert_eq!(
                GetWindowLongPtrW(widget.0, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                0
            );

            let mut parent_rect = RECT::default();
            assert_ne!(GetWindowRect(parent.0, &mut parent_rect), 0);
            let mut origin = POINT {
                x: parent_rect.left + 8,
                y: parent_rect.top,
            };
            assert_ne!(ScreenToClient(parent.0, &mut origin), 0);
            assert_ne!(
                SetWindowPos(
                    widget.0,
                    HWND_TOP,
                    origin.x,
                    origin.y,
                    306,
                    48,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED
                ),
                0
            );
            let mut actual = RECT::default();
            assert_ne!(GetWindowRect(widget.0, &mut actual), 0);
            assert_eq!(actual.left, parent_rect.left + 8);
            assert_eq!(actual.top, parent_rect.top);

            ShowWindow(widget.0, SW_SHOWNOACTIVATE);
            assert_ne!(
                GetWindowLongPtrW(widget.0, GWL_STYLE) as u32 & WS_VISIBLE,
                0
            );
            assert_eq!(
                IsWindowVisible(widget.0),
                0,
                "hidden parent must hide child"
            );
            drop(parent);
            assert_eq!(
                IsWindow(widget.0),
                0,
                "parent destruction must destroy child"
            );
        }
    }

    #[test]
    fn failed_attachment_restores_popup_style() {
        unsafe {
            let widget = TestWindow::new();
            let original = GetWindowLongPtrW(widget.0, GWL_STYLE);
            assert!(attach_widget_window(widget.0, -1isize as HWND).is_err());
            assert_eq!(GetWindowLongPtrW(widget.0, GWL_STYLE), original);
            assert!(GetParent(widget.0).is_null());
        }
    }
}
