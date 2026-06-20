use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

const SOCKET_PATH: &str = "/tmp/mk-daemon.sock";
const UINPUT_NAME: &str = "mk-virtual-keyboard";

// Linux input event codes
const EV_KEY: u16 = 0x01;
const EV_SYN: u16 = 0x00;
const SYN_REPORT: u16 = 0x00;
const KEY_A: u16 = 30;
const KEY_B: u16 = 48;
const KEY_C: u16 = 46;
const KEY_D: u16 = 32;
const KEY_E: u16 = 18;
const KEY_F: u16 = 33;
const KEY_G: u16 = 34;
const KEY_H: u16 = 35;
const KEY_I: u16 = 23;
const KEY_J: u16 = 36;
const KEY_K: u16 = 37;
const KEY_L: u16 = 38;
const KEY_M: u16 = 50;
const KEY_N: u16 = 49;
const KEY_O: u16 = 24;
const KEY_P: u16 = 25;
const KEY_Q: u16 = 16;
const KEY_R: u16 = 19;
const KEY_S: u16 = 31;
const KEY_T: u16 = 20;
const KEY_U: u16 = 22;
const KEY_V: u16 = 47;
const KEY_W: u16 = 17;
const KEY_X: u16 = 45;
const KEY_Y: u16 = 21;
const KEY_Z: u16 = 44;
const KEY_1: u16 = 2;
const KEY_2: u16 = 3;
const KEY_3: u16 = 4;
const KEY_4: u16 = 5;
const KEY_5: u16 = 6;
const KEY_6: u16 = 7;
const KEY_7: u16 = 8;
const KEY_8: u16 = 9;
const KEY_9: u16 = 10;
const KEY_0: u16 = 11;
const KEY_ENTER: u16 = 28;
const KEY_ESC: u16 = 1;
const KEY_BACKSPACE: u16 = 14;
const KEY_TAB: u16 = 15;
const KEY_SPACE: u16 = 57;
const KEY_DELETE: u16 = 111;
const KEY_UP: u16 = 103;
const KEY_DOWN: u16 = 108;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTALT: u16 = 56;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_LEFTMETA: u16 = 125;
const KEY_F1: u16 = 59;
const KEY_F2: u16 = 60;
const KEY_F3: u16 = 61;
const KEY_F4: u16 = 62;
const KEY_F5: u16 = 63;
const KEY_F6: u16 = 64;
const KEY_F7: u16 = 65;
const KEY_F8: u16 = 66;
const KEY_F9: u16 = 67;
const KEY_F10: u16 = 68;
const KEY_F11: u16 = 87;
const KEY_F12: u16 = 88;
const KEY_F13: u16 = 183;
const KEY_F14: u16 = 184;
const KEY_F15: u16 = 185;
const KEY_MINUS: u16 = 12;
const KEY_EQUAL: u16 = 13;
const KEY_LEFTBRACE: u16 = 26;
const KEY_RIGHTBRACE: u16 = 27;
const KEY_SEMICOLON: u16 = 39;
const KEY_APOSTROPHE: u16 = 40;
const KEY_GRAVE: u16 = 41;
const KEY_BACKSLASH: u16 = 43;
const KEY_COMMA: u16 = 51;
const KEY_DOT: u16 = 52;
const KEY_SLASH: u16 = 53;

// uinput constants
const UINPUT_MAX_NAME_SIZE: usize = 80;
const UI_SET_EVBIT: libc::c_ulong = 0x40045564;  // _IOW('U', 100, int)
const UI_SET_KEYBIT: libc::c_ulong = 0x40045565; // _IOW('U', 101, int)
const UI_DEV_SETUP: libc::c_ulong = 0x405c5503;  // _IOW('U', 3, struct uinput_setup)
const UI_DEV_CREATE: libc::c_ulong = 0x5501;     // _IO('U', 1) — takes NO argument

