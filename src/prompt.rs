use std::io::{self, Write};

use anyhow::Result;

pub fn prompt_line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}
