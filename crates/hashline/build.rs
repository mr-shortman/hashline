//! Compiles the GSettings schema for a development build.
//!
//! An installed Hashline finds its schema in the system directory; a build tree
//! has no such directory, and the application would then run without stored
//! preferences — or, worse, against a stale compiled schema. Compiling here
//! means the schema can never be out of date relative to the source that
//! declares it, which is exactly the trap that a hand-run
//! `glib-compile-schemas` sets.
//!
//! A missing `glib-compile-schemas` is not an error: the application degrades
//! to holding no preferences, as `preferences::Preferences` does anyway.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest
        .parent()
        .and_then(|crates| crates.parent())
        .map(|root| root.join("data"))
        .expect("workspace root");
    let schema = source.join("de.kalendium.Hashline.gschema.xml");
    println!("cargo::rerun-if-changed={}", schema.display());

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("schemas");
    // The path is handed to the binary either way, so a build without the
    // compiler still produces a program that runs.
    println!("cargo::rustc-env=HASHLINE_DEV_SCHEMA_DIR={}", out.display());

    if !schema.exists() {
        println!("cargo::warning=no schema at {}", schema.display());
        return;
    }
    if std::fs::create_dir_all(&out).is_err() {
        return;
    }
    if std::fs::copy(&schema, out.join("de.kalendium.Hashline.gschema.xml")).is_err() {
        return;
    }
    match Command::new("glib-compile-schemas").arg(&out).status() {
        Ok(status) if status.success() => {}
        Ok(status) => println!("cargo::warning=glib-compile-schemas failed: {status}"),
        Err(error) => println!("cargo::warning=glib-compile-schemas unavailable: {error}"),
    }
}
