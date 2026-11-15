fn main() {
    // The brand file is read into the program when it is built; a change to it must rebuild the program.
    println!("cargo:rerun-if-changed=../brand/brand.json");
    // The window's built files are packed into the program; a new build of them must rebuild the program too, even with a plain cargo build.
    println!("cargo:rerun-if-changed=../dist");
    tauri_build::build()
}
