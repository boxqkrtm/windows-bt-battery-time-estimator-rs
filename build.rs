fn main() {
    slint_build::compile("ui/appwindow.slint").unwrap();
    
    // Include Windows resources (icon, version info)
    #[cfg(target_os = "windows")]
    {
        embed_resource::compile("app.rc", embed_resource::NONE);
    }
} 