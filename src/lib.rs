use memmap::MmapOptions;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};

extern crate csv;
use csv::ReaderBuilder;

#[macro_use]
extern crate serde;

pub fn mmap_file(filepath: String) -> memmap::Mmap
{
    let file = File::open(filepath).expect("file problem");
    unsafe { MmapOptions::new().map(&file) }.expect("mmaping failed")
}

#[derive(Debug, Deserialize)]
pub struct FFindexEntry
{
    name: String,
    offset: usize,
    length: usize,
}

impl FFindexEntry
{
    pub fn name(&self) -> &str
    {
        &self.name
    }
    pub fn offset(&self) -> usize
    {
        self.offset
    }
    pub fn length(&self) -> usize
    {
        self.length
    }
}

pub struct FFindexDB
{
    pub ffindex_path: String,
    pub ffdata_path: String,
    entries: Vec<FFindexEntry>,
    ffdata: memmap::Mmap,
}

impl FFindexDB
{
    /// the parsed index entries
    pub fn entries(&self) -> &[FFindexEntry]
    {
        &self.entries
    }
}

/// load the index, assumed to be sorted
pub fn load_index(ffindex_index_path: String) -> Vec<FFindexEntry>
{
    let mut rdr = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .flexible(false)
        .from_path(ffindex_index_path)
        .expect("reader");

    rdr.deserialize()
        .collect::<Result<Vec<FFindexEntry>, csv::Error>>()
        .expect("Reading ffindex")
}

/// open index and data file
pub fn ffindex_db_open(ffindex_path: String, ffdata_path: String) -> FFindexDB
{
    FFindexDB {
        ffindex_path: ffindex_path.clone(),
        ffdata_path: ffdata_path.clone(),
        entries: load_index(ffindex_path),
        ffdata: mmap_file(ffdata_path),
    }
}

/// get an index entry by index
pub fn ffindex_get_entry_by_index<'a>(
    ffindex_db: &'a FFindexDB,
    index: usize,
) -> Option<&'a FFindexEntry>
{
    ffindex_db.entries.get(index)
}

/// get an index entry by name using binary search
pub fn ffindex_get_entry_by_name<'a>(
    ffindex_db: &'a FFindexDB,
    key: String,
) -> Result<&'a FFindexEntry, usize>
{
    match ffindex_db
        .entries
        .binary_search_by(|entry| entry.name.cmp(&key))
    {
        Ok(i) => Ok(&ffindex_db.entries[i]),
        Err(i) => Err(i),
    }
}

/// get the payload for an entry (without the trailing '\0' C-string terminator)
pub fn ffindex_get_data_by_entry<'a>(
    ffindex_db: &'a FFindexDB,
    entry: &FFindexEntry,
) -> Option<&'a [u8]>
{
    ffindex_db
        .ffdata
        .get(entry.offset..entry.offset + entry.length.saturating_sub(1))
}

/// get the data associated with an index position
pub fn ffindex_get_data_by_index<'a>(ffindex_db: &'a FFindexDB, index: usize) -> Option<&'a [u8]>
{
    match ffindex_get_entry_by_index(ffindex_db, index)
    {
        Some(entry) => ffindex_get_data_by_entry(ffindex_db, entry),
        None => None,
    }
}

/// get the data associated with a name
pub fn ffindex_get_data_by_name<'a>(ffindex_db: &'a FFindexDB, key: String) -> Option<&'a [u8]>
{
    match ffindex_get_entry_by_name(ffindex_db, key)
    {
        Ok(entry) => ffindex_get_data_by_entry(ffindex_db, entry),
        Err(_index) => None,
    }
}

/// Writer for building an ffindex database.
///
/// Each inserted entry is written to the data file followed by a single
/// `\0` separator; the stored length therefore includes that terminator,
/// matching the original C FFindex on-disk format.
pub struct FFindexWriter
{
    data: BufWriter<File>,
    index: BufWriter<File>,
    offset: usize,
}

impl FFindexWriter
{
    /// Create (or, when `append` is true, open for appending) a data/index pair.
    pub fn create(ffdata_path: &str, ffindex_path: &str, append: bool)
    -> io::Result<FFindexWriter>
    {
        let data_file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(ffdata_path)?;
        let index_file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(ffindex_path)?;

        let offset = if append
        {
            data_file.metadata()?.len() as usize
        }
        else
        {
            0
        };

        Ok(FFindexWriter {
            data: BufWriter::new(data_file),
            index: BufWriter::new(index_file),
            offset,
        })
    }

