use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseReason {
    DisplaysAsleep,
    ScreenLocked,
}

impl PauseReason {
    pub fn label(self) -> &'static str {
        match self {
            PauseReason::DisplaysAsleep => "Display asleep — paused",
            PauseReason::ScreenLocked => "Screen locked — paused",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            PauseReason::DisplaysAsleep => "Display off",
            PauseReason::ScreenLocked => "Locked",
        }
    }
}

/// Whether collection should pause (no active, no idle).
/// Clamshell OK: lid closed with an awake external display still tracks.
/// Pauses when every display is asleep, or the login screen is locked.
pub fn collection_pause_reason() -> Option<PauseReason> {
    if is_screen_locked() {
        return Some(PauseReason::ScreenLocked);
    }
    if all_displays_asleep() {
        return Some(PauseReason::DisplaysAsleep);
    }
    // Lid closed and no usable awake display (rare race) → treat as asleep.
    if is_lid_closed() && !has_awake_display() {
        return Some(PauseReason::DisplaysAsleep);
    }
    None
}

/// True when the MacBook lid is closed (`AppleClamshellState` via IOKit).
/// Desktops / unknown hardware return `false`.
pub fn is_lid_closed() -> bool {
    #[cfg(target_os = "macos")]
    {
        iokit_clamshell_closed().unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn iokit_clamshell_closed() -> Option<bool> {
    #![allow(non_camel_case_types, dead_code)]
    use std::ffi::{c_char, c_void};

    #[repr(C)]
    struct __CFBoolean(c_void);
    type CFBooleanRef = *const __CFBoolean;
    type CFStringRef = *const c_void;
    type CFAllocatorRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFMutableDictionaryRef = *mut c_void;
    type io_object_t = u32;
    type io_service_t = io_object_t;
    type mach_port_t = u32;

    #[link(name = "IOKit", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kIOMainPortDefault: mach_port_t;
        fn IOServiceMatching(name: *const c_char) -> CFMutableDictionaryRef;
        fn IOServiceGetMatchingService(
            main_port: mach_port_t,
            matching: CFDictionaryRef,
        ) -> io_service_t;
        fn IORegistryEntryCreateCFProperty(
            entry: io_service_t,
            key: CFStringRef,
            allocator: CFAllocatorRef,
            options: u32,
        ) -> CFTypeRef;
        fn IOObjectRelease(object: io_object_t) -> i32;
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(boolean: CFBooleanRef) -> u8;
        fn CFRelease(cf: CFTypeRef);
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    unsafe {
        let matching = IOServiceMatching(c"IOPMrootDomain".as_ptr());
        if matching.is_null() {
            return None;
        }
        let service = IOServiceGetMatchingService(kIOMainPortDefault, matching as CFDictionaryRef);
        if service == 0 {
            return None;
        }
        let key = CFStringCreateWithCString(
            std::ptr::null(),
            c"AppleClamshellState".as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        );
        if key.is_null() {
            IOObjectRelease(service);
            return None;
        }
        let prop =
            IORegistryEntryCreateCFProperty(service, key, std::ptr::null(), 0);
        CFRelease(key);
        IOObjectRelease(service);
        if prop.is_null() {
            return Some(false);
        }
        let closed = if CFGetTypeID(prop) == CFBooleanGetTypeID() {
            CFBooleanGetValue(prop as CFBooleanRef) != 0
        } else {
            false
        };
        CFRelease(prop);
        Some(closed)
    }
}

fn has_awake_display() -> bool {
    #[cfg(target_os = "macos")]
    {
        awake_display_count() > 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn all_displays_asleep() -> bool {
    #[cfg(target_os = "macos")]
    {
        let (total, awake) = display_counts();
        total > 0 && awake == 0
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

fn awake_display_count() -> u32 {
    display_counts().1
}

#[cfg(target_os = "macos")]
fn display_counts() -> (u32, u32) {
    extern "C" {
        fn CGGetActiveDisplayList(max_displays: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayIsAsleep(display: u32) -> bool;
    }
    unsafe {
        let mut count: u32 = 0;
        if CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) != 0 || count == 0 {
            return (0, 0);
        }
        let mut ids = vec![0u32; count as usize];
        let mut got: u32 = 0;
        if CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut got) != 0 {
            return (0, 0);
        }
        let awake = ids
            .iter()
            .take(got as usize)
            .filter(|&&id| !CGDisplayIsAsleep(id))
            .count() as u32;
        (got, awake)
    }
}

#[cfg(not(target_os = "macos"))]
fn display_counts() -> (u32, u32) {
    (1, 1)
}

pub fn is_screen_locked() -> bool {
    #[cfg(target_os = "macos")]
    {
        screen_locked_cg().unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn screen_locked_cg() -> Option<bool> {
    use std::ffi::c_void;

    type CFDictionaryRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFTypeRef = *const c_void;
    type CFBooleanRef = *const c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
        fn CFDictionaryGetValue(the_dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const std::ffi::c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFBooleanGetTypeID() -> usize;
        fn CFBooleanGetValue(boolean: CFBooleanRef) -> u8;
        fn CFRelease(cf: CFTypeRef);
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    unsafe {
        let dict = CGSessionCopyCurrentDictionary();
        if dict.is_null() {
            return None;
        }
        let key = CFStringCreateWithCString(
            std::ptr::null(),
            c"CGSSessionScreenIsLocked".as_ptr(),
            K_CF_STRING_ENCODING_UTF8,
        );
        if key.is_null() {
            CFRelease(dict as CFTypeRef);
            return None;
        }
        let val = CFDictionaryGetValue(dict, key as *const c_void);
        CFRelease(key);
        let locked = if !val.is_null() && CFGetTypeID(val) == CFBooleanGetTypeID() {
            CFBooleanGetValue(val as CFBooleanRef) != 0
        } else {
            false
        };
        CFRelease(dict as CFTypeRef);
        Some(locked)
    }
}

/// Seconds since last keyboard/mouse input (macOS CoreGraphics).
pub fn system_idle_seconds() -> f64 {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn CGEventSourceSecondsSinceLastEventType(state_id: u32, event_type: u32) -> f64;
        }
        const COMBINED: u32 = 0;
        const ANY_INPUT: u32 = 0xFFFF_FFFF;
        unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED, ANY_INPUT) }
    }
    #[cfg(not(target_os = "macos"))]
    {
        0.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct FrontApp {
    pub app_name: String,
    pub window_title: String,
    pub page_url: String,
}

fn is_browser(app_name: &str) -> bool {
    let n = app_name.to_lowercase();
    n.contains("chrome") || n.contains("brave") || n.contains("safari") || n.contains("firefox")
}

/// Active tab title + URL via the browser app.
fn browser_tab_info(app_name: &str) -> Option<(String, String)> {
    let n = app_name.to_lowercase();
    let script = if n.contains("chrome") {
        r#"tell application "Google Chrome"
  if (count of windows) is 0 then return ""
  try
    set t to title of active tab of front window
    set u to URL of active tab of front window
    return t & character id 31 & u
  end try
  return ""
end tell"#
    } else if n.contains("brave") {
        r#"tell application "Brave Browser"
  if (count of windows) is 0 then return ""
  try
    set t to title of active tab of front window
    set u to URL of active tab of front window
    return t & character id 31 & u
  end try
  return ""
end tell"#
    } else if n.contains("safari") {
        r#"tell application "Safari"
  if (count of windows) is 0 then return ""
  try
    set t to name of current tab of front window
    set u to URL of current tab of front window
    return t & character id 31 & u
  end try
  return ""
end tell"#
    } else {
        return None;
    };
    let raw = osascript(script)?.trim().to_string();
    if raw.is_empty() || raw == "missing value" {
        return None;
    }
    let mut parts = raw.splitn(2, '\u{001f}');
    let title = parts.next().unwrap_or("").trim().to_string();
    let url = parts.next().unwrap_or("").trim().to_string();
    Some((title, url))
}

/// Frontmost app + browser tab title/URL when applicable.
pub fn frontmost_app() -> FrontApp {
    #[cfg(target_os = "macos")]
    {
        let raw = osascript(
            r#"tell application "System Events"
  set p to first application process whose frontmost is true
  set appName to name of p
  set winTitle to ""
  if appName contains "Chrome" or appName contains "Brave" or appName contains "Safari" or appName contains "Firefox" then
    try
      set winTitle to name of front window of p
    end try
  end if
  return appName & character id 31 & winTitle
end tell"#,
        )
        .unwrap_or_default();

        let mut parts = raw.splitn(2, '\u{001f}');
        let app_name = parts.next().unwrap_or("").trim().to_string();
        let mut window_title = parts.next().unwrap_or("").trim().to_string();
        let mut page_url = String::new();

        if window_title.is_empty() && !raw.contains('\u{001f}') && !is_browser(&app_name) {
            return FrontApp {
                app_name: raw.trim().to_string(),
                window_title: String::new(),
                page_url: String::new(),
            };
        }

        if is_browser(&app_name) {
            if let Some((tab, url)) = browser_tab_info(&app_name) {
                if !tab.is_empty() {
                    window_title = tab;
                }
                page_url = url;
            }
        }

        FrontApp {
            app_name,
            window_title,
            page_url,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        FrontApp::default()
    }
}

fn osascript(script: &str) -> Option<String> {
    let output = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() || s == "missing value" {
        None
    } else {
        Some(s)
    }
}

pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Settings.PrivacySecurity.extension?Privacy_Accessibility")
            .spawn();
    }
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Non-blocking Notification Center banner + sound.
pub fn notify_task_reminder(summary: &str, when_label: &str) {
    #[cfg(target_os = "macos")]
    {
        let title = escape_applescript(summary);
        let subtitle = escape_applescript(when_label);
        let script = format!(
            r#"display notification "{subtitle}" with title "Task reminder" subtitle "{title}" sound name "Glass""#
        );
        let _ = Command::new("osascript").arg("-e").arg(script).spawn();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (summary, when_label);
    }
}

/// Modal popup in a background thread so the collector loop is not blocked.
pub fn alert_task_reminder(summary: &str) {
    #[cfg(target_os = "macos")]
    {
        let msg = escape_applescript(summary);
        std::thread::spawn(move || {
            let script = format!(
                r#"display alert "Task reminder" message "{msg}" as critical buttons {{"OK"}} default button "OK""#
            );
            let _ = Command::new("osascript").arg("-e").arg(script).output();
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = summary;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationStatus {
    Ok,
    Denied,
    /// Chrome not running / no window — cannot probe.
    Unknown,
}

/// Probe whether Chrome Automation (Apple Events) allows reading the active tab URL.
pub fn browser_automation_status() -> AutomationStatus {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(
                r#"tell application "System Events"
  if not (exists process "Google Chrome") then return "SKIP"
end tell
tell application "Google Chrome"
  if (count of windows) is 0 then return "SKIP"
  try
    set u to URL of active tab of front window
    if u is missing value or u is "" then return "DENIED"
    return "OK"
  on error
    return "DENIED"
  end try
end tell"#,
            )
            .output();
        match output {
            Ok(o) if o.status.success() => {
                match String::from_utf8_lossy(&o.stdout).trim() {
                    "OK" => AutomationStatus::Ok,
                    "DENIED" => AutomationStatus::Denied,
                    _ => AutomationStatus::Unknown,
                }
            }
            _ => AutomationStatus::Denied,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        AutomationStatus::Ok
    }
}

pub fn open_automation_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Automation")
            .spawn();
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Settings.PrivacySecurity.extension?Privacy_Automation")
            .spawn();
    }
}

pub fn accessibility_likely_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        !frontmost_app().app_name.is_empty()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn is_our_app(app_name: &str) -> bool {
    let n = app_name.to_lowercase();
    n.contains("productivity tracker") || n.contains("productivity-tracker")
}
