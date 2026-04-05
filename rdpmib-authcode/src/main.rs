use std::env::args;
use std::process;

use rdpmib_authcode::get_authcode;
use rdpmib_authcode::GetAuthcodeError;

fn main() -> Result<(), GetAuthcodeError> {
    let Some(url) = args().nth(1) else {
        eprintln!("no url");
        process::exit(-1);
    };

    let code = get_authcode(&url)?;
    println!("{code}");

    Ok(())
}
