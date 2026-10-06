//! Writes the factory bank from the reviewable source table in `src/preset_designs.rs`.
//!
//! The generation law itself lives in `preset_designs::generate` so the bank's tests can compare
//! the committed files against it. This example only clears the directory and writes.

use std::fs;
use std::path::PathBuf;

use mxm_fx_delay::preset_designs::generate;

fn main() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("presets");
    fs::create_dir_all(&output).expect("create presets directory");
    for entry in fs::read_dir(&output).expect("read presets directory") {
        let path = entry.expect("preset entry").path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            fs::remove_file(path).expect("remove old generated preset");
        }
    }

    let generated = generate();
    for (slug, json) in &generated {
        fs::write(output.join(format!("{slug}.json")), json).expect("write generated preset");
    }
    println!("wrote {} factory recipes", generated.len());
}
