fn main() {
    // The brand file is read into the program when it is built; a change to it must rebuild the program.
    println!("cargo:rerun-if-changed=../brand/brand.json");
    tauri_build::build()
}
