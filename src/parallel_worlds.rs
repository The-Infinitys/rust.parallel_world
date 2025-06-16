// src/parallel_worlds.rs

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::world::{World, WorldStatus}; // WorldとWorldStatusをインポート

/// # ParallelWorlds
///
/// `ParallelWorlds` は、複数の独立したタスク（`World`インスタンス）を管理し、
/// それらを並行して実行するための主要な構造体です。
///
/// Pythonの`threading`モジュールのように、複数のタスクの開始、停止、状態監視を一元的に行えます。
/// 各`World`は内部的に個別のスレッドで実行されます。
pub struct ParallelWorlds {
    /// ID（文字列）をキーとし、`World`インスタンスへの共有参照（`Arc<World>`）を値とする
    /// ハッシュマップ。`Mutex`によってスレッドセーフにアクセスが保護されます。
    worlds: Mutex<HashMap<String, Arc<World>>>,
    // 将来的にスレッドプールを導入する可能性もありますが、現状では各Worldが独立したスレッドを持ちます。
}

impl ParallelWorlds {
    /// 新しい空の `ParallelWorlds` インスタンスを生成します。
    ///
    /// 最初はどのWorldも含まれていません。`add`メソッドを使用してWorldを追加できます。
    ///
    /// # 例
    /// ```
    /// use parallel_world::ParallelWorlds;
    ///
    /// let pw = ParallelWorlds::new();
    /// assert!(pw.list().is_empty());
    /// ```
    pub fn new() -> Self {
        ParallelWorlds {
            worlds: Mutex::new(HashMap::new()),
        }
    }

    /// `ParallelWorlds` に新しい `World` を追加します。
    ///
    /// 指定された `id` が既に存在する場合、`Err` を返します。
    ///
    /// # 引数
    /// * `id` - 追加するWorldの一意な識別子（String）。
    /// * `world` - 追加する `World` インスタンス。
    ///
    /// # 戻り値
    /// `Ok(())` - Worldが正常に追加された場合。
    /// `Err(String)` - 同じIDのWorldが既に存在する場合、または内部エラーが発生した場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World};
    ///
    /// let pw = ParallelWorlds::new();
    /// let world1 = World::from(|| println!("Task 1"));
    /// let world2 = World::from(|| println!("Task 2"));
    ///
    /// assert!(pw.add("task_one".to_string(), world1).is_ok());
    /// assert!(pw.add("task_two".to_string(), world2).is_ok());
    ///
    /// // 同じIDを追加しようとするとエラー
    /// let world_duplicate = World::new();
    /// assert!(pw.add("task_one".to_string(), world_duplicate).is_err());
    /// ```
    pub fn add(&self, id: String, world: World) -> Result<(), String> {
        let mut worlds_guard = self.worlds.lock().unwrap();
        if worlds_guard.contains_key(&id) {
            return Err(format!("World with ID '{}' already exists.", id));
        }
        worlds_guard.insert(id, Arc::new(world));
        Ok(())
    }

    /// `ParallelWorlds` から指定されたIDの `World` を削除します。
    ///
    /// Worldが実行中の場合、削除することはできません。まず `stop_all` または `kill` メソッドで
    /// 停止させる必要があります。
    ///
    /// # 引数
    /// * `id` - 削除するWorldの識別子。
    ///
    /// # 戻り値
    /// `Ok(())` - Worldが正常に削除された場合。
    /// `Err(String)` - 指定されたIDのWorldが見つからない場合、またはWorldが実行中の場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    /// let world1 = World::from(|| { sleep(Duration::from_millis(100)); println!("Task 1 finished."); });
    /// pw.add("task_one".to_string(), world1).unwrap();
    ///
    /// // 実行中のWorldは削除できない
    /// pw.exec("task_one").unwrap();
    /// assert!(pw.del("task_one").is_err());
    ///
    /// // 停止または完了後に削除できる
    /// pw.status("task_one").unwrap(); // 完了を待つ
    /// assert!(pw.del("task_one").is_ok());
    /// assert!(pw.list().is_empty());
    ///
    /// // 存在しないWorldの削除はエラー
    /// assert!(pw.del("non_existent_task").is_err());
    /// ```
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

    /// `ParallelWorlds` に現在登録されているすべての `World` のIDリストを取得します。
    ///
    /// # 戻り値
    /// 現在登録されているWorldのID（`String`）のベクタ。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World};
    ///
    /// let pw = ParallelWorlds::new();
    /// pw.add("alpha".to_string(), World::new()).unwrap();
    /// pw.add("beta".to_string(), World::new()).unwrap();
    ///
    /// let mut ids = pw.list();
    /// ids.sort(); // 順序を保証するためにソート
    /// assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
    /// ```
    pub fn list(&self) -> Vec<String> {
        self.worlds.lock().unwrap().keys().cloned().collect()
    }

