#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // verify-doc runs on plain files anywhere; only the git-backed commands
    // need a repo, and their own git calls fail cleanly without one.
    let repo = xtask::repo_root().unwrap_or_else(|_| std::path::PathBuf::from("."));
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