    /// Append a single entry named `name` with payload `data`.
    pub fn insert(&mut self, name: &str, data: &[u8]) -> io::Result<()>
    {
        self.data.write_all(data)?;
        // Separate entries by '\0' and make sure at least one byte is written.
        self.data.write_all(b"\0")?;
        let length = data.len() + 1;
        writeln!(self.index, "{}\t{}\t{}", name, self.offset, length)?;
        self.offset += length;
        Ok(())
    }

    /// Flush both files.
    pub fn finish(mut self) -> io::Result<()>
    {
        self.data.flush()?;
        self.index.flush()?;
        Ok(())
    }
}

/// Sort an index file in place, ordering entries lexicographically by name.
pub fn sort_index_file(ffindex_path: &str) -> io::Result<()>
{
    let mut entries = load_index(ffindex_path.to_string());
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let mut writer = BufWriter::new(File::create(ffindex_path)?);
    for entry in &entries
    {
        writeln!(writer, "{}\t{}\t{}", entry.name, entry.offset, entry.length)?;
    }
    writer.flush()
}

#[cfg(test)]
mod tests
{
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> std::path::PathBuf
    {
        let mut p = std::env::temp_dir();
        p.push(format!("ffindexrs_test_{}_{}", std::process::id(), name));
        p
    }

    #[test]
    fn it_works()
    {
        let filepath = "/etc/passwd";
        let ffdata = mmap_file(filepath.to_string());
        println!("{:?}", ffdata.get(0..4));
    }

    #[test]
    fn build_and_read_roundtrip()
    {
        let data_path = tmp("rt.ffdata");
        let index_path = tmp("rt.ffindex");
        let data_s = data_path.to_string_lossy().to_string();
        let index_s = index_path.to_string_lossy().to_string();

        {
            let mut w = FFindexWriter::create(&data_s, &index_s, false).unwrap();
            // insert out of order to exercise sorting
            w.insert("banana", b"yellow\n").unwrap();
            w.insert("apple", b"red\n").unwrap();
            w.finish().unwrap();
        }
        sort_index_file(&index_s).unwrap();

        let db = ffindex_db_open(index_s.clone(), data_s.clone());
        assert_eq!(db.entries().len(), 2);
        // sorted: apple before banana
        assert_eq!(db.entries()[0].name(), "apple");

        let apple = ffindex_get_data_by_name(&db, "apple".to_string()).unwrap();
        assert_eq!(apple, b"red\n"); // trailing '\0' separator is stripped on read
        let banana = ffindex_get_data_by_name(&db, "banana".to_string()).unwrap();
        assert_eq!(banana, b"yellow\n");
        assert!(ffindex_get_data_by_name(&db, "cherry".to_string()).is_none());

        let _ = std::fs::remove_file(&data_path);
        let _ = std::fs::remove_file(&index_path);
    }

    #[test]
    fn append_extends_offsets()
    {
        let data_path = tmp("ap.ffdata");
        let index_path = tmp("ap.ffindex");
        let data_s = data_path.to_string_lossy().to_string();
        let index_s = index_path.to_string_lossy().to_string();

        {
            let mut w = FFindexWriter::create(&data_s, &index_s, false).unwrap();
            w.insert("a", b"one").unwrap();
            w.finish().unwrap();
        }
        {
            let mut w = FFindexWriter::create(&data_s, &index_s, true).unwrap();
            w.insert("b", b"two").unwrap();
            w.finish().unwrap();
        }

        let db = ffindex_db_open(index_s.clone(), data_s.clone());
        assert_eq!(db.entries().len(), 2);
        let b = ffindex_get_data_by_name(&db, "b".to_string()).unwrap();
        assert_eq!(b, b"two");
        // "a" is "one" + '\0' = 4 bytes, so "b" must start at offset 4
        assert_eq!(db.entries()[1].offset(), 4);

        // make sure removing it doesn't panic if a later test reuses the path
        drop(db);
        write!(std::io::sink(), "").unwrap();

        let _ = std::fs::remove_file(&data_path);
        let _ = std::fs::remove_file(&index_path);
    }
}
