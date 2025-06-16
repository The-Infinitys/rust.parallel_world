// src/parallel_worlds.rs

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::world::{World, WorldStatus}; // WorldとWorldStatusをインポート

/// ParalellWorldsは複数のタスクを同時並行で行うための構造体です。
pub struct ParallelWorlds {
    // WorldをID（String）で管理するHashMap
    worlds: Mutex<HashMap<String, Arc<World>>>,
    // 内部的にスレッドプールを使用するかどうかは将来の拡張性として考慮できます。
    // 現状は各Worldが独立したスレッドを持つと想定します。
}

impl ParallelWorlds {
    /// 空のParalellWorldsを生成します。
    pub fn new() -> Self {
        ParallelWorlds {
            worlds: Mutex::new(HashMap::new()),
        }
    }

    /// ParalellWorldsにWorldを追加します。
    /// 同じIDのWorldが既に存在する場合はエラーを返します。
    pub fn add(&self, id: String, world: World) -> Result<(), String> {
        let mut worlds_guard = self.worlds.lock().unwrap();
        if worlds_guard.contains_key(&id) {
            return Err(format!("World with ID '{}' already exists.", id));
        }
        worlds_guard.insert(id, Arc::new(world));
        Ok(())
    }

    /// ParalellWorldsからWorldを削除します。
    /// 存在しないIDの場合はエラーを返します。
    /// 実行中のWorldを削除しようとした場合もエラーを返します。
    pub fn del(&self, id: &str) -> Result<(), String> {
        let mut worlds_guard = self.worlds.lock().unwrap();
        if let Some(world) = worlds_guard.get(id) {
            if world.progress() == WorldStatus::Running {
                return Err(format!("Cannot delete running World with ID '{}'. Stop it first.", id));
            }
            worlds_guard.remove(id);
            Ok(())
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// ParalellWorldsに保存されたWorldのIDのリストを取得します。
    pub fn list(&self) -> Vec<String> {
        self.worlds.lock().unwrap().keys().cloned().collect()
    }

    /// ParalellWorldsからWorldを検索し、Arc参照を返します。
    pub fn get(&self, id: &str) -> Option<Arc<World>> {
        self.worlds.lock().unwrap().get(id).cloned()
    }

    // --- 処理用のメソッド ---

    /// すべてのReady状態のWorldを一気に実行開始します。
    /// 各Worldの`start()`メソッドを呼び出します。
    pub fn start_all(&self) {
        let worlds_guard = self.worlds.lock().unwrap();
        for (_, world) in worlds_guard.iter() {
            if world.progress() == WorldStatus::Ready {
                let _ = world.start(); // エラーは無視（個々のWorldのログで対応）
            }
        }
    }

    /// 特定のWorldを実行開始します。
    ///
    /// # Errors
    /// Worldが見つからない、または既に実行中の場合にエラーを返します。
    pub fn exec(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            world.start()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// すべての実行中のWorldを停止します。
    pub fn stop_all(&self) {
        let worlds_guard = self.worlds.lock().unwrap();
        for (_, world) in worlds_guard.iter() {
            if world.progress() == WorldStatus::Running {
                let _ = world.stop(); // エラーは無視
            }
        }
    }

    /// 特定のWorldを停止します。
    ///
    /// # Errors
    /// Worldが見つからない、または実行中でない場合にエラーを返します。
    pub fn kill(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            // killはstopよりも強制的な停止を意図するかもしれませんが、
            // std::threadの制約から、ここではstopと同じ処理とします。
            // 実際の強制終了は、より高度なプロセス管理（OSレベル）が必要になるでしょう。
            world.stop()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 指定されたWorldの実行状態を取得します。
    ///
    /// # Errors
    /// Worldが見つからない場合にエラーを返します。
    pub fn progress(&self, id: &str) -> Result<WorldStatus, String> {
        if let Some(world) = self.get(id) {
            Ok(world.progress())
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 指定されたWorldの実行終了を待機し、結果を返します。
    ///
    /// # Errors
    /// Worldが見つからない場合にエラーを返します。
    pub fn status(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            world.status()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }
}

impl Default for ParallelWorlds {
    fn default() -> Self {
        Self::new()
    }
}