// struct uinput_setup — passed to ioctl(UI_DEV_SETUP) on kernel >= 4.5.
// Modern flow: UI_SET_*BIT -> UI_DEV_SETUP(&setup) -> UI_DEV_CREATE (no arg).
// UI_DEV_CREATE itself takes no argument and only succeeds once the device
// state is SETUP_COMPLETE, which UI_DEV_SETUP establishes. Passing the struct
// straight to UI_DEV_CREATE (the old assumption) leaves the device unconfigured
// and the kernel returns EINVAL.
#[repr(C)]
struct UinputSetup {
    id: UinputId,
    name: [u8; UINPUT_MAX_NAME_SIZE],
    ff_effects_max: u32,
}

#[repr(C)]
struct UinputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct InputEvent {
    time: Timeval,
    type_: u16,
    code: u16,
    value: i32,
}

#[repr(C)]
struct Timeval {
    tv_sec: libc::time_t,
    tv_usec: libc::suseconds_t,
}

fn char_to_keycode(c: char) -> Option<(u16, bool)> {
    // Returns (keycode, needs_shift)
    match c {
        // evdev letter keycodes are NOT sequential (KEY_A=30 but KEY_B=48),
        // so map each letter by name instead of doing KEY_A + offset.
        'a'..='z' => keyname_to_keycode(&c.to_string()).map(|k| (k, false)),
        'A'..='Z' => keyname_to_keycode(&c.to_ascii_lowercase().to_string()).map(|k| (k, true)),
        '1'..='9' => Some((KEY_1 + (c as u16 - '1' as u16), false)),
        '0' => Some((KEY_0, false)),
        ' ' => Some((KEY_SPACE, false)),
        '-' => Some((KEY_MINUS, false)),
        '=' => Some((KEY_EQUAL, false)),
        '[' => Some((KEY_LEFTBRACE, false)),
        ']' => Some((KEY_RIGHTBRACE, false)),
        ';' => Some((KEY_SEMICOLON, false)),
        '\'' => Some((KEY_APOSTROPHE, false)),
        '`' => Some((KEY_GRAVE, false)),
        '\\' => Some((KEY_BACKSLASH, false)),
        ',' => Some((KEY_COMMA, false)),
        '.' => Some((KEY_DOT, false)),
        '/' => Some((KEY_SLASH, false)),
        '!' => Some((KEY_1, true)),
        '@' => Some((KEY_2, true)),
        '#' => Some((KEY_3, true)),
        '$' => Some((KEY_4, true)),
        '%' => Some((KEY_5, true)),
        '^' => Some((KEY_6, true)),
        '&' => Some((KEY_7, true)),
        '*' => Some((KEY_8, true)),
        '(' => Some((KEY_9, true)),
        ')' => Some((KEY_0, true)),
        '_' => Some((KEY_MINUS, true)),
        '+' => Some((KEY_EQUAL, true)),
        '{' => Some((KEY_LEFTBRACE, true)),
        '}' => Some((KEY_RIGHTBRACE, true)),
        ':' => Some((KEY_SEMICOLON, true)),
        '"' => Some((KEY_APOSTROPHE, true)),
        '~' => Some((KEY_GRAVE, true)),
        '|' => Some((KEY_BACKSLASH, true)),
        '<' => Some((KEY_COMMA, true)),
        '>' => Some((KEY_DOT, true)),
        '?' => Some((KEY_SLASH, true)),
        '\t' => Some((KEY_TAB, false)),
        '\n' => Some((KEY_ENTER, false)),
        _ => None,
    }
}

