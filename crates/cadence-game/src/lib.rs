//! Cadence's game: renderer, assets and the app loop, shared by the native and web builds.

pub mod anim;
pub mod animobj;
pub mod app;
pub mod assets;
pub mod audio;
pub mod cloud;
pub mod death;
#[cfg(not(target_arch = "wasm32"))]
pub mod death_check;
mod color_z;
mod spin;
mod black;
pub mod files;
pub mod gjapi;
pub mod fx;
pub mod fxhash;
pub mod color_children;
pub mod net;
pub mod online;
pub mod particles;
pub mod render;
pub mod saved;
pub mod follow;
pub mod mylists;
pub mod social;
pub mod ui;
pub mod trail;
#[cfg(target_arch = "wasm32")]
pub mod web;
