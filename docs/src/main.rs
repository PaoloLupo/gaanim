mod world;

use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Watch mode
    #[arg(short = 'w', long)]
    watch: bool,
}

fn main() {
    let args = Args::parse();
    if args.watch {
        watch();
    } else {
        compile();
    }
}

fn compile() {}

fn watch() {
    todo!()
}
