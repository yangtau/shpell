use anyhow::{bail, Result};

pub fn init_script(shell: &str) -> Result<&'static str> {
    match shell {
        "zsh" => Ok(include_str!("shpell.zsh")),
        "bash" => Ok(include_str!("shpell.bash")),
        other => bail!("unsupported shell {other:?} (supported: zsh, bash)"),
    }
}
