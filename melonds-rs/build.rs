use cc::Build;

fn main() {
    let mut interface_builder = Build::new();
    interface_builder.cpp(true);
    interface_builder.std("c++20"); // need C++20 for semaphores/threading (melonDS runs way slower without threading)
    interface_builder.file("interface.cpp");
    interface_builder.warnings(false);
    interface_builder.compile("melonds-rs-interface");

    println!("cargo::rerun-if-changed=interface.cpp");
    
    // let mut config = cmake::Config::new("melonDS");
    // config.define("ENABLE_JIT", "NO");
    // config.define("ENABLE_OGLRENDERER", "NO");
    // config.define("ENABLE_GDBSTUB", "NO");
    // config.define("BUILD_QT_SDL", "NO");
    //
    // config.build_target("core");
    // let b = config.build();
    // println!("cargo::rustc-link-search=native={}/build/src", b.display());
    //
    // config.build_target("teakra");
    // let b = config.build();
    // println!("cargo::rustc-link-search=native={}/build/src/teakra/src", b.display());
    //
    // println!("cargo::rustc-link-lib=core");
    // println!("cargo::rustc-link-lib=teakra");
}
