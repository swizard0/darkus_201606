extern crate getopts;
extern crate jobsteal;

use std::{io, env, process};
use std::io::{Write, BufReader, BufRead};
use std::fs::File;
use std::path::Path;
use std::num::ParseIntError;
use getopts::{Options, Matches};
use jobsteal::{make_pool, IntoSpliterator, Spliterator};

#[derive(Debug)]
enum Error {
    CmdArgs(CmdArgsError),
    TasksOpen(io::Error),
    TasksRead(io::Error),
    TasksParse(ParsingError),
    JobStealPool(io::Error),
    Solve(SolvingError),
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
enum SolvingError {
    NoSolution(String),
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
    row_mask: u64,
    bits: u64,
}

#[derive(Debug)]
struct Tile {
    area: Matrix,
}

#[derive(Debug)]
struct Task {
    id: String,
    field: Matrix,
    tiles: Vec<Tile>,
}

impl Tile {
    fn install(&self, row: usize, col: usize, mut bits: u64, cols: usize) -> u64 {
        for j in 0 .. self.area.rows {
            let row_offset = self.area.cols * j;
            let mask = self.area.row_mask << row_offset;
            let row_bits = (self.area.bits & mask) >> row_offset;
            let clear_mask = !((row_bits << col) << ((row + j) * cols));
            bits &= clear_mask;
        }
        bits
    }
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
        WaitingTile(Task),
        ReadingTile(String, Task),
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
                let mut row_mask = 0;
                for row in area.iter() {
                    row_mask = 0;
                    for ch in row.chars() {
                        if ch == '1' {
                            bits |= 1 << bit_index;
                        }
                        bit_index += 1;
                        row_mask = (row_mask << 1) | 1;
                    }
                }
                Ok(Matrix {
                    rows: rows,
                    cols: cols,
                    row_mask: row_mask,
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
                                    id: task_id,
                                    field: field,
                                    tiles: Vec::new(),
                                };
                                state = State::WaitingTile(task);
                                continue;
                            },

                        State::WaitingTile(task) =>
                            if len == 0 && task.tiles.is_empty() {
                                return Err(Error::TasksParse(ParsingError::MissingTiles(task.id)));
                            } else if trimmed_line.starts_with("= ЗАДАЧА") && task.tiles.is_empty() {
                                return Err(Error::TasksParse(ParsingError::MissingTiles(task.id)));
                            } else if len == 0 || trimmed_line.starts_with("= ЗАДАЧА") {
                                tasks.push(task);
                                state = State::WaitingTask;
                                continue;
                            } else if trimmed_line.starts_with("Фигура") {
                                area.clear();
                                state = State::ReadingTile(trimmed_line.to_owned(), task);
                            } else {
                                state = State::WaitingTile(task);
                            },

                        State::ReadingTile(tile_id, mut task) =>
                            if trimmed_line.len() > 0 && trimmed_line.chars().all(|c| c == '0' || c == '1') {
                                area.push(trimmed_line.to_owned());
                                state = State::ReadingTile(tile_id, task);
                            } else {
                                let tile_area =
                                    try!(area_to_matrix(&area).map_err(|e| Error::TasksParse(ParsingError::BadTile(task.id.clone(), tile_id, e))));
                                let tile = Tile {
                                    area: tile_area,
                                };
                                task.tiles.push(tile);
                                state = State::WaitingTile(task);
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

    let tasks = try!(load_tasks(tasks_filename));
    let mut pool = try!(make_pool(slaves_count).map_err(Error::JobStealPool));
    let results: Vec<_> = tasks
        .into_split_iter()
        .map(|Task { id: task_id, field: task_field, tiles: task_tiles }| {
            struct TileState {
                tile: Tile,
                row: usize,
                col: usize,
                field_bits: u64,
            }

            let mut stack: Vec<_> = task_tiles
                .into_iter()
                .map(|tile| TileState {
                    tile: tile,
                    row: 0,
                    col: 0,
                    field_bits: task_field.bits,
                })
                .collect();

            let mut sp = 0;
            loop {
                if stack[sp].col + stack[sp].tile.area.cols > task_field.cols {
                    stack[sp].col = 0;
                    stack[sp].row += 1;
                }
                if stack[sp].row + stack[sp].tile.area.rows > task_field.rows {
                    if sp == 0 {
                        return Err(Error::Solve(SolvingError::NoSolution(task_id)));
                    } else {
                        stack[sp - 1].col += 1;
                        sp -= 1;
                        continue;
                    }
                }

                let installed_bits =
                    stack[sp].tile.install(stack[sp].row, stack[sp].col, stack[sp].field_bits, task_field.cols);

                if sp + 1 < stack.len() {
                    stack[sp + 1].row = 0;
                    stack[sp + 1].col = 0;
                    stack[sp + 1].field_bits = installed_bits;
                    sp += 1;
                } else if installed_bits == 0 {
                    break;
                } else {
                    stack[sp].col += 1;
                }
            }

            let coords: Vec<_> = stack
                .into_iter()
                .map(|TileState { row: r, col: c, .. }| (c, r))
                .collect();
            Ok(coords)
        })
        .collect(&pool.spawner());

    for maybe_coords in results {
        for (i, (x, y)) in try!(maybe_coords).into_iter().enumerate() {
            print!("{}({}, {})", if i == 0 { "" } else { ", " }, x, y);
        }
        println!("");
    }

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
