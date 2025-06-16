use crate::world::{AnyWorld, World, WorldStatus};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// # Multiverse
///
/// `Multiverse` は、複数の独立したタスク（`World`インスタンス）を管理し、
/// それらを並行して実行するための主要な構造体です。
///
/// Pythonの`threading`モジュールのように、複数のタスクの開始、停止、状態監視を一元的に行えます。
/// 各`World`は内部的に個別のスレッドで実行されます。
pub struct Multiverse {
    /// WorldをID（String）で管理するHashMap。
    /// 異なる戻り値の型を持つWorldを管理するため、`AnyWorld`トレイトオブジェクトを使用します。
    worlds: Mutex<HashMap<String, Arc<dyn AnyWorld>>>,
}

impl Multiverse {
    /// 新しい空の `Multiverse` インスタンスを生成します。
    ///
    /// 最初はどのWorldも含まれていません。`add`メソッドを使用してWorldを追加できます。
    ///
    /// # 例
    /// ```
    /// use parallel_world::Multiverse;
    ///
    /// let pw = Multiverse::new();
    /// assert!(pw.list().is_empty());
    /// ```
    pub fn new() -> Self {
        Multiverse {
            worlds: Mutex::new(HashMap::new()),
        }
    }

    /// `Multiverse` に新しい `World` を追加します。
    /// 同じIDのWorldが既に存在する場合はエラーを返します。
    ///
    /// # 型引数
    /// * `R` - 追加するWorldが返す結果の型。
    ///
    /// # 引数
    /// * `id` - 追加するWorldの一意な識別子（String）。
    /// * `world` - 追加する `World<R>` インスタンス
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World};
    ///
    /// let pw = Multiverse::new();
    /// let world1 = World::from(|| 1);
    /// let world2 = World::from(|| "hello".to_string());
    ///
    /// assert!(pw.add("task_one".to_string(), world1).is_ok());
    /// assert!(pw.add("task_two".to_string(), world2).is_ok());
    ///
    /// // 同じIDを追加しようとするとエラー
    /// let world_duplicate: World<()> = World::new(); // 戻り値の型を明示
    /// assert!(pw.add("task_one".to_string(), world_duplicate).is_err());
    /// ```
    pub fn add<R: Send + 'static + std::any::Any>(
        &self,
        id: String,
        world: World<R>,
    ) -> Result<(), String> {
        let mut worlds_guard = self.worlds.lock().unwrap();
        if worlds_guard.contains_key(&id) {
            return Err(format!("World with ID '{}' already exists.", id));
        }
        // World<R>をArc<dyn AnyWorld>にダウンキャストして挿入
        worlds_guard.insert(id, Arc::new(world));
        Ok(())
    }

    /// `Multiverse` から指定されたIDの `World` を削除します。
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
    /// use parallel_world::{Multiverse, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = Multiverse::new();
    /// let world1 = World::from(|| { sleep(Duration::from_millis(100)); 1 });
    /// pw.add("task_one".to_string(), world1).unwrap();
    ///
    /// // 実行中のWorldは削除できない
    /// pw.exec("task_one").unwrap();
    /// sleep(Duration::from_millis(1)); // 状態更新を待つ
    /// assert!(pw.del("task_one").is_err());
    ///
    /// // 停止または完了後に削除できる
    /// pw.status::<i32>("task_one").unwrap(); // 完了を待つ
    /// assert!(pw.del("task_one").is_ok());
    /// assert!(pw.list().is_empty());
    ///
    /// // 存在しないWorldの削除はエラー
    /// assert!(pw.del("non_existent_task").is_err());
    /// ```
    pub fn del(&self, id: &str) -> Result<(), String> {
        let mut worlds_guard = self.worlds.lock().unwrap();
        if let Some(world) = worlds_guard.get(id) {
            if world.any_progress() == WorldStatus::Running {
                return Err(format!(
                    "Cannot delete running World with ID '{}'. Stop it first.",
                    id
                ));
            }
            worlds_guard.remove(id);
            Ok(())
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// `Multiverse` に保存されたWorldのIDのリストを取得します。
    ///
    /// # 戻り値
    /// 現在登録されているWorldのID（`String`）のベクタ。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World};
    ///
    /// let pw = Multiverse::new();
    /// pw.add("alpha".to_string(), World::from(|| ())).unwrap();
    /// pw.add("beta".to_string(), World::from(|| ())).unwrap();
    ///
    /// let mut ids = pw.list();
    /// ids.sort(); // 順序を保証するためにソート
    /// assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
    /// ```
    pub fn list(&self) -> Vec<String> {
        self.worlds.lock().unwrap().keys().cloned().collect()
    }

    /// `Multiverse`からWorldを検索し、`Arc<dyn AnyWorld>`参照を返します。
    /// 特定の型の戻り値を持つWorldを取得したい場合は、この参照をダウンキャストする必要があります。
    pub fn get(&self, id: &str) -> Option<Arc<dyn AnyWorld>> {
        self.worlds.lock().unwrap().get(id).cloned()
    }

    /// 登録されているすべての `World` のうち、状態が `Ready` のものを一括で実行開始します。
    ///
    /// 各Worldの`start()`メソッドを呼び出しますが、個々のWorldで発生した開始エラーは無視されます。
    /// それぞれのWorldのログや`progress()`メソッドで状態を確認してください。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = Multiverse::new();
    /// pw.add("task_a".to_string(), World::from(|| { println!("Task A running..."); sleep(Duration::from_millis(50)); 1 })).unwrap();
    /// pw.add("task_b".to_string(), World::from(|| { println!("Task B running..."); sleep(Duration::from_millis(100)); "done".to_string() })).unwrap();
    ///
    /// pw.start_all();
    ///
    /// // 少し待って、両方のタスクが実行中になっていることを確認
    /// sleep(Duration::from_millis(10));
    /// assert_eq!(pw.progress("task_a").unwrap(), WorldStatus::Running);
    /// assert_eq!(pw.progress("task_b").unwrap(), WorldStatus::Running);
    ///
    /// pw.status::<i32>("task_a").unwrap(); // 完了を待つ
    /// pw.status::<String>("task_b").unwrap(); // 完了を待つ
    /// assert_eq!(pw.progress("task_a").unwrap(), WorldStatus::Finished);
    /// assert_eq!(pw.progress("task_b").unwrap(), WorldStatus::Finished);
    /// ```
    pub fn start_all(&self) {
        let worlds_guard = self.worlds.lock().unwrap();
        for (_, world) in worlds_guard.iter() {
            if world.any_progress() == WorldStatus::Ready {
                let _ = world.any_start(); // エラーは無視（個々のWorldのログで対応）
            }
        }
    }

    /// 特定のWorldを実行開始します。
    ///
    /// # Errors
    /// Worldが見つからない、または既に実行中の場合にエラーを返します。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = Multiverse::new();
    /// pw.add("my_task".to_string(), World::from(|| { println!("Task running..."); sleep(Duration::from_millis(50)); 42 })).unwrap();
    ///
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Ready);
    ///
    /// assert!(pw.exec("my_task").is_ok());
    /// sleep(Duration::from_millis(1)); // 状態更新を待つ
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Running);
    ///
    /// // 既に実行中のWorldを再度開始しようとするとエラー
    /// assert!(pw.exec("my_task").is_err());
    ///
    /// pw.status::<i32>("my_task").unwrap(); // 完了を待つ
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Finished);
    /// ```
    pub fn exec(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            world.any_start()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// すべての実行中のWorldを停止します。
    pub fn stop_all(&self) {
        let worlds_guard = self.worlds.lock().unwrap();
        for (_, world) in worlds_guard.iter() {
            if world.any_progress() == WorldStatus::Running {
                let _ = world.any_stop(); // エラーは無視
            }
        }
    }

    /// 特定のWorldを停止します。
    ///
    /// # Errors
    /// Worldが見つからない、または実行中でない場合にエラーを返します。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    /// use std::sync::{Arc, Mutex};
    ///
    /// let pw = Multiverse::new();
    /// let stop_flag = Arc::new(Mutex::new(false));
    /// let flag_clone = Arc::clone(&stop_flag);
    /// pw.add("my_long_task".to_string(), World::from(move || {
    ///     for i in 0..10 {
    ///         sleep(Duration::from_millis(100)); println!("My long task: {}", i);
    ///     }
    ///     0
    /// })).unwrap();
    ///
    /// pw.exec("my_long_task").unwrap();
    /// sleep(Duration::from_millis(200)); // 少し実行させる
    ///
    /// assert_eq!(pw.progress("my_long_task").unwrap(), WorldStatus::Running);
    ///
    /// println!("Attempting to kill my_long_task...");
    /// *stop_flag.lock().unwrap() = true; // 停止フラグをセット
    /// pw.kill("my_long_task").unwrap(); // stopを呼び出す
    ///
    /// sleep(Duration::from_millis(150)); // 協調的停止が完了するまで待つ
    /// let status = pw.progress("my_long_task").unwrap();
    /// assert_eq!(status, WorldStatus::Stopped); // 協調的停止によりStopped
    ///
    /// // 存在しないWorldのkillはエラー
    /// assert!(pw.kill("non_existent_task").is_err());
    /// ```
    pub fn kill(&self, id: &str) -> Result<(), String> {
        if let Some(world) = self.get(id) {
            world.any_stop()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 指定されたWorldの実行状態を取得します。
    ///
    /// # Errors
    /// Worldが見つからない場合にエラーを返します。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = Multiverse::new();
    /// pw.add("my_task".to_string(), World::from(|| { sleep(Duration::from_millis(20)); 123 })).unwrap();
    ///
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Ready);
    /// pw.exec("my_task").unwrap();
    /// sleep(Duration::from_millis(1)); // 状態更新を待つ
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Running);
    ///
    /// pw.status::<i32>("my_task").unwrap(); // 完了を待つ
    /// assert_eq!(pw.progress("my_task").unwrap(), WorldStatus::Finished);
    ///
    /// assert!(pw.progress("non_existent_task").is_err());
    /// ```
    pub fn progress(&self, id: &str) -> Result<WorldStatus, String> {
        if let Some(world) = self.get(id) {
            Ok(world.any_progress())
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 指定されたWorldの実行終了を待機し、その結果（`Box<dyn Any + Send>`）を返します。
    ///
    /// このメソッドは、タスクが完了するまでブロックします。
    /// 戻り値の型は`Box<dyn Any + Send>`となるため、呼び出し側で元の型にダウンキャストする必要があります。
    ///
    /// # Errors
    /// Worldが見つからない、タスクが失敗した、または結果のダウンキャストに失敗した場合にエラーを返します。
    pub fn status_any(&self, id: &str) -> Result<Box<dyn Any + Send>, String> {
        if let Some(world) = self.get(id) {
            world.any_status()
        } else {
            Err(format!("World with ID '{}' not found.", id))
        }
    }

    /// 指定されたWorldの実行終了を待機し、期待される型の結果を返します。
    ///
    /// このメソッドは、タスクが完了するまでブロックします。
    /// 結果は自動的に`T`型にダウンキャストされます。
    /// ダウンキャストに失敗した場合（期待する型と実際の型が異なる場合）、エラーを返します。
    ///
    /// # 型引数
    /// * `T` - 期待される戻り値の型。
    ///
    /// # エラー
    /// Worldが見つからない、タスクが失敗した、または結果のダウンキャストに失敗した場合にエラーを返します。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{Multiverse, World};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let pw = Multiverse::new();
    ///
    /// // 整数を返すタスク
    /// pw.add("sum_task".to_string(), World::from(|| {
    ///     sleep(Duration::from_millis(50));
    ///     10 + 20
    /// })).unwrap();
    /// pw.exec("sum_task").unwrap();
    /// let sum_result: Result<i32, String> = pw.status("sum_task");
    /// assert!(sum_result.is_ok());
    /// assert_eq!(sum_result.unwrap(), 30);
    ///
    /// // 文字列を返すタスク
    /// pw.add("message_task".to_string(), World::from(|| {
    ///     sleep(Duration::from_millis(30));
    ///     "Task finished!".to_string()
    /// })).unwrap();
    /// pw.exec("message_task").unwrap();
    /// let msg_result: Result<String, String> = pw.status("message_task");
    /// assert!(msg_result.is_ok());
    /// assert_eq!(msg_result.unwrap(), "Task finished!".to_string());
    ///
    /// // 失敗するタスク
    /// pw.add("fail_task".to_string(), World::from(|| {
    ///     panic!("Intentional error!");
    ///     0 // ダミーの戻り値
    /// })).unwrap();
    /// pw.exec("fail_task").unwrap();
    /// let fail_result: Result<i32, String> = pw.status("fail_task");
    /// assert!(fail_result.is_err());
    /// println!("Fail task error: {}", fail_result.unwrap_err());
    ///
    /// // 存在しないWorldのstatusはエラー
    /// assert!(pw.status::<()>("non_existent_task").is_err());
    /// ```
    pub fn status<T: Send + 'static + std::any::Any>(&self, id: &str) -> Result<T, String> {
        self.status_any(id)? // まずAnyWorldトレイトオブジェクトとして結果を取得
            .downcast::<T>() // T型にダウンキャストを試みる
            .map(|b| *b) // BoxからTを取り出す
            .map_err(|_| {
                format!(
                    "Failed to downcast result for World '{}' to expected type.",
                    id
                )
            })
    }
}

impl Default for Multiverse {
    fn default() -> Self {
        Self::new()
    }
}
