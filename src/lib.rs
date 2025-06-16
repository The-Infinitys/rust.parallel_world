pub mod parallel_worlds;
pub mod world;

// クレートのトップレベルで利用できるように、use宣言を追加
pub use parallel_worlds::ParallelWorlds;
pub use world::{AnyWorld, World, WorldStatus}; // AnyWorldを追加
