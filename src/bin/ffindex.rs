

use std::io;

extern crate clap; 
use clap::{Arg, App, SubCommand, AppSettings};

use ffindexrs::ffindex_db_open;
use ffindexrs::ffindex_get_data_by_name;


fn ffindex_get(ffindex_path:String, ffdata_path:String, keys:Vec<&str>)
{
    let ffindex_db = ffindex_db_open(ffindex_path, ffdata_path);
    for key in &keys
    {
      match ffindex_get_data_by_name(&ffindex_db, key.to_string())
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
                .arg(Arg::with_name("key").short("k").takes_value(true).required(true).multiple(true)))
  ;
  let matches = app.get_matches(); 

  match matches.subcommand()
  {
      ("test",  Some(arg)) =>  { println!("{:?}", arg) },
      ("get",   Some(_arg)) =>  {
          if let Some(submatches) = matches.subcommand_matches("get") {
              ffindex_get(submatches.value_of("ffindex").expect("ffindex").to_string(),
                          submatches.value_of("ffdata").expect("ffdata").to_string(),
                          submatches.values_of("key").expect("key").collect())
          }
      },
      other => {println!("Command: {:?}", other)}
  };

  Ok(())
}