fn keyname_to_keycode(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        "a" => Some(KEY_A),
        "b" => Some(KEY_B),
        "c" => Some(KEY_C),
        "d" => Some(KEY_D),
        "e" => Some(KEY_E),
        "f" => Some(KEY_F),
        "g" => Some(KEY_G),
        "h" => Some(KEY_H),
        "i" => Some(KEY_I),
        "j" => Some(KEY_J),
        "k" => Some(KEY_K),
        "l" => Some(KEY_L),
        "m" => Some(KEY_M),
        "n" => Some(KEY_N),
        "o" => Some(KEY_O),
        "p" => Some(KEY_P),
        "q" => Some(KEY_Q),
        "r" => Some(KEY_R),
        "s" => Some(KEY_S),
        "t" => Some(KEY_T),
        "u" => Some(KEY_U),
        "v" => Some(KEY_V),
        "w" => Some(KEY_W),
        "x" => Some(KEY_X),
        "y" => Some(KEY_Y),
        "z" => Some(KEY_Z),
        "1" => Some(KEY_1),
        "2" => Some(KEY_2),
        "3" => Some(KEY_3),
        "4" => Some(KEY_4),
        "5" => Some(KEY_5),
        "6" => Some(KEY_6),
        "7" => Some(KEY_7),
        "8" => Some(KEY_8),
        "9" => Some(KEY_9),
        "0" => Some(KEY_0),
        "enter" | "return" => Some(KEY_ENTER),
        "esc" | "escape" => Some(KEY_ESC),
        "backspace" => Some(KEY_BACKSPACE),
        "tab" => Some(KEY_TAB),
        "space" => Some(KEY_SPACE),
        "delete" | "del" => Some(KEY_DELETE),
        "up" => Some(KEY_UP),
        "down" => Some(KEY_DOWN),
        "left" => Some(KEY_LEFT),
        "right" => Some(KEY_RIGHT),
        "f1" => Some(KEY_F1),
        "f2" => Some(KEY_F2),
        "f3" => Some(KEY_F3),
        "f4" => Some(KEY_F4),
        "f5" => Some(KEY_F5),
        "f6" => Some(KEY_F6),
        "f7" => Some(KEY_F7),
        "f8" => Some(KEY_F8),
        "f9" => Some(KEY_F9),
        "f10" => Some(KEY_F10),
        "f11" => Some(KEY_F11),
        "f12" => Some(KEY_F12),
        "f13" => Some(KEY_F13),
        "f14" => Some(KEY_F14),
        "f15" => Some(KEY_F15),
        "-" | "minus" => Some(KEY_MINUS),
        "=" | "equal" => Some(KEY_EQUAL),
        "[" | "leftbrace" => Some(KEY_LEFTBRACE),
        "]" | "rightbrace" => Some(KEY_RIGHTBRACE),
        ";" | "semicolon" => Some(KEY_SEMICOLON),
        "'" | "apostrophe" => Some(KEY_APOSTROPHE),
        "`" | "grave" => Some(KEY_GRAVE),
        "\\" | "backslash" => Some(KEY_BACKSLASH),
        "," | "comma" => Some(KEY_COMMA),
        "." | "dot" => Some(KEY_DOT),
        "/" | "slash" => Some(KEY_SLASH),
        _ => None,
    }
}

fn modifier_to_keycode(name: &str) -> Option<u16> {
    match name.to_lowercase().as_str() {
        "ctrl" | "control" => Some(KEY_LEFTCTRL),
        "alt" | "meta" => Some(KEY_LEFTALT),
        "shift" => Some(KEY_LEFTSHIFT),
        "super" | "win" | "logo" => Some(KEY_LEFTMETA),
        _ => None,
    }
}

struct UinputDevice {
    fd: libc::c_int,
}

