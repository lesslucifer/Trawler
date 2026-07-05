#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let repo = match xtask::repo_root() {
        Ok(root) => root,
        Err(e) => {
            eprintln!("error: {e}");
            exit(2);
        }
    };
    match xtask::run(&args, &repo) {
        Ok((code, message)) => {
            if !message.is_empty() {
                println!("{message}");
            }
            exit(code);
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit(2);
        }
    }
}
