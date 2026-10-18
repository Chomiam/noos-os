// Prevents an additional console window on Windows in release. DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    disable_webkit_dmabuf();

    noos_installer_lib::run()
}

/// Turns off WebKitGTK's DMABUF renderer on Linux.
///
/// On virtual GPUs and on NVIDIA it regularly fails to build a framebuffer,
/// and the failure mode is a blank window with nothing in the log rather than
/// an error — which on a live ISO looks exactly like a broken installer.
/// Losing a little compositing performance is a good trade for not shipping
/// that.
///
/// This has to run before the webview is created, which is why it is here
/// rather than in the Tauri builder.
#[cfg(target_os = "linux")]
fn disable_webkit_dmabuf() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: called first thing in main, before any thread or GTK/webview
        // initialisation exists, so there is nothing that could observe the
        // environment changing underneath it.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

#[cfg(not(target_os = "linux"))]
fn disable_webkit_dmabuf() {}
