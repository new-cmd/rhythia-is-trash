#[cfg(target_os = "linux")]
pub fn raise_main_thread() {
    // SAFETY: gettid takes no arguments and cannot fail.
    let tid = unsafe { libc::syscall(libc::SYS_gettid) } as u64;
    std::thread::spawn(move || {
        if let Err(e) = request_nice(tid, -15) {
            eprintln!("thread priority not raised: {e}");
        }
    });
}

#[cfg(target_os = "linux")]
fn request_nice(tid: u64, nice: i32) -> Result<(), String> {
    use dbus::{BusType, Connection, Message};

    let connection = Connection::get_private(BusType::System).map_err(|e| e.to_string())?;
    let mut call = Message::new_method_call(
        "org.freedesktop.RealtimeKit1",
        "/org/freedesktop/RealtimeKit1",
        "org.freedesktop.RealtimeKit1",
        "MakeThreadHighPriority",
    )?;
    call.append_items(&[tid.into(), nice.into()]);
    connection
        .send_with_reply_and_block(call, 1000)
        .map(drop)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn raise_main_thread() {}