impl UinputDevice {
    fn create() -> Result<Self, String> {
        unsafe {
            let fd = libc::open(
                b"/dev/uinput\0".as_ptr() as *const libc::c_char,
                libc::O_RDWR | libc::O_NONBLOCK,
            );
            if fd < 0 {
                return Err(format!("Failed to open /dev/uinput: {}", std::io::Error::last_os_error()));
            }

            // Enable EV_KEY
            if libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_uint) < 0 {
                libc::close(fd);
                return Err("Failed to set EV_KEY".into());
            }

            // Enable all key codes we need
            let all_keys = [
                KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H,
                KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P,
                KEY_Q, KEY_R, KEY_S, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X,
                KEY_Y, KEY_Z, KEY_0, KEY_1, KEY_2, KEY_3, KEY_4, KEY_5,
                KEY_6, KEY_7, KEY_8, KEY_9, KEY_ENTER, KEY_ESC, KEY_BACKSPACE,
                KEY_TAB, KEY_SPACE, KEY_DELETE, KEY_UP, KEY_DOWN, KEY_LEFT,
                KEY_RIGHT, KEY_LEFTCTRL, KEY_LEFTALT, KEY_LEFTSHIFT, KEY_LEFTMETA,
                KEY_F1, KEY_F2, KEY_F3, KEY_F4, KEY_F5, KEY_F6, KEY_F7,
                KEY_F8, KEY_F9, KEY_F10, KEY_F11, KEY_F12, KEY_F13, KEY_F14,
                KEY_F15, KEY_MINUS, KEY_EQUAL, KEY_LEFTBRACE, KEY_RIGHTBRACE,
                KEY_SEMICOLON, KEY_APOSTROPHE, KEY_GRAVE, KEY_BACKSLASH,
                KEY_COMMA, KEY_DOT, KEY_SLASH,
            ];

            for key in all_keys {
                if libc::ioctl(fd, UI_SET_KEYBIT, key as libc::c_ulong) < 0 {
                    libc::close(fd);
                    return Err(format!("Failed to set key bit for key {key}"));
                }
            }

            // Create device
            let mut setup: UinputSetup = std::mem::zeroed();
            let name = UINPUT_NAME.as_bytes();
            for (i, &b) in name.iter().enumerate().take(UINPUT_MAX_NAME_SIZE) {
                setup.name[i] = b;
            }
            setup.id.bustype = 0x03; // BUS_USB
            setup.id.vendor = 0x1234;
            setup.id.product = 0x5678;
            setup.id.version = 1;

            // Configure the device parameters (name + id). Required before
            // UI_DEV_CREATE on kernel >= 4.5, otherwise UI_DEV_CREATE -> EINVAL.
            if libc::ioctl(fd, UI_DEV_SETUP, &setup) < 0 {
                let err = std::io::Error::last_os_error();
                libc::close(fd);
                return Err(format!("Failed to setup uinput device: {err}"));
            }

            // Actually create the device. UI_DEV_CREATE takes no argument.
            if libc::ioctl(fd, UI_DEV_CREATE) < 0 {
                let err = std::io::Error::last_os_error();
                libc::close(fd);
                return Err(format!("Failed to create uinput device: {err}"));
            }

            // Small delay to let the device register
            std::thread::sleep(std::time::Duration::from_millis(100));

            Ok(Self { fd })
        }
    }

    fn emit(&self, type_: u16, code: u16, value: i32) {
        unsafe {
            let event = InputEvent {
                time: Timeval {
                    tv_sec: 0,
                    tv_usec: 0,
                },
                type_,
                code,
                value,
            };
            libc::write(
                self.fd,
                &event as *const InputEvent as *const libc::c_void,
                std::mem::size_of::<InputEvent>(),
            );
        }
    }

    fn syn(&self) {
        self.emit(EV_SYN, SYN_REPORT, 0);
    }

    fn press_key(&self, keycode: u16) {
        self.emit(EV_KEY, keycode, 1); // press
        self.syn();
    }

    fn release_key(&self, keycode: u16) {
        self.emit(EV_KEY, keycode, 0); // release
        self.syn();
    }

    fn type_char(&self, c: char) -> Result<(), String> {
        if let Some((keycode, needs_shift)) = char_to_keycode(c) {
            if needs_shift {
                self.press_key(KEY_LEFTSHIFT);
            }
            self.press_key(keycode);
            self.release_key(keycode);
            if needs_shift {
                self.release_key(KEY_LEFTSHIFT);
            }
            Ok(())
        } else {
            Err(format!(
                "Character '{c}' is not supported by the virtual keyboard layout. Try using the 'paste' command instead."
            ))
        }
    }

    fn type_text(&self, text: &str) -> Result<(), String> {
        for c in text.chars() {
            self.type_char(c)?;
            // Small delay between keystrokes for reliability
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(())
    }

    fn press_key_combo(&self, combo: &str) {
        // Parse "ctrl+s" style combos
        let parts: Vec<&str> = combo.split('+').collect();
        let mut modifiers: Vec<u16> = Vec::new();
        let mut main_key: Option<u16> = None;

        for part in &parts {
            let part = part.trim();
            if let Some(modifier) = modifier_to_keycode(part) {
                modifiers.push(modifier);
            } else if let Some(keycode) = keyname_to_keycode(part) {
                main_key = Some(keycode);
            }
        }

        // Press modifiers
        for &m in &modifiers {
            self.press_key(m);
        }

        if !modifiers.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(12));
        }

        // Press and release main key
        if let Some(key) = main_key {
            self.press_key(key);
            std::thread::sleep(std::time::Duration::from_millis(12));
            self.release_key(key);
        }

        if !modifiers.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(12));
        }

        // Release modifiers in reverse order
        for &m in modifiers.iter().rev() {
            self.release_key(m);
        }
    }
}

