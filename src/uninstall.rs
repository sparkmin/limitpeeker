//! Removes LimitPeeker from this PC, for those who would rather not use the
//! command line: runs `limitpeeker.exe --remove` from the same folder and
//! waits for it. Asking, closing the running product, removing and telling
//! what was done are all that option's, so they are in one place. The
//! folder itself is left for the person to delete, as the option says.

#![windows_subsystem = "windows"]

#[path = "message.rs"]
mod message;

use std::path::{Path, PathBuf};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK};

/// The product, beside this program.
const PRODUCT: &str = "limitpeeker.exe";

fn main() {
    let Some(product) = beside(std::env::current_exe().ok().as_deref()).filter(|p| p.is_file())
    else {
        return tell(&format!(
            "{PRODUCT} was not found next to uninstall.exe, so nothing was removed. Run uninstall.exe from LimitPeeker's folder."
        ));
    };
    match std::process::Command::new(&product)
        .arg("--remove")
        .status()
    {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(e) => tell(&format!(
            "{PRODUCT} could not be started, so nothing was removed: {e}"
        )),
    }
}

/// Where the product is, beside `exe`.
fn beside(exe: Option<&Path>) -> Option<PathBuf> {
    Some(exe?.parent()?.join(PRODUCT))
}

fn tell(text: &str) {
    message::show("Uninstall LimitPeeker", text, MB_OK | MB_ICONWARNING);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_product_is_looked_for_beside_it() {
        // Only beside this program, never on the PATH or in the current
        // folder.
        assert_eq!(
            beside(Some(Path::new(r"C:\Tools\LimitPeeker\uninstall.exe"))),
            Some(PathBuf::from(r"C:\Tools\LimitPeeker\limitpeeker.exe"))
        );
        assert_eq!(beside(None), None);
    }
}
