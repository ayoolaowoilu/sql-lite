use crate::utils::read_write_fn::read_db;


mod utils;


fn main() {
    println!("Hello, world!");
    read_db("default");
}
