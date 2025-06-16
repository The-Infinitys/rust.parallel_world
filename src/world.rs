// src/world.rs

use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// `WorldStatus` は、個々のタスク（`World`）の現在の実行状態を表す列挙型です。
///
/// この状態は、タスクのライフサイクルを通じて変化します。
#[derive(Debug, Clone, PartialEq)]
pub enum WorldStatus {
    /// タスクは作成されたばかりで、実行準備ができています。
    Ready,
    /// タスクは現在実行中です。
    Running,
    /// タスクは正常に実行を完了しました。
    Finished,
    /// タスクの実行中にエラーが発生し、失敗しました。
    /// 失敗の具体的な理由を含む文字列を保持します。
    Failed(String),
    /// タスクは外部からの指示により停止されました。
    Stopped,
    /// タスクは外部からの強力な指示により強制終了されました。
    /// （現在の`std::thread`の制限により、`Stopped`とほぼ同じ挙動をします。）
    Killed,
}

impl fmt::Display for WorldStatus {
    /// `WorldStatus` を人間が読める文字列形式にフォーマットします。
    ///
    /// # 例
    /// ```
    /// use parallel_world::WorldStatus;
    ///
    /// assert_eq!(WorldStatus::Running.to_string(), "Running");
    /// assert_eq!(WorldStatus::Failed("テストエラー".to_string()).to_string(), "Failed: テストエラー");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            WorldStatus::Ready => write!(f, "Ready"),
            WorldStatus::Running => write!(f, "Running"),
            WorldStatus::Finished => write!(f, "Finished"),
            WorldStatus::Failed(e) => write!(f, "Failed: {}", e),
            WorldStatus::Stopped => write!(f, "Stopped"),
            WorldStatus::Killed => write!(f, "Killed"),
        }
    }
}

/// # World
///
/// `World` は `ParallelWorlds` クレートで使用されるタスクの基本的な単位です。
///
/// 各 `World` は、独立した並行操作を表し、その実行状態を管理します。
/// ユーザーは実行したい関数（クロージャ）を `World` として定義し、
/// `start` メソッドで実行を開始したり、`status` メソッドで完了を待機したりできます。
pub struct World {
    /// この`World`で実行される関数（クロージャ）です。
    ///
    /// `FnOnce()`トレイトオブジェクトとして定義されており、一度しか実行できません。
    /// スレッド間で安全に移動できるように`Send`トレイトを要求し、
    /// 静的なライフタイム `'static` を持つことを保証します。
    /// `Mutex`によって保護され、実行後には`None`に設定されます。
    process: Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>,
    /// この`World`の現在の実行状態 (`WorldStatus`) を保持します。
    ///
    /// `Arc`と`Mutex`によって、複数のスレッドから安全に状態の読み書きが可能です。
    status: Arc<Mutex<WorldStatus>>,
    /// この`World`が実行されているスレッドの`JoinHandle`を保持します。
    ///
    /// `None`の場合、スレッドはまだ開始されていないか、既に完了/停止しています。
    /// `Mutex`によって保護され、`join()`呼び出し時にハンドルが移動できるように`Option`でラップされています。
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

impl World {
    /// 何もしない新しい `World` インスタンスを作成します。
    ///
    /// この`World`は、後から`process`フィールドに実行可能な関数を
    /// 手動で設定しない限り、`start()`を呼び出しても何も実行しません。
    /// 通常は `World::from()` を使用して関数を指定することをお勧めします。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    ///
    /// let world = World::new();
    /// assert_eq!(world.progress(), WorldStatus::Ready);
    /// ```
    pub fn new() -> Self {
        World {
            process: Mutex::new(None), // 初期状態ではプロセスなし
            status: Arc::new(Mutex::new(WorldStatus::Ready)),
            thread_handle: Mutex::new(None),
        }
    }

    /// 指定された関数（クロージャ）から新しい `World` インスタンスを作成します。
    ///
    /// このメソッドは、`World`が実行する主要なロジックをカプセル化するのに便利です。
    ///
    /// # 型引数
    /// * `F` - 実行する関数を表す型。`FnOnce()`（一度だけ実行可能）、
    ///         `Send`（スレッド間で安全に移動可能）、
    ///         `'static`（ライフタイムがプログラム全体にわたる）のトレイト境界を満たす必要があります。
    ///
    /// # 引数
    /// * `f` - この`World`が実行するクロージャ。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let my_task = World::from(|| {
    ///     println!("Hello from my task!");
    ///     sleep(Duration::from_millis(10));
    /// });
    /// assert_eq!(my_task.progress(), WorldStatus::Ready);
    /// ```
    pub fn from<F>(f: F) -> Self
    where
        F: FnOnce() + Send + 'static,
    {
        World {
            process: Mutex::new(Some(Box::new(f))),
            status: Arc::new(Mutex::new(WorldStatus::Ready)),
            thread_handle: Mutex::new(None),
        }
    }

