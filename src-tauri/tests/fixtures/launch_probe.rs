use std::{
    ffi::c_void,
    fs, thread,
    time::{Duration, Instant},
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetStdHandle(kind: u32) -> *mut c_void;
    fn GetConsoleMode(handle: *mut c_void, mode: *mut u32) -> i32;
}

fn main() {
    let mut mode = 0;
    let stdin_console = unsafe { GetConsoleMode(GetStdHandle(-10i32 as u32), &mut mode) != 0 };
    let stdout_console = unsafe { GetConsoleMode(GetStdHandle(-11i32 as u32), &mut mode) != 0 };
    println!("MCDH isolated launch probe");
    fs::write(
        "probe-state.txt",
        format!(
            "{}\n{}\n{}",
            std::env::current_dir().unwrap().display(),
            stdin_console,
            stdout_console
        ),
    )
    .unwrap();
    let start = Instant::now();
    while !std::path::Path::new("probe-stop").exists() && start.elapsed() < Duration::from_secs(30)
    {
        thread::sleep(Duration::from_millis(50));
    }
    std::process::exit(7);
}
