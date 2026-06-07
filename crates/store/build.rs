fn main() {
    // Nothing to do; the kuzu crate's own build script handles the
    // C++ static-link chain. We only need to print something so cargo
    // picks up changes when switching feature flags.
    let _ = std::env::var("CARGO_FEATURE_KUZU");
}
