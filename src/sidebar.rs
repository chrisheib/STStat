use std::{mem, ops::DerefMut, os::raw::c_void, sync::RwLock};

use lazy_static::lazy_static;
use windows::Win32::{
    Foundation::{GetLastError, SetLastError, HWND, LPARAM, RECT, WIN32_ERROR},
    Graphics::Dwm::{
        DwmSetWindowAttribute, DWMNCRENDERINGPOLICY, DWMNCRP_ENABLED, DWMWA_DISALLOW_PEEK,
        DWMWA_EXCLUDED_FROM_PEEK,
    },
    UI::{
        Shell::{
            SHAppBarMessage, ABE_LEFT, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA,
        },
        WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
        },
    },
};

use crate::{EDGE, POS, SIZE};

lazy_static! {
    static ref STATIC_HWND: RwLock<isize> = 0.into();
}

pub(crate) fn setup_sidebar() {
    let active_window = active_win_pos_rs::get_active_window().expect("Active window should exist");
    println!("active window: {active_window:#?}");
    let handle = active_window
        .window_id
        .replace("HWND(", "")
        .replace(')', "")
        .parse::<isize>()
        .expect("handle should be valid isize");

    *STATIC_HWND.write().unwrap().deref_mut() = handle;

    let hwnd = HWND(handle);

    let rect = RECT {
        left: POS.x as i32,
        top: 0,
        right: POS.x as i32 + SIZE.x as i32,
        bottom: i32::MAX,
    };

    let lparam = LPARAM(0);

    let mut appbardata = APPBARDATA {
        cbSize: mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: 0,
        uEdge: EDGE,
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

pub fn dispose_sidebar() {
    let hwnd = HWND(*STATIC_HWND.read().unwrap());

    let rect = RECT {
        left: 0,
        top: 0,
        right: 150,
        bottom: 10000,
    };

    let lparam = LPARAM(0);

    let mut appbardata = APPBARDATA {
        cbSize: mem::size_of::<APPBARDATA>() as u32,
        hWnd: hwnd,
        uCallbackMessage: 0,
        uEdge: ABE_LEFT,
        rc: rect,
        lParam: lparam,
    };

    unsafe {
        let abd_ptr = &mut appbardata as *mut APPBARDATA;
        SHAppBarMessage(ABM_REMOVE, abd_ptr);
    }
}

fn set_window_unpeekable(handle: HWND) {
    let exstyle = unsafe { GetWindowLongPtrW(handle, GWL_EXSTYLE) };
    print_last_error();

    // unset appwindow: Remove from taskbar. set toolwindow: remove from alt-tab
    let mut new_exstyle = exstyle | WS_EX_TOOLWINDOW.0 as isize;
    new_exstyle &= !WS_EX_APPWINDOW.0 as isize;

    unsafe {
        SetWindowLongPtrW(handle, GWL_EXSTYLE, new_exstyle);
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
