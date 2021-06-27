use std::io;
use std::io::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let str = line.unwrap();
        let jd = &mut serde_json::Deserializer::from_str(str.as_str());
        let result: Result<ichiran::types::Root, _> = serde_path_to_error::deserialize(jd);
        match result {
            Ok(root) => println!("{:?}", root),
            Err(err) => println!("{}", err),
        }
    }
    Ok(())
}