    /// 指定されたIDの `World` への共有参照（`Arc<World>`）を取得します。
    ///
    /// これにより、個々のWorldの状態を直接照会したり、操作したりできます。
    ///
    /// # 引数
    /// * `id` - 取得するWorldの識別子。
    ///
    /// # 戻り値
    /// `Some(Arc<World>)` - 指定されたIDのWorldが見つかった場合。
    /// `None` - 指定されたIDのWorldが見つからない場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World};
    ///
    /// let pw = ParallelWorlds::new();
    /// let world1 = World::new();
    /// pw.add("my_world".to_string(), world1).unwrap();
    ///
    /// let retrieved_world = pw.get("my_world").unwrap();
    /// // retrieved_world を介して World のメソッドを呼び出すことができる
    /// assert_eq!(retrieved_world.progress().to_string(), "Ready");
    ///
    /// assert!(pw.get("non_existent").is_none());
    /// ```
    pub fn get(&self, id: &str) -> Option<Arc<World>> {
        self.worlds.lock().unwrap().get(id).cloned()
    }


    /// 登録されているすべての `World` のうち、状態が `Ready` のものを一括で実行開始します。
    ///
    /// 各Worldの`start()`メソッドを呼び出しますが、個々のWorldで発生した開始エラーは無視されます。
    /// それぞれのWorldのログや`progress()`メソッドで状態を確認してください。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    /// pw.add("task_a".to_string(), World::from(|| { println!("Task A running..."); sleep(Duration::from_millis(50)); })).unwrap();
    /// pw.add("task_b".to_string(), World::from(|| { println!("Task B running..."); sleep(Duration::from_millis(100)); })).unwrap();
    ///
    /// pw.start_all();
    ///
    /// // 少し待って、両方のタスクが実行中になっていることを確認
    /// sleep(Duration::from_millis(10));
    /// assert_eq!(pw.progress("task_a").unwrap(), WorldStatus::Running);
    /// assert_eq!(pw.progress("task_b").unwrap(), WorldStatus::Running);
    ///
    /// pw.status("task_a").unwrap(); // 完了を待つ
    /// pw.status("task_b").unwrap(); // 完了を待つ
    /// assert_eq!(pw.progress("task_a").unwrap(), WorldStatus::Finished);
    /// assert_eq!(pw.progress("task_b").unwrap(), WorldStatus::Finished);
    /// ```
    pub fn start_all(&self) {
        let worlds_guard = self.worlds.lock().unwrap();
        for (_, world) in worlds_guard.iter() {
            if world.progress() == WorldStatus::Ready {
                let _ = world.start(); // エラーは無視（個々のWorldのログで対応）
            }
        }
    }

