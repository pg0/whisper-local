pub mod state;
pub mod hook;
pub use state::{HotkeyEvent, KeyEvent, Machine, VKey};
pub use hook::{spawn_hook, force_latch, force_idle};
