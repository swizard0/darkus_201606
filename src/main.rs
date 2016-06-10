extern crate getopts;

use std::{io, env, process};
use std::io::{Write, BufReader, BufRead};
use std::fs::File;
use std::path::Path;
use std::num::ParseIntError;
use getopts::{Options, Matches};

#[derive(Debug)]
enum Error {
    CmdArgs(CmdArgsError),
    TasksOpen(io::Error),
    TasksRead(io::Error),
    TasksParse(ParsingError),
}

#[derive(Debug)]
enum CmdArgsError {
    Getopts(getopts::Fail),
    NoTasksFileProvided,
    InvalidSlavesValue(String, ParseIntError),
}

#[derive(Debug)]
enum ParsingError {
    MissingField(String),
    MissingTiles(String),
    BadField(String, MatrixError),
    BadTile(String, String, MatrixError),
}

#[derive(Debug)]
enum MatrixError {
    Empty,
    NotSquare,
    RowsCountGreaterThan8,
    ColsCountGreaterThan8,
}

fn entrypoint(maybe_matches: getopts::Result) -> Result<(), Error> {
    let matches = try!(maybe_matches.map_err(|e| Error::CmdArgs(CmdArgsError::Getopts(e))));
    run(matches)
}

#[derive(Debug)]
struct Matrix {
    rows: usize,
    cols: usize,
    bits: u64,
}

#[derive(Debug)]
struct Tile {
    area: Matrix,
}

#[derive(Debug)]
struct Task {
    field: Matrix,
    tiles: Vec<Tile>,
}

fn load_tasks<P>(tasks_filename: P) -> Result<Vec<Task>, Error> where P: AsRef<Path> {
    let mut in_stream =
        BufReader::new(try!(File::open(tasks_filename).map_err(Error::TasksOpen)));
    let mut line = String::new();
    let mut tasks = Vec::new();
    enum State {
        WaitingTask,
        WaitingField(String),
        ReadingField(String),
        WaitingTile(String, Task),
        ReadingTile(String, String, Task),
    }
    let mut state = State::WaitingTask;
    let mut area = Vec::new();
    fn area_to_matrix(area: &[String]) -> Result<Matrix, MatrixError> {
        let mut iter = area.iter();
        if let Some(first_row) = iter.next() {
            let rows = area.len();
            let cols = first_row.len();
            for next_row in iter {
                if next_row.len() != cols {
                    return Err(MatrixError::NotSquare);
                }
            }
            if rows > 8 {
                Err(MatrixError::RowsCountGreaterThan8)
            } else if cols > 8 {
                Err(MatrixError::ColsCountGreaterThan8)
            } else {
                let mut bits = 0;
                let mut bit_index = 0;
                for row in area.iter() {
                    for ch in row.chars() {
                        if ch == '1' {
                            bits |= 1 << bit_index;
                        }
                        bit_index += 1;
                    }
                }
                Ok(Matrix {
                    rows: rows,
                    cols: cols,
                    bits: bits,
                })
            }
        } else {
            Err(MatrixError::Empty)
        }
    }        
    
    loop {
        line.clear();
        match in_stream.read_line(&mut line) {
            Ok(len) => {
                let trimmed_line = line[0 .. len].trim_matches(|c: char| c.is_whitespace());
                loop {
                    match state {
                        State::WaitingTask =>
                            if len == 0 {
                                return Ok(tasks);
                            } else if trimmed_line.starts_with("= ЗАДАЧА") {
                                state = State::WaitingField(trimmed_line.to_owned());
                            },
                        
                        State::WaitingField(task_id) =>
                            if len == 0 {
                                return Err(Error::TasksParse(ParsingError::MissingField(task_id.clone())));
                            } else if trimmed_line == "Поле:" {
                                area.clear();
                                state = State::ReadingField(task_id);
                            } else {
                                state = State::WaitingField(task_id);
                            },

                        State::ReadingField(task_id) =>
                            if trimmed_line.len() > 0 && trimmed_line.chars().all(|c| c == '0' || c == '1') {
                                area.push(trimmed_line.to_owned());
                                state = State::ReadingField(task_id);
                            } else {
                                let field = try!(area_to_matrix(&area).map_err(|e| Error::TasksParse(ParsingError::BadField(task_id.clone(), e))));
                                let task = Task {
                                    field: field,
                                    tiles: Vec::new(),
                                };
                                state = State::WaitingTile(task_id, task);
                                continue;
                            },
                        
                        State::WaitingTile(task_id, task) =>
                            if len == 0 && task.tiles.is_empty() {
                                return Err(Error::TasksParse(ParsingError::MissingTiles(task_id)));
                            } else if trimmed_line.starts_with("= ЗАДАЧА") && task.tiles.is_empty() {
                                return Err(Error::TasksParse(ParsingError::MissingTiles(task_id)));
                            } else if len == 0 || trimmed_line.starts_with("= ЗАДАЧА") {
                                tasks.push(task);
                                state = State::WaitingTask;
                                continue;
                            } else if trimmed_line.starts_with("Фигура") {
                                area.clear();
                                state = State::ReadingTile(task_id, trimmed_line.to_owned(), task);
                            } else {
                                state = State::WaitingTile(task_id, task);
                            },
                        
                        State::ReadingTile(task_id, tile_id, mut task) =>
                            if trimmed_line.len() > 0 && trimmed_line.chars().all(|c| c == '0' || c == '1') {
                                area.push(trimmed_line.to_owned());
                                state = State::ReadingTile(task_id, tile_id, task);
                            } else {
                                let tile_area =
                                    try!(area_to_matrix(&area).map_err(|e| Error::TasksParse(ParsingError::BadTile(task_id.clone(), tile_id, e))));
                                let tile = Tile {
                                    area: tile_area,
                                };
                                task.tiles.push(tile);
                                state = State::WaitingTile(task_id, task);
                                continue;
                            },
                    }
                    break;
                }
            },
            Err(e) =>
                return Err(Error::TasksRead(e)),
        }
    }

}

fn run(matches: Matches) -> Result<(), Error> {
    let tasks_filename = try!(matches.opt_str("tasks").ok_or(Error::CmdArgs(CmdArgsError::NoTasksFileProvided)));
    let slaves_count: usize = {
        let slaves_count_str = matches.opt_str("slaves").unwrap_or("4".to_string());
        try!(slaves_count_str.parse().map_err(|e| Error::CmdArgs(CmdArgsError::InvalidSlavesValue(slaves_count_str, e))))
    };

    println!("Starting with tasks_filename = {}, slaves_count = {}.", tasks_filename, slaves_count);
    
    let tasks = try!(load_tasks(tasks_filename));
    println!("Total {} tasks loaded.", tasks.len());

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
