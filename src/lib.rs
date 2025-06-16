pub mod world;
pub mod parallel_worlds;

// クレートのトップレベルで利用できるように、use宣言を追加
pub use world::{World, WorldStatus, AnyWorld}; // AnyWorldを追加
pub use parallel_worlds::ParallelWorlds;