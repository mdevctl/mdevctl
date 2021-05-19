use std::env;
use std::fs;
use structopt::clap::Shell;
use structopt::StructOpt;

#[path = "src/cli.rs"]
mod cli;

fn main() {
    let outdir = env::var_os("OUT_DIR").expect("OUT_DIR environemnt variable not defined");
    fs::create_dir_all(&outdir).expect("unable to create out dir");

    let mut mdevctl = cli::MdevctlCommands::clap();
    mdevctl.gen_completions("mdevctl", Shell::Bash, &outdir);

    let mut lsmdev = cli::LsmdevOptions::clap();
    lsmdev.gen_completions("lsmdev", Shell::Bash, &outdir);
}
