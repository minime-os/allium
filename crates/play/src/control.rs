#[cfg_attr(not(feature = "simulator"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlEvent {
    SaveState,
    LoadState,
    SelectStateSlot(u8),
}
