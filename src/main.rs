use std::io;
use std::process;

use clap::{Arg, Command};
use mdbook_preprocessor::errors::Result;
use mdbook_preprocessor::Preprocessor;
use mdbook_treesitter::TreesitterPreprocessor;

fn make_app() -> Command {
    Command::new("mdbook-treesitter")
        .about("An mdBook preprocessor that uses tree-sitter to extract code from files")
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
}

fn main() {
    let matches = make_app().get_matches();
    let preprocessor = TreesitterPreprocessor;

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        let renderer = sub_args
            .get_one::<String>("renderer")
            .expect("Required argument");
        let supported = preprocessor.supports_renderer(renderer).unwrap_or(false);
        process::exit(if supported { 0 } else { 1 });
    } else if let Err(e) = handle_preprocessing(&preprocessor) {
        eprintln!("{e:?}");
        process::exit(1);
    }
}

fn handle_preprocessing(pre: &dyn Preprocessor) -> Result<()> {
    let (ctx, book) = mdbook_preprocessor::parse_input(io::stdin())?;
    let processed_book = pre.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;
    Ok(())
}
