//! Temporary entry point: lists the hosts of an SSH configuration file.
use std::path::PathBuf;

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".ssh/config")
        });
    match ssho::config::load_hosts(&path) {
        Ok(hosts) => {
            for host in hosts {
                println!("{}\t{}", host.name, host.hostname);
            }
        }
        Err(error) => {
            eprintln!("Error: {error:#}");
            std::process::exit(1);
        }
    }
}
