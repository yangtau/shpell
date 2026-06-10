use anyhow::{bail, Result};

pub fn init_script(shell: &str) -> Result<&'static str> {
    match shell {
        "zsh" => Ok(include_str!("x.zsh")),
        other => bail!("unsupported shell {other:?} (supported: zsh)"),
    }
}