    /// 指定されたIDの `World` を実行開始します。
    ///
    /// このメソッドは新しいスレッドを生成し、すぐに制御を返します。
    /// `World` の実行完了を待つには、`status` メソッドを使用します。
    ///
    /// # 引数
    /// * `id` - 実行開始するWorldの識別子。
    ///
    /// # 戻り値
    /// `Ok(())` - Worldが正常に実行開始された場合。
    /// `Err(String)` - 指定されたIDのWorldが見つからない場合、またはWorldが既に実行中の場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    /// pw.add("my_task".to_string(), World::from(|| { println!("Task running..."); sleep(Duration::from_millis(50)); })).unwrap();
    ///
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Ready);
    ///
    /// assert!(pw.exec("my_task").is_ok());
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Running);
    ///
    /// // 既に実行中のWorldを実行開始しようとするとエラー
    /// assert!(pw.exec("my_task").is_err());
    ///
    /// pw.status("my_task").unwrap(); // 完了を待つ
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Finished);
    /// ```
    pub fn exec(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            world.start()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 登録されているすべての実行中の `World` を停止しようと試みます。
    ///
    /// このメソッドは、各Worldの`stop()`メソッドを呼び出します。
    /// `stop()`メソッドはスレッドを強制終了するものではなく、Worldが協力して停止するように
    /// シグナルを送るベストエフォートな試みであることに注意してください。
    /// Worldが実際に停止したことを確認するには、`progress()`メソッドで状態を監視してください。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    /// pw.add("long_task_1".to_string(), World::from(|| {
    ///     for i in 0..10 { sleep(Duration::from_millis(100)); println!("Long task 1: {}", i); }
    /// })).unwrap();
    /// pw.add("long_task_2".to_string(), World::from(|| {
    ///     for i in 0..10 { sleep(Duration::from_millis(150)); println!("Long task 2: {}", i); }
    /// })).unwrap();
    ///
    /// pw.start_all();
    /// sleep(Duration::from_millis(200)); // 少し実行させる
    ///
    /// println!("Attempting to stop all worlds...");
    /// pw.stop_all();
    ///
    /// // 停止の試み後、状態がStoppedになっていることを期待（実際の停止はWorldの実装による）
    /// sleep(Duration::from_millis(50)); // 状態更新を待つ
    /// let s1 = pw.progress("long_task_1").unwrap();
    /// let s2 = pw.progress("long_task_2").unwrap();
    /// assert!(s1 == WorldStatus::Stopped || s1 == WorldStatus::Running);
    /// assert!(s2 == WorldStatus::Stopped || s2 == WorldStatus::Running);
    /// ```
    pub fn stop_all(&self) {
        let worlds_guard = self.worlds.lock().unwrap();
        for (_, world) in worlds_guard.iter() {
            if world.progress() == WorldStatus::Running {
                let _ = world.stop(); // エラーは無視
            }
        }
    }

    /// 指定されたIDの `World` を停止しようと試みます。
    ///
    /// このメソッドは、対象のWorldの`stop()`メソッドを呼び出します。
    /// `stop()`メソッドはスレッドを強制終了するものではなく、Worldが協力して停止するように
    /// シグナルを送るベストエフォートな試みであることに注意してください。
    /// Worldが実際に停止したことを確認するには、`progress()`メソッドで状態を監視してください。
    ///
    /// # 引数
    /// * `id` - 停止するWorldの識別子。
    ///
    /// # 戻り値
    /// `Ok(())` - 停止の試みが正常に行われた場合。
    /// `Err(String)` - 指定されたIDのWorldが見つからない場合、またはWorldが実行中でない場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    /// pw.add("my_long_task".to_string(), World::from(|| {
    ///     for i in 0..10 { sleep(Duration::from_millis(100)); println!("My long task: {}", i); }
    /// })).unwrap();
    ///
    /// pw.exec("my_long_task").unwrap();
    /// sleep(Duration::from_millis(200)); // 少し実行させる
    ///
    /// assert_eq!(pw.progress("my_long_task").unwrap(), WorldStatus::Running);
    ///
    /// println!("Attempting to kill my_long_task...");
    /// assert!(pw.kill("my_long_task").is_ok());
    ///
    /// sleep(Duration::from_millis(50)); // 状態更新を待つ
    /// let status = pw.progress("my_long_task").unwrap();
    /// assert!(status == WorldStatus::Stopped || status == WorldStatus::Running);
    ///
    /// // 存在しないWorldのkillはエラー
    /// assert!(pw.kill("non_existent_task").is_err());
    /// ```
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

    /// 指定されたIDの `World` の現在の実行状態を取得します。
    ///
    /// # 引数
    /// * `id` - 状態を取得するWorldの識別子。
    ///
    /// # 戻り値
    /// `Ok(WorldStatus)` - 指定されたIDのWorldが見つかった場合、その現在の状態を返します。
    /// `Err(String)` - 指定されたIDのWorldが見つからない場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    /// pw.add("my_task".to_string(), World::from(|| sleep(Duration::from_millis(50)))).unwrap();
    ///
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Ready);
    /// pw.exec("my_task").unwrap();
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Running);
    ///
    /// pw.status("my_task").unwrap(); // 完了を待つ
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Finished);
    ///
    /// assert!(pw.progress("non_existent_task").is_err());
    /// ```
    pub fn progress(&self, id: &str) -> Result<WorldStatus, String> {
        if let Some(world) = self.get(id) {
            Ok(world.progress())
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 指定されたIDの `World` の実行終了を待機し、その最終結果を返します。
    ///
    /// このメソッドは、対象のWorldが完了するまで現在のスレッドをブロックします。
    /// パニックなどによりWorldが失敗した場合、`Err`を返します。
    ///
    /// # 引数
    /// * `id` - 実行終了を待機するWorldの識別子。
    ///
    /// # 戻り値
    /// `Ok(())` - Worldが正常に実行を完了した場合。
    /// `Err(String)` - Worldがパニックまたはその他の理由で失敗した場合、
    /// または指定されたIDのWorldが見つからない場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{ParallelWorlds, World};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = ParallelWorlds::new();
    ///
    /// // 成功するタスク
    /// pw.add("success_task".to_string(), World::from(|| {
    ///     println!("Success task running...");
    ///     sleep(Duration::from_millis(50));
    ///     println!("Success task done.");
    /// })).unwrap();
    /// pw.exec("success_task").unwrap();
    /// assert!(pw.status("success_task").is_ok());
    ///
    /// // 失敗するタスク
    /// pw.add("fail_task".to_string(), World::from(|| {
    ///     println!("Fail task running...");
    ///     sleep(Duration::from_millis(10));
    ///     panic!("Oh no!");
    /// })).unwrap();
    /// pw.exec("fail_task").unwrap();
    /// assert!(pw.status("fail_task").is_err());
    ///
    /// // 存在しないWorldのstatusはエラー
    /// assert!(pw.status("non_existent_task").is_err());
    /// ```
    pub fn status(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            world.status()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }
}

impl Default for ParallelWorlds {
    /// `ParallelWorlds::new()` と同じ結果を返します。
    ///
    /// `Default::default()` で `ParallelWorlds` のインスタンスを生成できます。
    ///
    /// # 例
    /// ```
    /// use parallel_world::ParallelWorlds;
    ///
    /// let pw = ParallelWorlds::default();
    /// assert!(pw.list().is_empty());
    /// ```
    fn default() -> Self {
        Self::new()
    }
}