    /// この `World` のプロセスを実行し、プロセスが完了するまで現在のスレッドをブロックします。
    ///
    /// 内部的には `start()` を呼び出してプロセスを非同期に開始し、
    /// その後 `status()` を呼び出して実行完了を待機します。
    ///
    /// # 戻り値
    /// `Ok(())` - プロセスが正常に完了した場合。
    /// `Err(String)` - プロセスが失敗した場合（パニックを含む）、または既に実行中であった場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let world = World::from(|| {
    ///     println!("Synchronous task executing...");
    ///     sleep(Duration::from_millis(5));
    /// });
    /// let result = world.run();
    /// assert!(result.is_ok());
    /// assert_eq!(world.progress(), WorldStatus::Finished);
    ///
    /// let world_with_panic = World::from(|| panic!("Oops!"));
    /// let result_fail = world_with_panic.run();
    /// assert!(result_fail.is_err());
    /// assert!(matches!(world_with_panic.progress(), WorldStatus::Failed(_)));
    /// ```
    pub fn run(&self) -> Result<(), String> {
        self.start()?; // バックグラウンドで実行開始
        self.status() // 実行終了を待機し、結果を返す
    }

    /// この `World` のプロセスを新しいスレッドで非同期に実行開始します。
    ///
    /// このメソッドは、プロセスの実行完了を待たずにすぐに制御を返します。
    /// プロセスの状態を監視するには `progress()` を、完了を待つには `status()` を使用します。
    /// プロセス内で発生したパニックは捕捉され、`WorldStatus::Failed`として記録されます。
    ///
    /// # エラー
    /// * `Err("World is already running.")` - この`World`が既に実行中の場合に返されます。
    /// * `Err("No process defined for this World.")` - `World::new()`で作成され、
    ///   まだ実行する関数が設定されていない`World`に対して呼び出された場合に返されます。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let world = World::from(|| {
    ///     println!("Async task started...");
    ///     sleep(Duration::from_millis(10));
    ///     println!("Async task finished.");
    /// });
    ///
    /// assert_eq!(world.progress(), WorldStatus::Ready);
    /// assert!(world.start().is_ok());
    /// assert_eq!(world.progress(), WorldStatus::Running);
    ///
    /// // 既に実行中のWorldを再度開始しようとするとエラー
    /// assert!(world.start().is_err());
    ///
    /// world.status().unwrap(); // 完了を待つ
    /// assert_eq!(world.progress(), WorldStatus::Finished);
    /// ```
    pub fn start(&self) -> Result<(), String> {
        let status_guard = self.status.lock().unwrap();
        if *status_guard == WorldStatus::Running {
            return Err("World is already running.".to_string());
        }

        let mut process_guard = self.process.lock().unwrap();
        let process_opt = process_guard.take(); // processの所有権を移動

        if let Some(process_fn) = process_opt {
            let status_clone = Arc::clone(&self.status);

            let handle = thread::spawn(move || {
                let mut s = status_clone.lock().unwrap();
                *s = WorldStatus::Running;
                drop(s); // ロックを早期に解放

                // パニックを捕捉するために AssertUnwindSafe を使用
                match std::panic::catch_unwind(AssertUnwindSafe(move || process_fn())) {
                    Ok(_) => {
                        let mut s = status_clone.lock().unwrap();
                        *s = WorldStatus::Finished;
                    }
                    Err(e) => {
                        let mut s = status_clone.lock().unwrap();
                        *s = WorldStatus::Failed(format!("{:?}", e));
                    }
                }
            });

            let mut thread_handle_guard = self.thread_handle.lock().unwrap();
            *thread_handle_guard = Some(handle);
            Ok(())
        } else {
            Err("No process defined for this World.".to_string())
        }
    }

