//! Docs-only helper. Reads TSRX from stdin and prints JSON with the exact
//! legal-TSX lint projection and structural overlay the real engine produces:
//! `{"projected": "...", "tokens": [...], "counts": {...}}` on success,
//! `{"error": "..."}` on failure.
use std::io::Read;

fn main() {
    // --types: emit the type-semantic projection instead (for TS completions).
    let types_mode = std::env::args().any(|argument| argument == "--types");
    let mut source = String::new();
    if std::io::stdin().read_to_string(&mut source).is_err() {
        println!("{}", serde_json::json!({ "error": "stdin was not valid UTF-8" }));
        std::process::exit(1);
    }
    let overlay = match tsrx_syntax::scan(&source) {
        Ok(overlay) => overlay,
        Err(error) => {
            println!("{}", serde_json::json!({ "error": error.to_string() }));
            std::process::exit(1);
        }
    };
    let tokens = overlay
        .tokens()
        .iter()
        .map(|token| {
            serde_json::json!({
                "kind": format!("{:?}", token.kind),
                "start": token.span.start,
                "end": token.span.end,
            })
        })
        .collect::<Vec<_>>();
    if types_mode {
        match tsrx_syntax::project_for_types(&source, &overlay) {
            Ok(projection) => {
                println!("{}", serde_json::json!({ "projected": projection.source() }));
            }
            Err(error) => {
                println!("{}", serde_json::json!({ "error": error.to_string() }));
                std::process::exit(1);
            }
        }
        return;
    }
    match tsrx_syntax::project_for_lint(&source, &overlay) {
        Ok(projection) => {
            println!(
                "{}",
                serde_json::json!({
                    "projected": projection.source(),
                    "tokens": tokens,
                    "counts": {
                        "controls": overlay.control_count(),
                        "dynamicTags": overlay.dynamic_tag_count(),
                        "styleBlocks": overlay.style_block_count(),
                    },
                })
            );
        }
        Err(error) => {
            println!("{}", serde_json::json!({ "error": error.to_string() }));
            std::process::exit(1);
        }
    }
}
