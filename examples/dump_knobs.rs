//! Print bundled layout knob defaults (`defaults/*.toml`).
//!
//! ```bash
//! cargo run --example dump_knobs
//! ```

use ariadnes_weave::LayoutKnobs;

fn main() {
    println!("{}", LayoutKnobs::bundled().describe());
}
