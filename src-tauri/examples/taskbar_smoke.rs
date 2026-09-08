//! Live Windows smoke test. Start the widget first, then pass its PID.
//! Uses a temporary non-topmost fullscreen window; never restarts Explorer.
#[cfg(not(windows))]
fn main() {
    panic!("This smoke test requires Windows");
}

#[cfg(windows)]
fn main() {
    if let Err(error) = unsafe { live::run() } {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod live {
    use std::{
        ptr::{null, null_mut},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::Gdi::{
            GetDC, GetMonitorInfoW, GetPixel, GetStockObject, InvalidateRect, MonitorFromWindow,
            ReleaseDC, BLACK_BRUSH, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            Input::KeyboardAndMouse::{
                mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN,
                MOUSEEVENTF_RIGHTUP,
            },
            WindowsAndMessaging::*,
        },
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> i32 {
        (*(data as *mut Vec<HWND>)).push(hwnd);
        1
    }

    unsafe fn find(
        process_id: u32,
        class: Option<&str>,
        title: Option<&str>,
    ) -> Result<HWND, String> {
        let mut roots = Vec::<HWND>::new();
        EnumWindows(Some(collect), &mut roots as *mut _ as LPARAM);
        let mut windows = roots.clone();
        for root in roots {
            EnumChildWindows(root, Some(collect), &mut windows as *mut _ as LPARAM);
        }
        let matches: Vec<_> = windows
            .into_iter()
            .filter(|hwnd| {
                let mut pid = 0;
                GetWindowThreadProcessId(*hwnd, &mut pid);
                if pid != process_id {
                    return false;
                }
                let mut name = [0u16; 256];
                let length = GetClassNameW(*hwnd, name.as_mut_ptr(), name.len() as i32);
                if class.is_some_and(|class| {
                    String::from_utf16_lossy(&name[..length as usize]) != class
                }) {
                    return false;
                }
                let length = GetWindowTextW(*hwnd, name.as_mut_ptr(), name.len() as i32);
                !title.is_some_and(|title| {
                    String::from_utf16_lossy(&name[..length as usize]) != title
                })
            })
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "expected one target window, found {}",
                matches.len()
            ));
        }
        Ok(matches[0])
    }

    unsafe fn pump_for(duration: Duration) {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn check(ok: bool, label: &str) -> Result<(), String> {
        if !ok {
            return Err(label.to_owned());
        }
        println!("PASS: {label}");
        Ok(())
    }

    struct Fullscreen(HWND);
    impl Drop for Fullscreen {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    unsafe fn fullscreen(widget: HWND) -> Result<Fullscreen, String> {
        let monitor = MonitorFromWindow(widget, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        check(
            GetMonitorInfoW(monitor, &mut info) != 0,
            "target monitor geometry",
        )?;
        let instance = GetModuleHandleW(null());
        let name = wide("CodexPulseFullscreenSmokeTest");
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            hbrBackground: GetStockObject(BLACK_BRUSH) as _,
            lpszClassName: name.as_ptr(),
            ..Default::default()
        };
        check(RegisterClassExW(&class) != 0, "fullscreen test class")?;
        let bounds = info.rcMonitor;
        let hwnd = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("Codex Pulse fullscreen smoke test").as_ptr(),
            WS_POPUP,
            bounds.left,
            bounds.top,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        check(!hwnd.is_null(), "fullscreen test window created")?;
        let window = Fullscreen(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        Ok(window)
    }

    struct CursorRestore(POINT);
    impl Drop for CursorRestore {
        fn drop(&mut self) {
            unsafe {
                SetCursorPos(self.0.x, self.0.y);
            }
        }
    }

    unsafe fn click(point: POINT, right: bool) {
        SetCursorPos(point.x, point.y);
        mouse_event(
            if right {
                MOUSEEVENTF_RIGHTDOWN
            } else {
                MOUSEEVENTF_LEFTDOWN
            },
            0,
            0,
            0,
            0,
        );
        pump_for(Duration::from_millis(80));
        mouse_event(
            if right {
                MOUSEEVENTF_RIGHTUP
            } else {
                MOUSEEVENTF_LEFTUP
            },
            0,
            0,
            0,
            0,
        );
        pump_for(Duration::from_millis(800));
    }

    pub unsafe fn run() -> Result<(), String> {
        let process_id: u32 = std::env::args()
            .nth(1)
            .ok_or("usage: taskbar_smoke <widget-pid>")?
            .parse()
            .map_err(|_| "invalid PID")?;
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let widget = find(process_id, Some("CodexPulseNativeTaskbarWidget"), None)?;
        let detail = find(process_id, None, Some("Codex Pulse Details"))?;
        let parent = GetAncestor(widget, GA_ROOT);
        let mut parent_class = [0u16; 128];
        let length = GetClassNameW(parent, parent_class.as_mut_ptr(), parent_class.len() as i32);
        let parent_class = String::from_utf16_lossy(&parent_class[..length as usize]);
        println!("widget={widget:?}; parent={parent:?}; parent_class={parent_class}");
        check(
            matches!(
                parent_class.as_str(),
                "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ),
            "real Explorer taskbar ancestor",
        )?;
        check(
            GetWindowLongPtrW(widget, GWL_STYLE) as u32 & WS_CHILD != 0,
            "WS_CHILD enabled",
        )?;
        check(
            GetWindowLongPtrW(widget, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST == 0,
            "widget is not topmost",
        )?;
        check(IsWindowVisible(widget) != 0, "widget visible")?;
        let mut bounds = RECT::default();
        check(
            GetWindowRect(widget, &mut bounds) != 0,
            "widget bounds available",
        )?;
        let point = POINT {
            x: bounds.left + 30,
            y: bounds.top + (bounds.bottom - bounds.top) / 2,
        };
        let hit = WindowFromPoint(point);
        println!("widget point=({}, {}); hit={hit:?}", point.x, point.y);
        check(
            hit == widget || IsChild(widget, hit) != 0,
            "widget receives pointer hit at its visible position",
        )?;
        let dc = GetDC(null_mut());
        let accent = GetPixel(dc, bounds.left + 9, bounds.top + 9);
        ReleaseDC(null_mut(), dc);
        let red = accent & 255;
        let green = (accent >> 8) & 255;
        let blue = (accent >> 16) & 255;
        check(
            red < 150 && green > 130 && blue > 80 && blue < 220,
            "mint status accent is actually painted on the screen",
        )?;
        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor);
        let _restore = CursorRestore(cursor);
        if IsWindowVisible(detail) != 0 {
            click(point, false);
        }
        check(IsWindowVisible(detail) == 0, "detail closed before testing")?;
        click(point, false);
        check(IsWindowVisible(detail) != 0, "real left click opens detail")?;
        let mut detail_bounds = RECT::default();
        GetWindowRect(detail, &mut detail_bounds);
        check(
            detail_bounds.bottom == bounds.top
                || detail_bounds.top == bounds.bottom
                || detail_bounds.left == bounds.right
                || detail_bounds.right == bounds.left,
            "detail panel remains adjacent to widget",
        )?;
        click(point, false);
        check(
            IsWindowVisible(detail) == 0,
            "second real left click closes detail",
        )?;
        let dc = GetDC(null_mut());
        let background_before = GetPixel(dc, bounds.left + 2, bounds.top + 2);
        ReleaseDC(null_mut(), dc);
        click(point, true);
        let menu = find(process_id, Some("#32768"), None)?;
        check(
            IsWindowVisible(menu) != 0,
            "real right click opens context menu",
        )?;
        InvalidateRect(widget, null(), 0);
        pump_for(Duration::from_millis(1100));
        let dc = GetDC(null_mut());
        let background_during_menu = GetPixel(dc, bounds.left + 2, bounds.top + 2);
        ReleaseDC(null_mut(), dc);
        check(
            background_before == background_during_menu,
            "background stays stable while menu covers sample point",
        )?;
        PostMessageW(widget, WM_CANCELMODE, 0, 0);
        pump_for(Duration::from_millis(500));
        check(IsWindowVisible(menu) == 0, "test context menu dismissed")?;

        let window = fullscreen(widget)?;
        let mut fullscreen_bounds = RECT::default();
        GetWindowRect(window.0, &mut fullscreen_bounds);
        SetCursorPos(
            (fullscreen_bounds.left + fullscreen_bounds.right) / 2,
            (fullscreen_bounds.top + fullscreen_bounds.bottom) / 2,
        );
        pump_for(Duration::from_millis(500));
        SetForegroundWindow(window.0);
        println!("WAIT: activate 'Codex Pulse fullscreen smoke test' within 60 seconds");
        let activation_deadline = Instant::now() + Duration::from_secs(60);
        let mut stable_since = None;
        while Instant::now() < activation_deadline {
            if GetForegroundWindow() == window.0 {
                let since = stable_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(750) {
                    break;
                }
            } else {
                stable_since = None;
            }
            pump_for(Duration::from_millis(100));
        }
        check(
            GetForegroundWindow() == window.0,
            "test fullscreen window is foreground",
        )?;
        check(
            GetWindowLongPtrW(window.0, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST == 0,
            "fullscreen test does not force topmost",
        )?;
        for _ in 0..15 {
            let hit = WindowFromPoint(point);
            if GetAncestor(hit, GA_ROOT) != window.0 {
                return Err(format!(
                    "widget position not covered by fullscreen: hit={hit:?}"
                ));
            }
            pump_for(Duration::from_millis(200));
        }
        println!("PASS: fullscreen covers widget continuously for 3 seconds");
        drop(window);
        pump_for(Duration::from_secs(2));
        check(
            IsWindowVisible(widget) != 0,
            "widget still visible after fullscreen closes",
        )?;
        let hit = WindowFromPoint(point);
        check(
            hit == widget || IsChild(widget, hit) != 0,
            "widget receives pointer hits again after fullscreen",
        )?;
        Ok(())
    }
}
