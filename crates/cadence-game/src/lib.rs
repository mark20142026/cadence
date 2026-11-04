//! Cadence's game: renderer, assets and the app loop, shared by the native and web builds.

pub mod anim;
pub mod animobj;
pub mod app;
pub mod assets;
pub mod audio;
mod color_z;
mod spin;
mod black;
pub mod files;
pub mod gjapi;
pub mod fx;
pub mod fxhash;
pub mod color_children;
pub mod online;
pub mod particles;
pub mod render;
pub mod ui;
pub mod trail;
#[cfg(target_arch = "wasm32")]
mod web;
