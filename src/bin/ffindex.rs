use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

extern crate clap;
use clap::{Arg, ArgAction, Command as ClapCommand};

extern crate csv;
use csv::ReaderBuilder;

use ffindexrs::{
    FFindexWriter, ffindex_db_open, ffindex_get_data_by_index, ffindex_get_data_by_name,
    load_index, sort_index_file,
};

/// How entry keys are derived when building a database.
#[derive(Clone, Copy)]
enum KeyMode
{
    /// the input file's base name (`build` default)
    Basename,
    /// a running integer 1, 2, 3, ...
    Sequential,
    /// the first whitespace-delimited token of a FASTA header
    Header,
}

/// get the keys from a listfile (one key per line; extra tab-separated columns are ignored)
pub fn get_keys_from_file(path: String) -> Vec<String>
{
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .flexible(true)
        .from_path(path)
        .expect("listfile reader");

    rdr.records()
        .map(|record| {
            record
                .expect("Reading listfile")
                .get(0)
                .expect("empty line in listfile")
                .to_string()
        })
        .collect()
}

/// Drop a single trailing '\0' separator so output matches the original payload.
fn strip_separator(data: &[u8]) -> &[u8]
{
    match data.last()
    {
        Some(0) => &data[..data.len() - 1],
        _ => data,
    }
}

/// Gather keys from the optional `-k`/`-f` arguments of a subcommand.
fn collect_keys(submatches: &clap::ArgMatches) -> Vec<String>
{
    let mut keys: Vec<String> = submatches
        .get_many::<String>("listfile")
        .map(|files| {
            files
                .flat_map(|listfile| get_keys_from_file(listfile.to_string()))
                .collect()
        })
        .unwrap_or_default();

    if let Some(values) = submatches.get_many::<String>("key")
    {
        keys.extend(values.map(|k| k.to_string()));
    }
    keys
}

fn ffindex_get(ffindex_path: String, ffdata_path: String, keys: Vec<String>)
{
    let ffindex_db = ffindex_db_open(ffindex_path, ffdata_path);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for key in keys
    {
        match ffindex_get_data_by_name(&ffindex_db, key.clone())
        {
            Some(data) => out
                .write_all(strip_separator(data))
                .expect("write to stdout"),
            None => eprintln!("ffindex: key not found: {}", key),
        }
    }
}

/// Expand the given input paths: directories contribute each of their files.
fn expand_inputs(inputs: Vec<PathBuf>) -> Vec<PathBuf>
{
    let mut files = Vec::new();
    for path in inputs
    {
        if path.is_dir()
        {
            let mut dir_files: Vec<PathBuf> = fs::read_dir(&path)
                .unwrap_or_else(|e| panic!("read_dir {}: {}", path.display(), e))
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|p| p.is_file())
                .collect();
            dir_files.sort();
            files.extend(dir_files);
        }
        else
        {
            files.push(path);
        }
    }
    files
}

fn ffindex_build(
    ffdata_path: String,
    ffindex_path: String,
    append: bool,
    sort: bool,
    key_mode: KeyMode,
    inputs: Vec<PathBuf>,
)
{
    let mut writer = FFindexWriter::create(&ffdata_path, &ffindex_path, append)
        .expect("could not create ffindex database");

    let mut counter: u64 = 0;
    for path in expand_inputs(inputs)
    {
        counter += 1;
        let name = match key_mode
        {
            KeyMode::Sequential => counter.to_string(),
            // Basename is the default; Header is rejected for `build` by clap.
            _ => path
                .file_name()
                .expect("input path has no file name")
                .to_string_lossy()
                .to_string(),
        };
        let content = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
        writer.insert(&name, &content).expect("insert entry");
    }

    writer.finish().expect("finish writing database");

    if sort
    {
        sort_index_file(&ffindex_path).expect("sort index");
    }
}

