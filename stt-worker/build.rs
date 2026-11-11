fn main() {
    // With Vulkan the worker would not start on a machine that has no Vulkan at all.
    // Loading it late lets the worker start there; the app then has it do without.
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if windows && msvc && std::env::var_os("CARGO_FEATURE_VULKAN").is_some() {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:vulkan-1.dll");
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
        // The recognizer names the Vulkan library and leaves finding it to the linker.
        println!("cargo:rerun-if-env-changed=VULKAN_SDK");
        if let Some(sdk) = std::env::var_os("VULKAN_SDK") {
            let libraries = std::path::Path::new(&sdk).join("Lib");
            println!("cargo:rustc-link-search=native={}", libraries.display());
        }
    }
}
