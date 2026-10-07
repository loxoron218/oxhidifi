//! Application-level utilities including XDG base directory resolution and
//! Libadwaita `AdwApplication` setup.

pub mod bootstrap;
pub mod lifecycle;
pub mod runtime;
pub mod watch_loop;
pub mod xdg_paths;

#[cfg(test)]
pub mod mocks;
