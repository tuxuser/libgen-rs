use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
   #[clap(short, long, value_parser)]
   pub search: Option<String>,

   #[clap(short, long, value_parser)]
   pub results: Option<u32>,

   #[clap(short = 'o', long, value_parser)]
   pub search_option: Option<String>,

   #[clap(short, long, value_parser)]
   pub download: Option<String>,
}
