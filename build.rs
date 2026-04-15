fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        if let Err(e) = res.compile() {
            eprintln!("winresource compile failed: {e}");
        }
    }
}
