fn main() {
    println!("cargo:rerun-if-changed=../brand/brand.json");
    // The window's files are built into the program, so a change there needs a rebuild.
    println!("cargo:rerun-if-changed=../ui/index.html");
    println!("cargo:rerun-if-changed=../ui/css");
    println!("cargo:rerun-if-changed=../ui/js");
    println!("cargo:rerun-if-changed=../ui/brand");
    tauri_build::build()
}
