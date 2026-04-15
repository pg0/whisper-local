pub mod state;
pub mod hook;
pub use state::{HotkeyEvent, KeyEvent, Machine, VKey};
pub use hook::spawn_hook;
