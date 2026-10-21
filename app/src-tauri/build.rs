fn main() {
    // tauri-build 不会因为图标文件变了而重跑，换图标后 exe 里还是旧的：显式监视图标目录。
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
