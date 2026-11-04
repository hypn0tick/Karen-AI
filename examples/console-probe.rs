//! Prints whether this process has a console window: `window=none` or `window=visible`. Used to pin
//! that Karen starts her children without putting a window on the owner's desktop (a child started
//! with `CREATE_NO_WINDOW` has none). Off Windows there is never one.
//!
//! * no argument: print this process's own state.
//! * `relay`: start another copy of itself the way Karen starts children (`karen::proc::command`)
//!   and print what it reports. `relay-flags` does the same with an extra creation flag added.
//!
//! The test starts the relay as a *detached* process, which has no console at all: that is the case
//! where a child started without the flag gets a window of its own. The Karen who counts the windows.

#[cfg(windows)]
#[allow(unsafe_code)]
fn window() -> &'static str {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleWindow() -> *mut std::ffi::c_void;
    }
    // SAFETY: takes no arguments and only returns a window handle value, or null.
    if unsafe { GetConsoleWindow() }.is_null() {
        "none"
    } else {
        "visible"
    }
}

#[cfg(not(windows))]
fn window() -> &'static str {
    "none"
}

fn main() {
    let mode = std::env::args().nth(1);
    let Some(mode) = mode else {
        println!("window={}", window());
        return;
    };
    let me = std::env::current_exe().expect("this program has a path");
    let mut child = karen::proc::command(me);
    if mode == "relay-flags" {
        karen::proc::flags(&mut child, karen::proc::CREATE_NEW_PROCESS_GROUP);
    }
    let output = child.output().expect("the child runs");
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
