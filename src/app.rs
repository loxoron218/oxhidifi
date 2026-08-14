//! Application-level utilities including XDG base directory resolution and
//! Libadwaita `AdwApplication` setup.

pub mod bootstrap;
pub mod lifecycle;
#[cfg(test)]
pub mod mocks;
pub mod runtime;
pub mod xdg_paths;
