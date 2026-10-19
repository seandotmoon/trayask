use std::path::Path;

/// Release builds embed `../dist` into the binary. `dist/` is gitignored, so a
/// Linux `cargo build --release` that skips the frontend step compiles cleanly
/// and then every window shows "asset not found: index.html". Fail that build
/// instead. `tauri dev` is a debug build and loads `devUrl`, so it does not
/// need the file.
fn main() {
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        let index = Path::new("../dist/index.html");
        if !index.is_file() {
            panic!(
                "release build requires {} — run `pnpm build` from the app root before packaging",
                index.display()
            );
        }
    }

    tauri_build::build();
}
