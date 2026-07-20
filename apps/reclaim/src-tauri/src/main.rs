// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Allocator hardening: preload GrapheneOS hardened_malloc for this process and
    // every child it spawns. MUST run first — it may re-exec the process before any
    // allocation-heavy work. No-op if the library isn't built or is disabled
    // (RECLAIM_HARDENED_MALLOC=0). See security::allocator. [HARDENING]
    reclaim_lib::preload_hardened_malloc();

    // Initialize logging
    env_logger::init();

    // Set environment for Linux GPU/WebKit compatibility
    #[cfg(target_os = "linux")]
    {
        use std::env;

        // Force the GTK/X11 backend. The media stack (GStreamer video surfaces) and
        // the floating media controls embed into the app window via X11 window
        // reparenting, which only works when the Tauri/GTK windows are real X11
        // surfaces. On a Wayland session they're Wayland surfaces with no X11 parent,
        // so dropped videos pop out as separate top-level windows and the floating
        // controls never appear. Must be set before GTK initializes.
        //
        // ALWAYS force it: Nobara/Fedora 44 KDE exports GDK_BACKEND=wayland
        // SESSION-WIDE, so the old "respect an existing GDK_BACKEND" made the
        // whole app silently go Wayland-native after the distro upgrade — the
        // main window had no XID, every create_video_surface failed with
        // "unsupported window handle type: Wayland", and playback showed a
        // black pane. A session-wide default is not a choice about THIS app;
        // the only override honored is the app-specific EARTH_GDK_BACKEND.
        match env::var("EARTH_GDK_BACKEND") {
            Ok(v) if !v.is_empty() => env::set_var("GDK_BACKEND", v),
            _ => env::set_var("GDK_BACKEND", "x11"),
        }

        // Same story for GStreamer's GL stack (the Enhance bin): on a Wayland
        // session it auto-picks the Wayland GL platform (WAYLAND_DISPLAY is
        // set), which renders PURE BLACK frames into our X11 pipeline without
        // a single error — every video looked black with working audio.
        // Explicitly user-set values are respected (these are not distro-set).
        if env::var("GST_GL_PLATFORM").is_err() {
            env::set_var("GST_GL_PLATFORM", "glx");
        }
        if env::var("GST_GL_WINDOW").is_err() {
            env::set_var("GST_GL_WINDOW", "x11");
        }

        // Workaround for WebKitGTK GBM buffer allocation issues on some GPUs:
        // disabling the DMA-BUF renderer forces a software-ish path. But that
        // path is exactly what blocks GPU-accelerated media. So when the GPU-accel
        // spike is enabled (EARTH_GL), DON'T disable it — let WebKit use the
        // accelerated renderer end-to-end.
        let gl_enabled = env::var("EARTH_GL")
            .map(|v| v == "1" || v == "ondemand" || v == "always")
            .unwrap_or(false);
        if !gl_enabled && env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
            env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }

        // NOTE: We intentionally do NOT force `GST_GL_API=opengl3`. The embedded
        // browser surface composites in SOFTWARE (HardwareAccelerationPolicy::Never),
        // and forcing GStreamer onto GL there produces "GstIntRange" caps failures
        // that break <audio>/<video> playback (mp3/mp4 requests fail) and stall the
        // web process. Letting GStreamer auto-select keeps WebKit media working;
        // the app's own GStreamer media player auto-selects fine without it too.
        // (Re-add `env::set_var("GST_GL_API", "opengl3");` here if the app's media
        // player regresses.)

        // For NVIDIA GPUs - prefer NVDEC/NVENC
        if std::path::Path::new("/usr/lib64/libcuda.so").exists()
            || std::path::Path::new("/usr/lib/x86_64-linux-gnu/libcuda.so").exists()
        {
            env::set_var("GST_VAAPI_ALL_DRIVERS", "1");
        }

        eprintln!("[Earth Reclaim] Starting with GStreamer hardware acceleration");
    }

    reclaim_lib::run()
}
