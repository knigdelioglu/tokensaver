#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = tokensaver::run_desktop() {
        eprintln!("TokenSaver failed to start: {error}");
        std::process::exit(1);
    }
}
