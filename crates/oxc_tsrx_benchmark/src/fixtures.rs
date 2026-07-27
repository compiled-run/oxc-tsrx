use std::{
    env,
    fmt::Write as _,
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn build_corpus(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 1024);
    source.push_str("type BenchmarkProps = { ready: boolean; value: number };\n\n");
    let mut index = 0usize;
    while source.len() < target_bytes {
        let debugger = if index.is_multiple_of(64) { "    debugger;\n" } else { "" };
        write!(
            source,
            "export function View{index:05}({{ ready, value }}: BenchmarkProps) @{{\n\
             \x20 const contact = \"@if@example.com\";\n\
             \x20 const label = `item-${{value}}-@else`;\n\
             \x20 // @if (false) {{ debugger; }}\n\
             \x20 @if (ready) {{\n\
             {debugger}\
             \x20   <main data-contact={{contact}} data-value={{value}}>{{label}}</main>;\n\
             \x20 }} @else {{\n\
             \x20   <aside>idle</aside>;\n\
             \x20 }}\n\
             }}\n\n"
        )
        .expect("writing to a String cannot fail");
        index += 1;
    }
    source
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(crate) fn create_temp_directory() -> Result<PathBuf, String> {
    let nonce =
        SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos();
    let directory = env::temp_dir()
        .join(format!("oxc-tsrx-native-lint-benchmark-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory)
        .map_err(|error| format!("unable to create {}: {error}", directory.display()))?;
    Ok(directory)
}
