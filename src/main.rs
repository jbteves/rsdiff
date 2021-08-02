/// rsdiff
/// Will use Rust to perform abstracted diff

// Build a friendly CLI
use clap::{Arg, App};
// Use our own library
use rsdiff::differ;

/// Run a differ on two objects
fn main() {
    
    let matches = App::new("rsdiff")
                    .version("0.1")
                    .author("Joshua B. Teves <joshua.teves@nih.gov>")
                    .about("Performs abstract diffs")
                    .arg(Arg::with_name("left")
                         .help("The left object to diff")
                         .required(true))
                    .arg(Arg::with_name("right")
                         .help("The right object to diff")
                         .required(true))
                    .arg(Arg::with_name("debug")
                         .long("debug")
                         .takes_value(false)
                         .help("Run in debug mode")
                         .required(false))
                    .get_matches();

    let left = matches.value_of("left").unwrap();
    let right = matches.value_of("right").unwrap();
    let d = differ(left, right);
    if !d.matches {
        println!("{}", d.report);
    }
    if matches.is_present("debug") {
        println!("{:?}", d);
    }
}
