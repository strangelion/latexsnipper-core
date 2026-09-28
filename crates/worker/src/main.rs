use std::io;

fn main() {
    if let Err(error) = latexsnipper_worker::run(io::stdin().lock(), io::stdout().lock()) {
        eprintln!("latexsnipper-worker: {error}");
        std::process::exit(1);
    }
}
