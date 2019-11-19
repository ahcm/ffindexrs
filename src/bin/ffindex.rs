

use std::io;

extern crate clap; 
use clap::{Arg, App, SubCommand, AppSettings};

extern crate csv;
use csv::{ReaderBuilder};

use ffindexrs::ffindex_db_open;
use ffindexrs::ffindex_get_data_by_name;

/// get the keys from the listfile
pub fn get_keys_from_file(path:String) -> Vec<String>
{
    let mut rdr = ReaderBuilder::new()
                    .has_headers(false)
                    .delimiter(b'\t')
                    .from_path(path)
                    .expect("listfile reader");

    rdr.deserialize().collect::<Result<Vec<String>, csv::Error>>().expect("Reading listfile")
}

fn ffindex_get(ffindex_path:String, ffdata_path:String, keys:Vec<String>)
{
    let ffindex_db = ffindex_db_open(ffindex_path, ffdata_path);
    for key in keys
    {
      match ffindex_get_data_by_name(&ffindex_db, key)
      {
          Some(data) => print!("{}", std::str::from_utf8(data).unwrap()),
          None => println!("Not found")
      }
    }
}


fn main() -> io::Result<()>
{
  let app = App::new("ffindex")
    .version("0.1")
    .about("FFindex flat file database")
    .author("Andreas Hauser")
    .setting(AppSettings::ArgRequiredElseHelp)
    .subcommand(SubCommand::with_name("test").about("controls testing features"))
    .subcommand(SubCommand::with_name("get")
                .about("ffindex get - extract records")
                .setting(AppSettings::ArgRequiredElseHelp)
                .arg(Arg::with_name("ffindex").short("i").takes_value(true).required(true))
                .arg(Arg::with_name("ffdata").short("d").takes_value(true).required(true))
                .arg(Arg::with_name("listfile").short("f").takes_value(true))
                .arg(Arg::with_name("key").short("k").takes_value(true).multiple(true)))
  ;
  let matches = app.get_matches(); 

  match matches.subcommand()
  {
      ("test",  Some(arg)) =>  { println!("{:?}", arg) },
      ("get",   Some(_arg)) =>  {
          if let Some(submatches) = matches.subcommand_matches("get")
          {
              let mut keys : Vec<String> = submatches.values_of("listfile").expect("listfile")
                  .flat_map(|listfile| get_keys_from_file(listfile.to_string()) ).collect();

              if submatches.is_present("key")
              { 
                  for key in submatches.values_of("key").expect("key") { keys.push(key.to_string()); }
              }

              ffindex_get(submatches.value_of("ffindex").expect("ffindex").to_string(),
                          submatches.value_of("ffdata").expect("ffdata").to_string(),
                          keys)
          }
      },
      other => {println!("Command: {:?}", other)}
  };

  Ok(())
}
