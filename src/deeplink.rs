//! Jam deep links: `ytm-rs://jam/CODE` and `https://grapedjuice.github.io/ytm-rs/jam/?c=CODE`.
//!
//! macOS doesn't put the URL in argv: LaunchServices starts the app if needed (one copy
//! per bundle) and sends it a kAEGetURL Apple Event. `install` registers a handler for
//! that before the event loop starts; the handler queues the code here and wakes the UI,
//! which drains the queue with `take()` (the same pattern as `tray.rs`). A code that
//! arrives before the UI is up just waits in the queue.

use crate::sync::MutexExt;
use std::sync::Mutex;

use crate::jam::normalize_code as normalize;

static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// A plain later launch asked for the window (see `forward_show`).
static SHOW: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// What a plain later launch sends instead of a code.
const SHOW_MSG: &str = "show";
/// The updater's relaunch passes this: the old copy may still be exiting, so don't hand over to it.
pub const RELAUNCH_FLAG: &str = "--relaunch";

/// The room code in a launch argument, if it is a jam link:
/// `ytm-rs://jam/CODE[/]` (any case) or the https share link with `?c=CODE`.
pub fn parse(arg: &str) -> Option<String> {
    let arg = arg.trim().trim_matches('"');
    let (scheme, rest) = arg.split_once("://")?;
    let rest = rest.split('#').next().unwrap_or("");
    if ["dawnfm", "ytm-rs"].iter().any(|s| scheme.eq_ignore_ascii_case(s)) {
        // `jam/CODE`, `jam/CODE/`, or `jam/?c=CODE`.
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let path = path.trim_end_matches('/');
        let (host, code) = path.split_once('/').unwrap_or((path, ""));
        if !host.eq_ignore_ascii_case("jam") {
            return None;
        }
        if !code.is_empty() {
            return normalize(code);
        }
        return query_param(query, "c").and_then(normalize);
    }
    if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") {
        let (path, query) = rest.split_once('?')?;
        let lower = path.to_ascii_lowercase();
        let path = lower.trim_end_matches('/').trim_end_matches("/index.html");
        if path != "grapedjuice.github.io/ytm-rs/jam" {
            return None;
        }
        return query_param(query, "c").and_then(normalize);
    }
    None
}

fn query_param<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == name).then_some(v)
    })
}

/// The first jam link among the process arguments.
pub fn from_args() -> Option<String> {
    std::env::args_os().skip(1).find_map(|a| parse(&a.to_string_lossy()))
}

/// Queue a code for the UI (e.g. from the launch arguments).
pub fn push(code: String) {
    PENDING.lock_unpoisoned().push(code);
}

/// Codes received since the last call, oldest first.
pub fn take() -> Vec<String> {
    std::mem::take(&mut *PENDING.lock_unpoisoned())
}

/// Whether a later launch asked for the window since the last call.
pub fn take_show() -> bool {
    SHOW.swap(false, std::sync::atomic::Ordering::Relaxed)
}

