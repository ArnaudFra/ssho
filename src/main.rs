fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match ssho::cli::run(&args) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Error: {error:#}");
            1
        }
    };
    std::process::exit(code);
}