/// The first whitespace-delimited token of a FASTA header line (leading '>' removed).
fn header_token(line: &str) -> String
{
    line[1..].split_whitespace().next().unwrap_or("").to_string()
}

fn ffindex_from_fasta(
    ffdata_path: String,
    ffindex_path: String,
    sort: bool,
    key_mode: KeyMode,
    fasta_path: String,
)
{
    use std::io::{BufRead, BufReader};

    let mut writer = FFindexWriter::create(&ffdata_path, &ffindex_path, false)
        .expect("could not create ffindex database");

    let file = fs::File::open(&fasta_path).expect("open fasta file");
    let reader = BufReader::new(file);

    let mut current: Vec<u8> = Vec::new();
    let mut counter: u64 = 0;
    let mut header: Option<String> = None;

    // Pick a key for the record we are about to flush, falling back to the
    // running counter when a header is missing or empty.
    let key_for = |key_mode: KeyMode, counter: &mut u64, header: Option<String>| -> String {
        *counter += 1;
        match key_mode
        {
            KeyMode::Header => match header
            {
                Some(h) if !h.is_empty() => h,
                _ => counter.to_string(),
            },
            // Basename is rejected for `from_fasta` by clap; treat as sequential.
            _ => counter.to_string(),
        }
    };

    for line in reader.lines()
    {
        let line = line.expect("read fasta line");
        if line.starts_with('>')
        {
            if !current.is_empty()
            {
                let key = key_for(key_mode, &mut counter, header.take());
                writer.insert(&key, &current).expect("insert fasta entry");
                current.clear();
            }
            header = Some(header_token(&line));
        }
        current.extend_from_slice(line.as_bytes());
        current.push(b'\n');
    }
    if !current.is_empty()
    {
        let key = key_for(key_mode, &mut counter, header.take());
        writer.insert(&key, &current).expect("insert fasta entry");
    }

    writer.finish().expect("finish writing database");

    if sort
    {
        sort_index_file(&ffindex_path).expect("sort index");
    }
}

fn ffindex_modify(ffindex_path: String, sort: bool, unlink: bool, keys: Vec<String>)
{
    if unlink
    {
        let to_remove: HashSet<String> = keys.into_iter().collect();
        let entries = load_index(ffindex_path.clone());
        let mut writer =
            io::BufWriter::new(fs::File::create(&ffindex_path).expect("rewrite index"));
        for entry in &entries
        {
            if !to_remove.contains(entry.name())
            {
                writeln!(writer, "{}\t{}\t{}", entry.name(), entry.offset(), entry.length())
                    .expect("write index line");
            }
        }
        writer.flush().expect("flush index");
    }

    if sort
    {
        sort_index_file(&ffindex_path).expect("sort index");
    }
}

fn ffindex_apply(
    ffdata_path: String,
    ffindex_path: String,
    program: Vec<String>,
    out_ffdata: Option<String>,
    out_ffindex: Option<String>,
)
{
    let ffindex_db = ffindex_db_open(ffindex_path, ffdata_path);
    let (cmd, args) = program.split_first().expect("no program given");

    // When an output database is requested, capture each program's stdout into it.
    let mut writer = match (out_ffdata, out_ffindex)
    {
        (Some(data), Some(index)) => Some(
            FFindexWriter::create(&data, &index, false).expect("could not create output database"),
        ),
        _ => None,
    };

    for index in 0..ffindex_db.entries().len()
    {
        let name = ffindex_db.entries()[index].name().to_string();
        let data = match ffindex_get_data_by_index(&ffindex_db, index)
        {
            // own the bytes so they can move into the stdin-writer thread
            Some(data) => strip_separator(data).to_vec(),
            None => continue,
        };

        let mut command = Command::new(cmd);
        command.args(args).stdin(Stdio::piped());
        if writer.is_some()
        {
            command.stdout(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {}: {}", cmd, e));

        // Feed the entry on a separate thread to avoid deadlocking on large output.
        let mut stdin = child.stdin.take().expect("child stdin");
        let feeder = std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });

        match writer.as_mut()
        {
            Some(w) => {
                let mut output = Vec::new();
                child
                    .stdout
                    .take()
                    .expect("child stdout")
                    .read_to_end(&mut output)
                    .expect("read child stdout");
                feeder.join().expect("stdin feeder thread");
                child.wait().expect("wait for child");
                w.insert(&name, &output).expect("insert output entry");
            }
            None => {
                feeder.join().expect("stdin feeder thread");
                child.wait().expect("wait for child");
            }
        }
    }

    if let Some(w) = writer
    {
        w.finish().expect("finish output database");
    }
}

