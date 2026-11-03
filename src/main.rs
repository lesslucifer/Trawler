fn main() {
    std::process::exit(trawler::cli::main(std::env::args_os().skip(1)));
}
