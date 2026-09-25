//! Writes a plugin and sleeve composition after verifying its routing.

#![forbid(unsafe_code)]

use std::error::Error;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let [plugin, sleeve, output] = paths(std::env::args_os().skip(1))?;
    let plugin = std::fs::read(plugin)?;
    let sleeve = std::fs::read(sleeve)?;
    let composed = sleeve_host::compose(&plugin, &sleeve, sleeve_host::sleeve_sha256(&sleeve))?;
    std::fs::write(PathBuf::from(output), composed)?;
    Ok(())
}

fn paths(args: impl Iterator<Item = OsString>) -> io::Result<[OsString; 3]> {
    args.collect::<Vec<_>>().try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: compose-component <plugin.wasm> <sleeve.wasm> <output.wasm>",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::paths;

    #[test]
    fn requires_one_plugin_sleeve_and_output() {
        assert!(paths(["plugin", "sleeve", "output"].into_iter().map(Into::into)).is_ok());
        assert!(paths(["plugin", "sleeve"].into_iter().map(Into::into)).is_err());
    }
}
