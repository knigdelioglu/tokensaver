#![forbid(unsafe_code)]

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();

    if tokensaver::should_run_cli(&args) {
        match tokensaver::run_cli(args) {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("TokenSaver CLI error: {error}");
                std::process::exit(2);
            }
        }
    }

    if let Err(error) = tokensaver::run_desktop() {
        eprintln!("TokenSaver failed to start: {error}");
        std::process::exit(1);
    }
}
