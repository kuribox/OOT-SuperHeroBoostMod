//! A throwaway check of the folder dialog (`cargo run -p shb-gui --example pick`).
//!
//! The window's "Examinar…" button opens the same dialog from a thread; this asks for it once and prints what came
//! back, so a dialog that is broken on this machine (no COM, no shell) is visible without the window in the way.

fn main() {
    println!("asking for a folder…");
    let picked = rfd::FileDialog::new()
        .set_title("Elige una carpeta (prueba)")
        .set_directory(std::env::current_dir().unwrap_or_default())
        .pick_folder();
    match picked {
        Some(path) => println!("picked: {}", path.display()),
        None => println!("cancelled (or the dialog could not open)"),
    }
}