/// A plain launch while another copy runs: bring that one's window up instead, so only
/// one copy uses (and refreshes) the Google session. Two copies refreshing it apart
/// leave one with old cookies, which Google treats as stolen and signs out. True if
/// a running copy took it.
pub fn forward_show() -> bool {
    if std::env::args_os().any(|a| a == RELAUNCH_FLAG) {
        return false;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    forward_to_running(SHOW_MSG)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let mut name = format!(r"\\.\pipe\dawnfm-party-{user}");
    if let Some(dir) = std::env::var_os("YTM_DATA_DIR") {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        dir.hash(&mut h);
        name.push_str(&format!("-{:016x}", h.finish()));
    }
    name
}

#[cfg(windows)]
pub use win::{forward_to_running, listen, register_protocol};

#[cfg(not(windows))]
pub fn forward_to_running(_code: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn register_protocol() {}

type Waker = Box<dyn Fn() + Send>;
static WAKE: Mutex<Option<Waker>> = Mutex::new(None);

#[cfg(not(windows))]
/// Wake the UI with `wake` whenever a link arrives. Codes queued before this (a cold
/// launch from a link) get one wake now.
pub fn listen(wake: impl Fn() + Send + 'static) {
    let pending = !PENDING.lock_unpoisoned().is_empty();
    if pending {
        wake();
    }
    *WAKE.lock_unpoisoned() = Some(Box::new(wake));
}

/// A URL from the system (the Apple Event): queue its code and wake the UI.
fn receive(url: &str) {
    let Some(code) = parse(url) else {
        log::warn!("deeplink: ignoring {url:?}");
        return;
    };
    log::info!("deeplink: jam {code}");
    push(code);
    if let Some(wake) = WAKE.lock_unpoisoned().as_ref() {
        wake();
    }
}

/// Start receiving links. Call on the main thread before the event loop runs,
/// so the launch event isn't missed.
pub fn install() {
    #[cfg(target_os = "macos")]
    mac::install();
}

#[cfg(windows)]
mod win {
    use std::io::Write;

    use crate::sync::MutexExt;

    use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE};
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND, ReadFile};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_WAIT,
    };
    use windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    use windows::core::HSTRING;

    use super::{pipe_name, push};

    /// Hand `msg` (a link, or `show`) to a running instance. True if one took it.
    pub fn forward_to_running(msg: &str) -> bool {
        let name = pipe_name();
        // The server has one pipe instance; between clients it is briefly busy.
        for _ in 0..20 {
            match std::fs::OpenOptions::new().write(true).open(&name) {
                Ok(mut pipe) => {
                    return pipe.write_all(format!("{msg}\n").as_bytes()).and_then(|_| pipe.flush()).is_ok();
                }
                // ERROR_PIPE_BUSY: another client is mid-handoff.
                Err(e) if e.raw_os_error() == Some(231) => std::thread::sleep(std::time::Duration::from_millis(50)),
                // No server (ERROR_FILE_NOT_FOUND) or anything else: nobody to hand to.
                Err(_) => return false,
            }
        }
        false
    }

    struct Pipe(HANDLE);
    // SAFETY: a pipe handle may be used from any thread.
    unsafe impl Send for Pipe {}
    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// Serve the pipe on a background thread: each link received is queued and
    /// `wake` called. Only the first instance gets the pipe; any later one (a plain
    /// second launch) does nothing here.
    pub fn listen(wake: impl Fn() + Send + 'static) {
        let pending = !super::PENDING.lock_unpoisoned().is_empty();
        if pending {
            wake();
        }
        let name = pipe_name();
        let handle = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(name.as_str()),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                512,
                0,
                None,
            )
        };
        if handle.is_invalid() {
            // ERROR_ACCESS_DENIED here means another instance already serves it.
            log::info!("party link pipe not served by this instance: {}", windows::core::Error::from_thread());
            return;
        }
        let pipe = Pipe(handle);
        let spawned = std::thread::Builder::new().name("deeplink".into()).spawn(move || serve(pipe, wake));
        if let Err(e) = spawned {
            log::warn!("party link listener: {e}");
        }
    }

    fn serve(pipe: Pipe, wake: impl Fn()) {
        loop {
            let connected = unsafe { ConnectNamedPipe(pipe.0, None) };
            if let Err(e) = connected {
                // A client that connected between create/disconnect and this call.
                if e.code() != ERROR_PIPE_CONNECTED.to_hresult() {
                    log::warn!("party link pipe: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    continue;
                }
            }
            let mut msg = Vec::new();
            let mut buf = [0u8; 64];
            loop {
                let mut n = 0u32;
                let ok = unsafe { ReadFile(pipe.0, Some(&mut buf), Some(&mut n), None) };
                if ok.is_err() || n == 0 {
                    break; // ERROR_BROKEN_PIPE: the client wrote and closed.
                }
                msg.extend_from_slice(&buf[..n as usize]);
                if msg.len() > 256 {
                    break;
                }
            }
            unsafe {
                let _ = DisconnectNamedPipe(pipe.0);
            }
            let msg = String::from_utf8_lossy(&msg).trim().to_owned();
            if msg == super::SHOW_MSG {
                log::info!("another launch asked for the window");
                super::SHOW.store(true, std::sync::atomic::Ordering::Relaxed);
                wake();
                continue;
            }
            match super::parse(&msg) {
                Some(code) => {
                    log::info!("jam code received from another launch: {code}");
                    push(code);
                    wake();
                }
                None => log::warn!("party link pipe: ignored an invalid message"),
            }
        }
    }

    /// Register the `dawnfm:` URL protocol for this user, pointing at this exe.
    /// Call only from an installed copy, so dev builds don't take the protocol over.
    pub fn register_protocol() {
        match std::env::current_exe() {
            Ok(exe) => {
                if let Err(e) = register_at(HKEY_CURRENT_USER, r"Software\Classes\dawnfm", &exe.to_string_lossy()) {
                    log::warn!("registering the dawnfm: protocol: {e}");
                }
            }
            Err(e) => log::warn!("registering the dawnfm: protocol: {e}"),
        }
    }

    pub(super) fn register_at(root: HKEY, key: &str, exe: &str) -> windows::core::Result<()> {
        let set = |sub: &str, name: &str, value: &str| -> windows::core::Result<()> {
            let path = if sub.is_empty() { key.to_string() } else { format!(r"{key}\{sub}") };
            let data: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            let name = HSTRING::from(name);
            unsafe {
                RegSetKeyValueW(
                    root,
                    &HSTRING::from(path),
                    if name.is_empty() { windows::core::PCWSTR::null() } else { windows::core::PCWSTR(name.as_ptr()) },
                    REG_SZ.0,
                    Some(data.as_ptr().cast()),
                    (data.len() * 2) as u32,
                )
            }
            .ok()
        };
        set("", "", "URL:dawnfm Protocol")?;
        set("", "URL Protocol", "")?;
        set("DefaultIcon", "", &format!("\"{exe}\",0"))?;
        set(r"shell\open\command", "", &format!("\"{exe}\" \"%1\""))
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, Sel};
    use objc2::{define_class, msg_send, sel, ClassType};
    use objc2_foundation::NSString;

    /// `kInternetEventClass` / `kAEGetURL`: both 'GURL'.
    const GURL: u32 = u32::from_be_bytes(*b"GURL");
    /// `keyDirectObject`: '----', the URL itself.
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

    define_class!(
        // SAFETY: a plain NSObject subclass with no ivars and no Drop.
        #[unsafe(super(NSObject))]
        #[name = "DawnFMLinkHandler"]
        struct Handler;

        impl Handler {
            // SAFETY: the signature NSAppleEventManager calls: two NSAppleEventDescriptors.
            #[unsafe(method(handleGetURL:withReplyEvent:))]
            fn handle_get_url(&self, event: &AnyObject, _reply: &AnyObject) {
                // SAFETY: `event` is an NSAppleEventDescriptor; both methods exist on it
                // and take/return what's declared (AEKeyword is a UInt32).
                let url: Option<Retained<NSString>> = unsafe {
                    let desc: Option<Retained<AnyObject>> = msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT];
                    match desc {
                        Some(d) => msg_send![&*d, stringValue],
                        None => None,
                    }
                };
                match url {
                    Some(url) => super::receive(&url.to_string()),
                    None => log::warn!("deeplink: URL event without a URL"),
                }
            }

            // AppKit may set up its own Apple Event handlers while launching; take GURL
            // back just before the launch events are delivered.
            #[unsafe(method(willFinishLaunching:))]
            fn will_finish_launching(&self, _note: &AnyObject) {
                set_handler(self);
            }
        }
    );

    fn set_handler(handler: &Handler) {
        // SAFETY: `sharedAppleEventManager` always returns the manager; the selector
        // matches `handle_get_url` above, and the handler lives forever (leaked in
        // `install`), as the manager doesn't retain it.
        unsafe {
            let manager: Retained<AnyObject> = msg_send![objc2::class!(NSAppleEventManager), sharedAppleEventManager];
            let sel: Sel = sel!(handleGetURL:withReplyEvent:);
            let _: () = msg_send![&*manager, setEventHandler: handler, andSelector: sel, forEventClass: GURL, andEventID: GURL];
        }
    }

    pub fn install() {
        // SAFETY: `new` on an NSObject subclass returns a retained instance.
        let handler: Retained<Handler> = unsafe { msg_send![Handler::class(), new] };
        set_handler(&handler);
        // SAFETY: the selector matches `will_finish_launching`; the observer is never
        // freed, and a nil object means the notification from any sender (NSApp).
        unsafe {
            let center: Retained<AnyObject> = msg_send![objc2::class!(NSNotificationCenter), defaultCenter];
            let none: *const AnyObject = std::ptr::null();
            // AppKit installs its own GURL handler during launch, after "will finish":
            // take it back once launching has finished too.
            for note in ["NSApplicationWillFinishLaunchingNotification", "NSApplicationDidFinishLaunchingNotification"] {
                let name = NSString::from_str(note);
                let _: () = msg_send![&*center, addObserver: &*handler, selector: sel!(willFinishLaunching:), name: &*name, object: none];
            }
        }
        // Both registrations hold a plain pointer: keep the handler for the whole run.
        std::mem::forget(handler);
        log::info!("deeplink: listening for dawnfm: and ytm-rs: links");
    }

    /// Hand `url` to a handler as macOS would: a GURL event with the URL as its direct object.
    #[cfg(test)]
    pub fn simulate(url: &str) {
        // SAFETY: the documented NSAppleEventDescriptor constructors and setter, with the
        // C types they take (FourCharCodes are UInt32, AEReturnID SInt16, AETransactionID SInt32).
        unsafe {
            let handler: Retained<Handler> = msg_send![Handler::class(), new];
            let class = objc2::class!(NSAppleEventDescriptor);
            let none: *const AnyObject = std::ptr::null();
            let event: Retained<AnyObject> = msg_send![class, appleEventWithEventClass: GURL, eventID: GURL, targetDescriptor: none, returnID: -1i16, transactionID: 0i32];
            let url: Retained<AnyObject> = msg_send![class, descriptorWithString: &*NSString::from_str(url)];
            let _: () = msg_send![&*event, setParamDescriptor: &*url, forKeyword: DIRECT_OBJECT];
            let _: () = msg_send![&*handler, handleGetURL: &*event, withReplyEvent: &*event];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_protocol() {
        assert_eq!(parse("ytm-rs://jam/ABC234").as_deref(), Some("ABC234"));
        assert_eq!(parse("ytm-rs://jam/abc234/").as_deref(), Some("ABC234"));
        assert_eq!(parse("YTM-RS://JAM/aBc234").as_deref(), Some("ABC234"));
        assert_eq!(parse("\"ytm-rs://jam/ABC234\"").as_deref(), Some("ABC234"));
        assert_eq!(parse("ytm-rs://jam/?c=xyz789").as_deref(), Some("XYZ789"));
    }

    #[test]
    fn share_link() {
        assert_eq!(parse("https://grapedjuice.github.io/ytm-rs/jam/?c=ABC234").as_deref(), Some("ABC234"));
        assert_eq!(parse("https://grapedjuice.github.io/ytm-rs/jam?c=abc234").as_deref(), Some("ABC234"));
        assert_eq!(parse("https://grapedjuice.github.io/ytm-rs/jam/index.html?x=1&c=abc234#top").as_deref(), Some("ABC234"));
    }

    #[test]
    fn rejects() {
        for bad in [
            "",
            "%1",
            "ABC234",
            "ytm-rs://jam/",
            "ytm-rs://jam/ABC23",      // too short
            "ytm-rs://jam/ABC2345",    // too long
            "ytm-rs://jam/ABC0I1",     // 0, I, 1 are not in the alphabet
            "ytm-rs://jam/ABC234/xyz", // extra path
            "ytm-rs://room/ABC234",
            "https://example.com/ytm-rs/jam/?c=ABC234",
            "https://grapedjuice.github.io/ytm-rs/?c=ABC234",
            "https://grapedjuice.github.io/ytm-rs/jam/",
            "--login",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn apple_event_url() {
        // What macOS hands over: the bare URL, no quotes.
        assert_eq!(parse("ytm-rs://jam/ABC234?utm=x").as_deref(), Some("ABC234"));
        assert_eq!(parse(" ytm-rs://jam/abc234#x ").as_deref(), Some("ABC234"));
    }

    #[test]
    fn queue() {
        push("ABC234".into());
        push("XYZ789".into());
        assert_eq!(take(), ["ABC234", "XYZ789"]);
        assert!(take().is_empty());

        // The Apple Event path: a link is queued and wakes the UI; anything else is dropped.
        // (Here rather than its own test: the queue is shared.)
        use std::sync::atomic::{AtomicUsize, Ordering};
        static WOKEN: AtomicUsize = AtomicUsize::new(0);
        listen(|| {
            WOKEN.fetch_add(1, Ordering::Relaxed);
        });
        #[cfg(target_os = "macos")]
        {
            mac::simulate("ytm-rs://jam/abc234");
            mac::simulate("https://example.com/");
            assert_eq!(take(), ["ABC234"]);
            assert_eq!(WOKEN.load(Ordering::Relaxed), 1);
        }
    }
}