    /// この `World` のプロセスを停止しようと試みます（ベストエフォート）。
    ///
    /// Rustの標準ライブラリの`std::thread`には、実行中のスレッドを外部から
    /// 強制的に停止させる安全なメカニズムは提供されていません。
    /// したがって、このメソッドは`WorldStatus`を`Stopped`に設定し、
    /// 可能であれば内部のスレッドが終了するのを`join()`で待機します。
    ///
    /// 実行中のタスク（クロージャ）がこの停止シグナルに応答するには、
    /// クロージャ内で定期的に`World`の`progress()`メソッドをチェックし、
    /// `WorldStatus::Stopped`になった場合に自ら終了するような
    /// 協調的な停止メカニズムを実装する必要があります。
    ///
    /// # エラー
    /// * `Err("World is not running or already stopped.")` - `World`が実行中でない場合に返されます。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    /// use std::sync::{Arc, Mutex};
    ///
    /// let stop_flag = Arc::new(Mutex::new(false));
    /// let flag_clone = Arc::clone(&stop_flag);
    /// let world = World::from(move || {
    ///     let mut counter = 0;
    ///     loop {
    ///         println!("Cooperative task: {}", counter);
    ///         sleep(Duration::from_millis(50));
    ///         counter += 1;
    ///         // 外部からの停止フラグをチェック
    ///         if *flag_clone.lock().unwrap() {
    ///             println!("Cooperative task received stop signal. Exiting.");
    ///             break;
    ///         }
    ///         if counter > 50 { break; } // 無限ループ防止の保険
    ///     }
    /// });
    ///
    /// assert!(world.start().is_ok());
    /// sleep(Duration::from_millis(150)); // 少し実行させる
    ///
    /// assert_eq!(world.progress(), WorldStatus::Running);
    /// println!("Attempting to stop the cooperative world...");
    /// *stop_flag.lock().unwrap() = true; // 停止シグナルを送る
    ///
    /// // ここでworld.stop()を呼び出しても良いですが、
    /// // 上記のようにWorldのクロージャが停止シグナルをチェックする方が協調的です。
    /// // world.stop().unwrap(); // もしWorld::stop()が停止フラグを設定するなら
    ///
    /// // WorldがStopped状態になるか、プロセスが終了するのを待つ
    /// world.status().unwrap(); // 完了を待つ
    /// assert_eq!(world.progress(), WorldStatus::Finished); // 協力的に終了した場合はFinished
    /// ```
    pub fn stop(&self) -> Result<(), String> {
        let mut status_guard = self.status.lock().unwrap();
        if *status_guard != WorldStatus::Running {
            return Err("World is not running or already stopped.".to_string());
        }
        // ここにスレッド停止のロジックを実装する必要があります。
        // 例えば、`process`クロージャ内で定期的に停止シグナルをチェックするような機構。
        *status_guard = WorldStatus::Stopped;
        // スレッドのjoin()を試みて、クリーンアップを行います。
        let mut handle_guard = self.thread_handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            // スレッドがまだ動いている場合、joinはブロックされます。
            // 強制終了が必要な場合は、別の方法を検討する必要があります。
            let _ = handle.join(); // エラーは無視
        }
        Ok(())
    }

    /// この `World` の現在の実行状態を取得します。
    ///
    /// このメソッドはブロックせず、すぐに現在の`WorldStatus`を返します。
    ///
    /// # 戻り値
    /// 現在の `WorldStatus`。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let world = World::from(|| sleep(Duration::from_millis(20)));
    /// assert_eq!(world.progress(), WorldStatus::Ready);
    /// world.start().unwrap();
    /// assert_eq!(world.progress(), WorldStatus::Running);
    /// sleep(Duration::from_millis(25)); // 完了を待つ
    /// assert_eq!(world.progress(), WorldStatus::Finished);
    /// ```
    pub fn progress(&self) -> WorldStatus {
        self.status.lock().unwrap().clone()
    }

    /// この `World` の実行が完了するまで現在のスレッドをブロックし、
    /// その最終結果（成功か失敗か）を返します。
    ///
    /// `World`が正常に終了した場合、`Ok(())`を返します。
    /// `World`がパニックしたり、`Failed`状態になった場合は、
    /// 失敗の理由を含む`Err(String)`を返します。
    ///
    /// # 戻り値
    /// `Ok(())` - `World`が正常に完了した場合。
    /// `Err(String)` - `World`が失敗（パニックを含む）した場合、または予期せぬ状態になった場合。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let success_world = World::from(|| sleep(Duration::from_millis(5)));
    /// success_world.start().unwrap();
    /// assert!(success_world.status().is_ok());
    /// assert_eq!(success_world.progress(), WorldStatus::Finished);
    ///
    /// let fail_world = World::from(|| panic!("Something went wrong!"));
    /// fail_world.start().unwrap();
    /// let result = fail_world.status();
    /// assert!(result.is_err());
    /// assert!(matches!(fail_world.progress(), WorldStatus::Failed(_)));
    /// println!("Error from failed world: {:?}", result.unwrap_err());
    /// ```
    pub fn status(&self) -> Result<(), String> {
        let mut handle_guard = self.thread_handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            // スレッドが完了するまで待機
            handle
                .join()
                .map_err(|e| format!("Thread panicked: {:?}", e))?;
        }

        let current_status = self.status.lock().unwrap().clone();
        match current_status {
            WorldStatus::Finished => Ok(()),
            WorldStatus::Failed(e) => Err(e),
            // World::run()やWorld::start()が呼ばれていない場合や、
            // stop()が呼ばれてスレッドがjoinされたが、Finished/Failedになっていない場合など
            _ => Err(format!(
                "World finished with unexpected status: {}",
                current_status
            )),
        }
    }
}

impl Default for World {
    /// `World::new()` と同じ結果を返します。
    ///
    /// `Default::default()` で `World` のインスタンスを生成できます。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    ///
    /// let world = World::default();
    /// assert_eq!(world.progress(), WorldStatus::Ready);
    /// ```
    fn default() -> Self {
        Self::new()
    }
}
