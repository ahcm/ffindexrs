# ffindexrs

A Rust implementation of [FFindex](https://github.com/ahcm/ffindex), a very
simple index/database for huge amounts of small files. A database is a pair of
files:

* a **data** file (`*.ffdata`): all entry payloads concatenated, each followed
  by a `\0` separator.
* an **index** file (`*.ffindex`): tab-separated `name\toffset\tlength`, sorted
  by name. `length` includes the trailing separator, matching the original C
  FFindex on-disk format.

## Building

```sh
cargo build --release
```

The binary is produced at `target/release/ffindex`.

## Usage

```
ffindex <SUBCOMMAND>
```

### build — create a database from files/directories

```sh
# add every file in ./docs, then sort the index
ffindex build -d data.ffdata -i data.ffindex -s docs/

# append a single file to an existing database
ffindex build -d data.ffdata -i data.ffindex -a -s extra.txt

# read the list of input paths from a file (one per line)
ffindex build -d data.ffdata -i data.ffindex -s -f paths.txt
```

Each entry is named after the file's base name. Directories are expanded to
their immediate files.

* `-a` append to an existing database
* `-s` sort the index after building
* `-f FILE` a file listing input paths, one per line (repeatable)

### from_fasta — create a database from a FASTA file

```sh
ffindex from_fasta -d seqs.ffdata -i seqs.ffindex -s input.fasta
```

Each record (header line plus its sequence lines) becomes one entry keyed by a
running integer (`1`, `2`, ...).

### get — extract records by key

```sh
ffindex get -d data.ffdata -i data.ffindex -k alpha -k beta
ffindex get -d data.ffdata -i data.ffindex -f keys.tsv
```

* `-k KEY` a key to extract (repeatable)
* `-f FILE` a file with one key per line (extra tab-separated columns ignored)

### modify — sort and/or unlink entries

```sh
ffindex modify -i data.ffindex -s            # sort in place
ffindex modify -i data.ffindex -u -k beta    # remove the "beta" entry
```

Unlinking removes entries from the index only; the data file is left untouched.

* `-s` sort the index
* `-u` unlink the given keys (`-k`/`-f`)

### apply — run a program per entry

```sh
ffindex apply -d data.ffdata -i data.ffindex wc -c
```

For each entry, the program is run with the entry's payload piped to its stdin.

## Library

The crate is also usable as a library (`ffindexrs`) exposing
`ffindex_db_open`, `ffindex_get_data_by_name`, the `FFindexWriter` for building
databases, `sort_index_file`, and related helpers.
