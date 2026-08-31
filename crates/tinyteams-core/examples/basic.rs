//! Minimal end-to-end usage of the crate.
//!
//! Examples are compiled and linted in CI, so they cannot drift from the API.
//! Run it with:
//!
//! ```sh
//! cargo run --example basic
//! ```

use template::{Result, greet};

fn main() -> Result<()> {
    println!("{}", greet("Rust")?);

    // Failure modes are part of the public contract; show them too.
    match greet("   ") {
        Ok(greeting) => println!("{greeting}"),
        Err(error) => println!("expected failure: {error}"),
    }

    Ok(())
}
