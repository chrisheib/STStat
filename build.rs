use std::path::Path;

/// Embeds the Google OAuth client secret at compile time via an env var.
/// If the file is absent the env var is left unset and the constant falls back to an empty string,
/// which causes `has_oauth_client_config` to return false unless a local override is present.
fn main() {
    let secret_path = Path::new("google_client_secret.json");
    println!("cargo:rerun-if-changed={}", secret_path.display());

    if let Ok(content) = std::fs::read_to_string(secret_path) {
        println!("cargo:rustc-env=GOOGLE_CLIENT_SECRET_EMBEDDED={content}");
    }
}
