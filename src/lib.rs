use memmap::MmapOptions;
use std::fs::File;

extern crate csv;
use csv::{ReaderBuilder};

#[macro_use]
extern crate serde;


pub fn mmap_file(filepath : String) -> memmap::Mmap
{
    let file = File::open(filepath).expect("file problem");
    unsafe { MmapOptions::new().map(&file) }.expect("mmaping failed")
}


#[derive(Debug,Deserialize)]
pub struct FFindexEntry
{
    name: String,
    offset: usize,
    length: usize
}


pub struct FFindexDB
{
    ffindex_path : String,
    ffdata_path : String,
    entries : Vec<FFindexEntry>,
    ffdata : memmap::Mmap,
}


/// load the index, assumed to be sorted
pub fn load_index(ffindex_index_path:String) -> Vec<FFindexEntry>
{
    let mut rdr = ReaderBuilder::new()
                    .has_headers(false)
                    .delimiter(b'\t')
                    .from_path(ffindex_index_path)
                    .expect("reader");

    rdr.deserialize().collect::<Result<Vec<FFindexEntry>, csv::Error>>().expect("Reading ffindex")
}


/// open index and data file
pub fn ffindex_db_open(ffindex_path : String, ffdata_path : String) -> FFindexDB
{
    FFindexDB
    {
        ffindex_path : ffindex_path.clone(),
        ffdata_path : ffdata_path.clone(),
        entries : load_index(ffindex_path),
        ffdata : mmap_file(ffdata_path),
    }
}


/// get an index entry by name using binary search
pub fn ffindex_get_entry_by_name<'a>(ffindex_db : &'a FFindexDB, key : String) -> Result<&'a FFindexEntry,usize>
{
    match ffindex_db.entries.binary_search_by(|entry| entry.name.cmp(&key))
    {
        Ok(i) => Ok(&ffindex_db.entries[i]),
        Err(i) => Err(i)
    }
}


/// get the data associated with a name
pub fn ffindex_get_data_by_name<'a>(ffindex_db : &'a FFindexDB, key : String) ->Option<&'a [u8]>
{
    match ffindex_get_entry_by_name(ffindex_db, key)
    {
        Ok(entry) => ffindex_db.ffdata.get(entry.offset .. entry.offset + entry.length),
        Err(_index) => None
    }
}

#[cfg(test)]
mod tests {
    use crate::mmap_file;

    #[test]
    fn it_works() {
        let filepath = "/etc/passwd";
        let ffdata = mmap_file(filepath.to_string());
        println!("{:?}", ffdata.get(0..4));
    }
}
