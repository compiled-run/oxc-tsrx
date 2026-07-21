fn main() {
    for name in [
        "OXC_TSRX_STAGE4_ADAPTER_SHA256",
        "OXC_TSRX_STAGE4_ENGINE_MANIFEST_SHA256",
        "OXC_TSRX_STAGE4_ENGINE_SHA256",
        "OXC_TSRX_STAGE4_ENGINE_UTF16_SHA256",
        "OXC_TSRX_STAGE4_ENGINE_BRIDGE_SHA256",
        "OXC_TSRX_STAGE4_BINDING_MANIFEST_SHA256",
        "OXC_TSRX_STAGE4_BINDING_SHA256",
    ] {
        println!("cargo:rerun-if-env-changed={name}");
    }
    napi_build::setup();
}
