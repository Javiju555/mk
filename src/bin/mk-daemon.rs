#[cfg(target_os = "linux")]
mod linux {
    // Include the actual Linux daemon logic
    include!("../mk-daemon-linux.rs");
}

fn main() {
    #[cfg(target_os = "linux")]
    {
        linux::main();
    }

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("mk-daemon is only supported on Linux.");
        std::process::exit(1);
    }
}
