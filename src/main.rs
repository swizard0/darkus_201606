extern crate getopts;

use std::{io, env, process};
use std::io::{Write, BufReader, BufRead};
use std::fs::File;
use std::path::Path;
use std::num::ParseIntError;
use getopts::{Options, Matches};

#[derive(Debug)]
enum CmdArgsError {
    Getopts(getopts::Fail),
    NoTasksFileProvided,
    InvalidSlavesValue(String, ParseIntError),
}

#[derive(Debug)]
enum Error {
    CmdArgs(CmdArgsError),
    TasksOpen(io::Error),
    TasksRead(io::Error),
}

fn entrypoint(maybe_matches: getopts::Result) -> Result<(), Error> {
    let matches = try!(maybe_matches.map_err(|e| Error::CmdArgs(CmdArgsError::Getopts(e))));
    run(matches)
}

fn run(matches: Matches) -> Result<(), Error> {
    let tasks_filename = try!(matches.opt_str("tasks").ok_or(Error::CmdArgs(CmdArgsError::NoWordsFileProvided)));
    let slaves_count: usize = {
        let slaves_count_str = matches.opt_str("slaves").unwrap_or("4".to_string());
        try!(slaves_count_str.parse().map_err(|e| Error::CmdArgs(CmdArgsError::InvalidSlavesValue(slaves_count_str, e))))
    };

    println!("Starting with tasks_filename = {}, slaves_count = {}", tasks_filename, slaves_count);
    
    Ok(())
}

fn main() {
    let mut args = env::args();
    let cmd_proc = args.next().unwrap();
    let mut opts = Options::new();

    opts.optopt("t", "tasks", "tasks text file", "TASKS");
    opts.optopt("s", "slaves", "calculating slaves count", "SLAVES");
    match entrypoint(opts.parse(args)) {
        Ok(()) => (),
        Err(cause) => {
            let _ = writeln!(&mut io::stderr(), "Error: {:?}", cause);
            let usage = format!("Usage: {}", cmd_proc);
            let _ = writeln!(&mut io::stderr(), "{}", opts.usage(&usage[..]));
            process::exit(1);
        }
    }
}

