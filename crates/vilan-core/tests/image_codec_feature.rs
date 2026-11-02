//! M185: vilan-core's `image-codec` feature (Cargo.toml says why it exists).
//!
//! This binary is built BOTH ways and pins each:
//!
//! - with the feature (the default, and the workspace suite's build), a program
//!   that reaches `std::image`'s codec carries `__image_wasm` — the blob;
//! - without it (`cargo test -p vilan-core --no-default-features --test
//!   image_codec_feature`, which `scripts/ci-local.sh wasm` runs — the
//!   playground's configuration), the same program is REFUSED by a diagnostic
//!   naming the feature, never emitted with an empty codec; and a program that
//!   does not reach the codec compiles exactly as before.
//!
//! The workspace suite only ever builds vilan-core WITH the feature (the CLI
//! enables it and features unify), so the refusal's pin runs in the wasm leg.

use std::path::{Path, PathBuf};

use vilan_core::{BuildOptions, Platform, Workspace, analyze_source, transform};

/// Compile `source` for node on a large-stack worker, as the CLI does.
fn compile(source: &'static str) -> Result<String, Vec<String>> {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let spec = vilan_core::manifest::resolve_std(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
            );
            let (program, errors) = analyze_source(
                source,
                &spec,
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).map_err(|error| vec![error.msg])
                }
                _ => Err(errors.into_iter().map(|error| error.msg).collect()),
            }
        })
        .expect("spawn worker")
        .join()
        .expect("the compiler thread")
}

const REACHES_THE_CODEC: &str = r#"
import std::bytes::Bytes;
import std::image::Image;
import std::io::print;
fun main() {
    print(Image::sniff(Bytes::alloc(4)).is_none());
}
"#;

/// Naming the module's types is not reaching the codec.
const DOES_NOT_REACH_IT: &str = r#"
import std::image::Fit;
import std::io::print;
fun main() {
    let fit = Fit::Cover;
    print(match fit { Fit::Cover => "cover", _ => "other" });
}
"#;

#[cfg(feature = "image-codec")]
#[test]
fn with_the_feature_a_program_reaching_the_codec_carries_it() {
    let emitted = compile(REACHES_THE_CODEC).expect("compiles");
    assert_eq!(emitted.matches("const __image_wasm = \"").count(), 1);
}

#[cfg(not(feature = "image-codec"))]
#[test]
fn without_the_feature_a_program_reaching_the_codec_is_refused_by_name() {
    let errors = compile(REACHES_THE_CODEC).expect_err("refused");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("`image-codec` feature") && error.contains("std::image")),
        "{errors:?}"
    );
}

#[test]
fn either_way_a_program_that_does_not_reach_the_codec_compiles_without_it() {
    let emitted = compile(DOES_NOT_REACH_IT).expect("compiles");
    assert!(!emitted.contains("__image"), "{emitted}");
}
