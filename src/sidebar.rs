use std::{
    mem,
    ops::DerefMut,
    os::raw::c_void,
    sync::{Arc, RwLock},
};

use lazy_static::lazy_static;
use parking_lot::Mutex;
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::{GetLastError, SetLastError, HWND, LPARAM, RECT, WIN32_ERROR},
        Graphics::Dwm::{
            DwmSetWindowAttribute, DWMNCRENDERINGPOLICY, DWMNCRP_ENABLED, DWMWA_DISALLOW_PEEK,
            DWMWA_EXCLUDED_FROM_PEEK,
        },
        UI::{
            Shell::{
                SHAppBarMessage, ABE_LEFT, ABE_RIGHT, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE,
                ABM_SETPOS, APPBARDATA,
            },
            WindowsAndMessaging::{
                FindWindowA, GetWindowLongPtrA, SetWindowLongPtrA, GWL_EXSTYLE, WS_EX_APPWINDOW,
                WS_EX_TOOLWINDOW,
            },
        },
    },
};

use crate::{settings::MySettings, MyApp, INTERNAL_WINDOW_TITLE};

lazy_static! {
    pub static ref STATIC_HWND: RwLock<HWND> = HWND(0).into();
}

// #[no_mangle]
// pub unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> windows::Win32::Foundation::BOOL {
//     let cbi_addr = lparam.0 as *mut CallbackInfo;
//     let mut cbi = &mut *cbi_addr;
//     let win_pid = GetWindowThreadProcessId(hwnd, None);
//     if win_pid == cbi.mypid {
//         println!("FOUND: cbi: {cbi:?}, {hwnd:?}, pid: {win_pid}");
//         cbi.return_hwnd = Some(hwnd);
//         return windows::Win32::Foundation::BOOL(0);
//     }

//     println!("cbi: {cbi:?}, {hwnd:?}, pid: {win_pid}");
//     windows::Win32::Foundation::BOOL(1)
// }

// #[derive(Debug)]
// struct CallbackInfo {
//     mypid: u32,
//     return_hwnd: Option<HWND>,
// }

pub(crate) fn setup_sidebar(appdata: &MyApp, scale_override: Option<f32>) {
    // find handle: enum active windows, find window with my process id
    // let pid = std::process::id();

    // let mut cbi = CallbackInfo {
    //     mypid: pid,
    //     return_hwnd: None,
    // };

    // unsafe {
    //     EnumWindows(Some(cb), LPARAM(&mut cbi as *mut CallbackInfo as isize));
    // }

    let title = PCSTR::from_raw(INTERNAL_WINDOW_TITLE.as_bytes().as_ptr());
    let hwnd = unsafe { FindWindowA(None, title) };
    // dbg!(hwnd);

    // let active_window = active_win_pos_rs::get_active_window().expect("Active window should exist");
    // println!("active window: {active_window:#?}");
    // let handle = active_window
    //     .window_id
    //     .replace("HWND(", "")
    //     .replace(')', "")
    //     .parse::<isize>()
    //     .expect("handle should be valid isize");

    *STATIC_HWND.write().unwrap().deref_mut() = hwnd;

    let settings = appdata.settings.lock();

    let location = &settings.current_settings.location;

    let scale = scale_override.unwrap_or(1.0);

    let rect = RECT {
        left: location.x as i32,
        top: location.y as i32,
        right: location.x as i32 + (location.width * scale) as i32,
        bottom: location.y as i32 + (location.height * scale) as i32,
    };

    let lparam = LPARAM(0);

    let mut appbardata = APPBARDATA {
        cbSize: mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: 0,
        uEdge: if settings.current_settings.display_right {
            ABE_RIGHT
        } else {
            ABE_LEFT
        },
        rc: rect,
        lParam: lparam,
    };

    let mut dwmncrp_enabled_orig = DWMNCRP_ENABLED;
    let dwmncrp_enabled: *mut c_void = &mut dwmncrp_enabled_orig as *mut _ as *mut c_void;

    unsafe {
        let abd_ptr = &mut appbardata as *mut APPBARDATA;
        SHAppBarMessage(ABM_NEW, abd_ptr);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_EXCLUDED_FROM_PEEK,
            dwmncrp_enabled,
            mem::size_of::<DWMNCRENDERINGPOLICY>() as u32,
        )
        .expect("DWMWA_EXCLUDED_FROM_PEEK should work");
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_DISALLOW_PEEK,
            dwmncrp_enabled,
            mem::size_of::<DWMNCRENDERINGPOLICY>() as u32,
        )
        .expect("DWMWA_EXCLUDED_FROM_PEEK should work");
        SHAppBarMessage(ABM_QUERYPOS, abd_ptr);
        SHAppBarMessage(ABM_SETPOS, abd_ptr);
    }
    set_window_unpeekable(hwnd);
    set_window_unpeekable(hwnd);
}

pub fn dispose_sidebar(settings: Arc<Mutex<MySettings>>) {
    let settings = settings.lock();

    let hwnd = *STATIC_HWND.read().unwrap();

    let rect = RECT {
        left: settings.current_settings.location.x as i32,
        top: 0,
        right: settings.current_settings.location.x as i32
            + settings.current_settings.location.width as i32,
        bottom: i32::MAX,
    };

    let lparam = LPARAM(0);

    let mut appbardata = APPBARDATA {
        cbSize: mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: 0,
        uEdge: if settings.current_settings.display_right {
            ABE_RIGHT
        } else {
            ABE_LEFT
        },
        rc: rect,
        lParam: lparam,
    };

    unsafe {
        let abd_ptr = &mut appbardata as *mut APPBARDATA;
        SHAppBarMessage(ABM_REMOVE, abd_ptr);
    }
}

fn set_window_unpeekable(handle: HWND) {
    let exstyle = unsafe { GetWindowLongPtrA(handle, GWL_EXSTYLE) };
    print_last_error();

    // unset appwindow: Remove from taskbar. set toolwindow: remove from alt-tab
    let mut new_exstyle = exstyle | WS_EX_TOOLWINDOW.0 as isize;
    new_exstyle &= !WS_EX_APPWINDOW.0 as isize;

    unsafe {
        SetWindowLongPtrA(handle, GWL_EXSTYLE, new_exstyle);
    }
    print_last_error();
}

fn print_last_error() {
    let e = unsafe { GetLastError() };
    if e.0 != 0 {
        println!("{e:?}");
    }
    unsafe {
        SetLastError(WIN32_ERROR(0));
    }
}