impl Drop for UinputDevice {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

fn handle_client(mut stream: UnixStream, device: Arc<Mutex<UinputDevice>>) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let msg = String::from_utf8_lossy(&buffer[..n]);
                    let msg = msg.trim();

                    let response = {
                        let dev = device.lock().unwrap();
                        match msg {
                            "PING" => "OK".to_string(),
                            text if text.starts_with("TYPE:") => {
                                let text = &text[5..];
                                match dev.type_text(text) {
                                    Ok(()) => "OK".to_string(),
                                    Err(e) => format!("ERR:{e}"),
                                }
                            }
                            key if key.starts_with("KEY:") => {
                                let key = &key[4..];
                                dev.press_key_combo(key);
                                "OK".to_string()
                            }
                            _ => format!("ERR:unknown command:{msg}"),
                        }
                    };

                    let _ = stream.write_all(format!("{response}\n").as_bytes());
                }
                Err(_) => break,
            }
        }
    });
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Clean up old socket
    let _ = fs::remove_file(SOCKET_PATH);

    // Parse --foreground flag
    let _foreground = args.iter().any(|a| a == "--foreground" || a == "-f");

    eprintln!("mk-daemon: starting...");

    // Check if running as root
    unsafe {
        if libc::getuid() != 0 {
            eprintln!("mk-daemon: WARNING - not running as root, uinput may fail");
        }
    }

    // Create uinput device
    let device = match UinputDevice::create() {
        Ok(d) => {
            eprintln!("mk-daemon: virtual keyboard created: {}", UINPUT_NAME);
            Arc::new(Mutex::new(d))
        }
        Err(e) => {
            eprintln!("mk-daemon: failed to create uinput device: {e}");
            eprintln!("mk-daemon: make sure /dev/uinput is accessible");
            std::process::exit(1);
        }
    };

    // Create socket
    let listener = match UnixListener::bind(SOCKET_PATH) {
        Ok(l) => {
            eprintln!("mk-daemon: listening on {SOCKET_PATH}");
            // Set permissions so only the owner can connect
            let _ = fs::set_permissions(
                SOCKET_PATH,
                std::os::unix::fs::PermissionsExt::from_mode(0o600),
            );
            
            // If run under sudo, chown the socket to the original user
            if let Ok(sudo_uid_str) = std::env::var("SUDO_UID") {
                if let Ok(uid) = sudo_uid_str.parse::<u32>() {
                    let c_path = std::ffi::CString::new(SOCKET_PATH).unwrap();
                    unsafe {
                        libc::chown(c_path.as_ptr(), uid, u32::MAX);
                    }
                }
            }
            l
        }
        Err(e) => {
            eprintln!("mk-daemon: failed to bind socket: {e}");
            std::process::exit(1);
        }
    };

    // Set socket to non-blocking for clean shutdown
    listener.set_nonblocking(true).ok();

    eprintln!("mk-daemon: ready. Press Ctrl+C to stop.");

    // Handle Ctrl+C
    static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    extern "C" fn signal_handler(_: libc::c_int) {
        STOPPING.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    unsafe {
        libc::signal(libc::SIGINT, signal_handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, signal_handler as *const () as libc::sighandler_t);
    }

    loop {
        if STOPPING.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).ok();
                handle_client(stream, device.clone());
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
            Err(_) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
        }
    }

    eprintln!("mk-daemon: shutting down...");
    let _ = fs::remove_file(SOCKET_PATH);
    eprintln!("mk-daemon: stopped.");
}
