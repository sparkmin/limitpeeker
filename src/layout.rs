/// What the controller asks the hook to show, and what the hook reports shown.
/// The number is what crosses shared memory. The five-rung ladder that shrank
/// the label into free space is gone: the hook reserves the space instead.
/// The label itself is sent separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum DisplayMode {
    TrayOnly = 0,
    Full = 4,
}

impl DisplayMode {
    /// The one place a number read back from shared memory becomes a mode.
    pub const fn from_word(value: u32) -> Self {
        match value {
            4 => Self::Full,
            _ => Self::TrayOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_memory_numbers_round_trip() {
        for mode in [DisplayMode::TrayOnly, DisplayMode::Full] {
            assert_eq!(DisplayMode::from_word(mode as u32), mode);
        }
        // A word from an older ladder build is not mistaken for a label.
        for stale in [1, 2, 3, 5] {
            assert_eq!(DisplayMode::from_word(stale), DisplayMode::TrayOnly);
        }
    }
}
