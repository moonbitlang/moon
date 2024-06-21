use chrono::DateTime;
use std::{
    error::Error,
    time::{SystemTime, UNIX_EPOCH},
};
use vergen::EmitBuilder;

pub fn main() -> Result<(), Box<dyn Error>> {
    EmitBuilder::builder().build_date().git_sha(true).emit()?;

    let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let datetime = DateTime::from_timestamp(time.as_secs() as i64, 0).unwrap();
    let date_str = datetime.format("%Y%m%d").to_string();
    println!("cargo:rustc-env=CARGO_PKG_VERSION=0.1.{}", date_str);
    Ok(())
}
