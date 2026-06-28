// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Initialize logging
    env_logger::init();

    // Set environment for Linux GPU/WebKit compatibility
    #[cfg(target_os = "linux")]
    {
        use std::env;

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