fn main() -> io::Result<()>
{
    let app = ClapCommand::new("ffindex")
        .version("0.1")
        .about("FFindex flat file database")
        .author("Andreas Hauser")
        .arg_required_else_help(true)
        .subcommand(
            ClapCommand::new("get")
                .about("extract records by key")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("ffindex")
                        .short('i')
                        .required(true)
                        .help("index file"),
                )
                .arg(
                    Arg::new("ffdata")
                        .short('d')
                        .required(true)
                        .help("data file"),
                )
                .arg(
                    Arg::new("listfile")
                        .short('f')
                        .action(ArgAction::Append)
                        .help("file with one key per line"),
                )
                .arg(
                    Arg::new("key")
                        .short('k')
                        .action(ArgAction::Append)
                        .help("a key to extract (may be repeated)"),
                ),
        )
        .subcommand(
            ClapCommand::new("build")
                .about("build a database from files and/or directories")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("ffdata")
                        .short('d')
                        .required(true)
                        .help("data file"),
                )
                .arg(
                    Arg::new("ffindex")
                        .short('i')
                        .required(true)
                        .help("index file"),
                )
                .arg(
                    Arg::new("append")
                        .short('a')
                        .action(ArgAction::SetTrue)
                        .help("append to an existing database"),
                )
                .arg(
                    Arg::new("sort")
                        .short('s')
                        .action(ArgAction::SetTrue)
                        .help("sort the index after building"),
                )
                .arg(
                    Arg::new("key")
                        .short('k')
                        .long("key")
                        .value_parser(["basename", "sequential"])
                        .default_value("basename")
                        .help("entry key source"),
                )
                .arg(
                    Arg::new("listfile")
                        .short('f')
                        .action(ArgAction::Append)
                        .help("file listing input paths, one per line"),
                )
                .arg(
                    Arg::new("paths")
                        .action(ArgAction::Append)
                        .help("files or directories to add"),
                ),
        )
        .subcommand(
            ClapCommand::new("from_fasta")
                .about("build a database from a FASTA file (one entry per record)")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("ffdata")
                        .short('d')
                        .required(true)
                        .help("data file"),
                )
                .arg(
                    Arg::new("ffindex")
                        .short('i')
                        .required(true)
                        .help("index file"),
                )
                .arg(
                    Arg::new("sort")
                        .short('s')
                        .action(ArgAction::SetTrue)
                        .help("sort the index after building"),
                )
                .arg(
                    Arg::new("key")
                        .short('k')
                        .long("key")
                        .value_parser(["header", "sequential"])
                        .default_value("sequential")
                        .help("entry key source"),
                )
                .arg(Arg::new("fasta").required(true).help("input FASTA file")),
        )
        .subcommand(
            ClapCommand::new("modify")
                .about("sort an index and/or unlink entries from it")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("ffindex")
                        .short('i')
                        .required(true)
                        .help("index file"),
                )
                .arg(
                    Arg::new("sort")
                        .short('s')
                        .action(ArgAction::SetTrue)
                        .help("sort the index"),
                )
                .arg(
                    Arg::new("unlink")
                        .short('u')
                        .action(ArgAction::SetTrue)
                        .help("remove the given keys from the index"),
                )
                .arg(
                    Arg::new("key")
                        .short('k')
                        .action(ArgAction::Append)
                        .help("a key to unlink (may be repeated)"),
                )
                .arg(
                    Arg::new("listfile")
                        .short('f')
                        .action(ArgAction::Append)
                        .help("file with one key to unlink per line"),
                ),
        )
        .subcommand(
            ClapCommand::new("apply")
                .about("run a program for each entry, feeding its data on stdin")
                .arg_required_else_help(true)
                .arg(
                    Arg::new("ffdata")
                        .short('d')
                        .required(true)
                        .help("data file"),
                )
                .arg(
                    Arg::new("ffindex")
                        .short('i')
                        .required(true)
                        .help("index file"),
                )
                .arg(
                    Arg::new("out_ffdata")
                        .short('D')
                        .requires("out_ffindex")
                        .help("output data file (capture program stdout)"),
                )
                .arg(
                    Arg::new("out_ffindex")
                        .short('I')
                        .requires("out_ffdata")
                        .help("output index file (capture program stdout)"),
                )
                .arg(
                    Arg::new("program")
                        .required(true)
                        .num_args(1..)
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true)
                        .help("program and arguments to run per entry"),
                ),
        );

    let matches = app.get_matches();

    match matches.subcommand()
    {
        Some(("get", sub)) => {
            let keys = collect_keys(sub);
            ffindex_get(
                sub.get_one::<String>("ffindex").expect("ffindex").to_string(),
                sub.get_one::<String>("ffdata").expect("ffdata").to_string(),
                keys,
            );
        }
        Some(("build", sub)) => {
            let key_mode = match sub.get_one::<String>("key").map(String::as_str)
            {
                Some("sequential") => KeyMode::Sequential,
                _ => KeyMode::Basename,
            };
            let mut inputs: Vec<PathBuf> = sub
                .get_many::<String>("listfile")
                .map(|files| {
                    files
                        .flat_map(|listfile| get_keys_from_file(listfile.to_string()))
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default();
            if let Some(paths) = sub.get_many::<String>("paths")
            {
                inputs.extend(paths.map(PathBuf::from));
            }
            ffindex_build(
                sub.get_one::<String>("ffdata").expect("ffdata").to_string(),
                sub.get_one::<String>("ffindex").expect("ffindex").to_string(),
                sub.get_flag("append"),
                sub.get_flag("sort"),
                key_mode,
                inputs,
            );
        }
        Some(("from_fasta", sub)) => {
            let key_mode = match sub.get_one::<String>("key").map(String::as_str)
            {
                Some("header") => KeyMode::Header,
                _ => KeyMode::Sequential,
            };
            ffindex_from_fasta(
                sub.get_one::<String>("ffdata").expect("ffdata").to_string(),
                sub.get_one::<String>("ffindex").expect("ffindex").to_string(),
                sub.get_flag("sort"),
                key_mode,
                sub.get_one::<String>("fasta").expect("fasta").to_string(),
            );
        }
        Some(("modify", sub)) => {
            let keys = collect_keys(sub);
            ffindex_modify(
                sub.get_one::<String>("ffindex").expect("ffindex").to_string(),
                sub.get_flag("sort"),
                sub.get_flag("unlink"),
                keys,
            );
        }
        Some(("apply", sub)) => {
            let program: Vec<String> = sub
                .get_many::<String>("program")
                .expect("program")
                .map(|s| s.to_string())
                .collect();
            ffindex_apply(
                sub.get_one::<String>("ffdata").expect("ffdata").to_string(),
                sub.get_one::<String>("ffindex").expect("ffindex").to_string(),
                program,
                sub.get_one::<String>("out_ffdata").map(|s| s.to_string()),
                sub.get_one::<String>("out_ffindex").map(|s| s.to_string()),
            );
        }
        _ => unreachable!("arg_required_else_help guarantees a subcommand"),
    };

    Ok(())
}
