//! 编译 vendor/ 下的 tinyuz（C++，MIT）：灯效帧数据下发前要用它压缩，设备固件用对应的解压器。
//! 构建方式与 lian-li-linux 相同。

fn main() {
    let vendor = std::path::Path::new("../../vendor");
    let tinyuz = vendor.join("tinyuz");
    let hdiff = vendor.join("HDiffPatch");

    let sources = [
        vendor.join("tuz_wrapper.cpp"),
        tinyuz.join("compress/tuz_enc.cpp"),
        tinyuz.join("compress/tuz_enc_private/tuz_enc_clip.cpp"),
        tinyuz.join("compress/tuz_enc_private/tuz_enc_code.cpp"),
        tinyuz.join("compress/tuz_enc_private/tuz_enc_match.cpp"),
        tinyuz.join("compress/tuz_enc_private/tuz_sstring.cpp"),
        hdiff.join("libHDiffPatch/HDiff/private_diff/libdivsufsort/divsufsort.cpp"),
    ];

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .opt_level(3)
        .define("NDEBUG", None)
        // 不需要多线程压缩，省掉线程库依赖。
        .define("_IS_USED_MULTITHREAD", "0")
        .include(vendor)
        .include(&tinyuz)
        .include(&hdiff)
        .warnings(false);
    for src in &sources {
        build.file(src);
    }
    // 解压器只给往返测试用。
    build.file(tinyuz.join("decompress/tuz_dec.c"));
    build.compile("tinyuz");

    println!("cargo:rerun-if-changed=../../vendor");
}
