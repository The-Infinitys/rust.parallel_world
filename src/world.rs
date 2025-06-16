use std::any::Any;
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

/// Worldの実行状態を表す列挙型
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
/// `World` は `Multiverse` クレートで使用されるタスクの基本的な単位です。
///
/// 各 `World` は、独立した並行操作を表し、その実行状態を管理します。
/// ユーザーは実行したい関数（クロージャ）を `World` として定義し、
/// `start` メソッドで実行を開始したり、`status` メソッドで完了を待機したりできます。
/// `R` はこのWorldが返す結果の型です。
pub struct World<R: Send + 'static> {
    /// 実行する関数（Job）
    process: WorldProcess<R>,
    /// Worldの現在の状態
    status: Arc<Mutex<WorldStatus>>,
    /// 実行中のスレッドハンドル（Noneは未実行または実行完了/停止）
    thread_handle: WorldThreadHandle,
    /// タスクの実行結果を送信するためのチャネルの送信側。
    result_sender: WorldResultSender<R>,
    /// タスクの実行結果を受信するためのチャネルの受信側。
    result_receiver: WorldResultReceiver<R>,
}

type WorldProcess<R> = Mutex<Option<Box<dyn FnOnce() -> R + Send + 'static>>>;
type WorldThreadHandle = Mutex<Option<JoinHandle<()>>>;
type WorldResultSender<R> = Mutex<Option<mpsc::Sender<Result<R, String>>>>;
type WorldResultReceiver<R> = Arc<Mutex<Option<mpsc::Receiver<Result<R, String>>>>>;

impl<R: Send + 'static> Default for World<R> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Send + 'static> World<R> {
    /// 何もしないWorldを作成します。
    /// このWorldは、実行する関数が設定されていないため、start()を呼び出すとエラーになります。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    ///
    /// // 戻り値の型を明示的に指定する必要がある
    /// let world: World<()> = World::new();
    /// assert_eq!(world.progress(), WorldStatus::Ready);
    /// ```
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        World {
            process: Mutex::new(None),
            status: Arc::new(Mutex::new(WorldStatus::Ready)),
            thread_handle: Mutex::new(None),
            result_sender: Mutex::new(Some(tx)),
            result_receiver: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// 関数からWorldを作成します。
    ///
    /// # 型引数
    /// * `F` - 実行する関数を表す型。`FnOnce() -> R`（一度だけ実行可能で`R`を返す）、
    ///   `Send`（スレッド間で安全に移動可能）、
    ///   `'static`（ライフタイムがプログラム全体にわたる）のトレイト境界を満たす必要があります。
    /// * `R` - 関数が返す結果の型。`Send + 'static`のトレイト境界を満たす必要があります。
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
    ///     42 // 整数を返す
    /// });
    /// assert_eq!(my_task.progress(), WorldStatus::Ready);
    /// ```
    pub fn from<F>(f: F) -> Self
    where
        F: FnOnce() -> R + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        World {
            process: Mutex::new(Some(Box::new(f))),
            status: Arc::new(Mutex::new(WorldStatus::Ready)),
            thread_handle: Mutex::new(None),
            result_sender: Mutex::new(Some(tx)),
            result_receiver: Arc::new(Mutex::new(Some(rx))),
        }
    }

    /// Worldのプロセスを実行し、終了まで待機します。
    /// このメソッドは現在のスレッドをブロックします。
    ///
    /// # 戻り値
    /// `Ok(R)` - プロセスが正常に完了し、`R`型の値を返した場合。
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
    ///     123
    /// });
    /// let result = world.run();
    /// assert!(result.is_ok());
    /// assert_eq!(result.unwrap(), 123);
    /// assert_eq!(world.progress(), WorldStatus::Finished);
    ///
    /// let world_with_panic = World::from(|| { panic!("Oops!"); 0 }); // 戻り値の型を合わせるためにダミーの0
    /// let result_fail = world_with_panic.run();
    /// assert!(result_fail.is_err());
    /// assert!(matches!(world_with_panic.progress(), WorldStatus::Failed(_)));
    /// ```
    pub fn run(&self) -> Result<R, String> {
        self.start()?; // バックグラウンドで実行開始
        self.status() // 実行終了を待機し、結果を返す
    }

    /// Worldのプロセスを新しいスレッドで非同期に実行開始します。
    ///
    /// このメソッドは、プロセスの実行完了を待たずにすぐに制御を返します。
    /// プロセスの状態を監視するには `progress()` を、完了を待つには `status()` を使用します。
    /// プロセス内で発生したパニックは捕捉され、`WorldStatus::Failed`として記録されます。
    ///
    /// # エラー
    /// * `Err("World is already running.")` - この`World`が既に実行中の場合に返されます。
    /// * `Err("World has already completed or failed and cannot be restarted.")` - 既に完了または失敗したWorldを再実行しようとした場合に返されます。
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
    ///     true
    /// });
    ///
    /// assert_eq!(world.progress(), WorldStatus::Ready);
    /// assert!(world.start().is_ok());
    /// sleep(Duration::from_millis(1)); // 状態更新を待つ
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
        if *status_guard == WorldStatus::Finished || matches!(*status_guard, WorldStatus::Failed(_)) {
            return Err(
                "World has already completed or failed and cannot be restarted.".to_string(),
            );
        }

        let mut process_guard = self.process.lock().unwrap();
        let process_opt = process_guard.take();

        if let Some(process_fn) = process_opt {
            let status_clone = Arc::clone(&self.status);
            let result_sender_opt = self.result_sender.lock().unwrap().take();

            if result_sender_opt.is_none() {
                return Err("Internal error: Result sender not available.".to_string());
            }
            let result_sender = result_sender_opt.unwrap();

            let handle = thread::spawn(move || {
                let mut s = status_clone.lock().unwrap();
                *s = WorldStatus::Running;
                drop(s);

                let result = match std::panic::catch_unwind(AssertUnwindSafe(process_fn)) {
                    Ok(val) => {
                        let mut s = status_clone.lock().unwrap();
                        if *s != WorldStatus::Stopped { // Stoppedが設定されていなければFinished
                            *s = WorldStatus::Finished;
                        }
                        Ok(val)
                    }
                    Err(e) => {
                        let err_msg = format!("Thread panicked: {:?}", e);
                        let mut s = status_clone.lock().unwrap();
                        *s = WorldStatus::Failed(err_msg.clone());
                        Err(err_msg)
                    }
                };
                let _ = result_sender.send(result);
            });

            let mut thread_handle_guard = self.thread_handle.lock().unwrap();
            *thread_handle_guard = Some(handle);
            Ok(())
        } else {
            Err("No process defined for this World.".to_string())
        }
    }

    /// Worldのプロセスを停止します（ベストエフォート）。
    ///
    /// Rustの標準ライブラリの`std::thread`には、実行中のスレッドを外部から
    /// 強制的に停止させる安全なメカニズムは提供されていません。
    /// したがって、このメソッドは`WorldStatus`を`Stopped`に設定し、
    /// スレッドハンドルを解放（デタッチ）します。
    ///
    /// 実行中のタスク（クロージャ）がこの停止シグナルに応答するには、
    /// クロージャ内で定期的に停止フラグをチェックし、
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
    ///         // 外部からの停止フラグをチェック（協調的停止）
    ///         if *flag_clone.lock().unwrap() {
    ///             println!("Cooperative task received stop signal. Exiting.");
    ///             break 0; // 0 を返して終了
    ///         }
    ///         println!("Cooperative task: {}", counter);
    ///         sleep(Duration::from_millis(50));
    ///         counter += 1;
    ///         if counter > 50 { break counter; } // 無限ループ防止の保険
    ///     }
    /// });
    ///
    /// world.start().unwrap();
    /// sleep(Duration::from_millis(150)); // 少し実行させる
    ///
    /// assert_eq!(world.progress(), WorldStatus::Running);
    /// println!("Attempting to stop the cooperative world...");
    /// *stop_flag.lock().unwrap() = true; // 停止シグナルを送る
    ///
    /// world.stop().unwrap();
    /// let result = world.status();
    /// assert!(result.is_ok()); // 協調的に終了すればOk
    /// assert_eq!(world.progress(), WorldStatus::Stopped); // 協調的停止によりStopped
    /// ```
    pub fn stop(&self) -> Result<(), String> {
        let mut status_guard = self.status.lock().unwrap();
        if *status_guard != WorldStatus::Running {
            return Err("World is not running or already stopped.".to_string());
        }
        *status_guard = WorldStatus::Stopped;
        drop(status_guard); // ロックを早期に解放

        // スレッドハンドルをNoneにするが、joinはしない。これにより、stop()はブロックしない。
        // スレッド自体が協調的に終了するか、外部からstatus()でjoinされるのを待つ。
        let mut handle_guard = self.thread_handle.lock().unwrap();
        let _ = handle_guard.take(); // ハンドルの所有権を放棄（スレッドはバックグラウンドで実行継続または終了する）

        // result_senderをドロップし、これにより受信側でRecvErrorが発生するようにできる
        let _ = self.result_sender.lock().unwrap().take();

        Ok(())
    }

    /// Worldの実行状態を取得します。
    pub fn progress(&self) -> WorldStatus {
        self.status.lock().unwrap().clone()
    }

    /// 実行終了まで待機し、成功したか失敗したかなどの値を返します。
    ///
    /// # 戻り値
    /// `Ok(R)` - プロセスが正常に完了し、`R`型の値を返した場合。
    /// `Err(String)` - プロセスが失敗した場合、またはWorldが見つからない場合。
    ///                 また、Worldが停止されたり、結果が取得できなかったりした場合も`Err`。
    ///
    /// # 例
    /// ```
    /// use parallel_world::{World, WorldStatus};
    /// use std::thread::sleep;
    /// use std::time::Duration;
    ///
    /// let success_world = World::from(|| { sleep(Duration::from_millis(5)); "Success!".to_string() });
    /// success_world.start().unwrap();
    /// assert_eq!(success_world.status().unwrap(), "Success!".to_string());
    /// assert_eq!(success_world.progress(), WorldStatus::Finished);
    ///
    /// let fail_world = World::from(|| { panic!("Something went wrong!"); 0 });
    /// fail_world.start().unwrap();
    /// let result = fail_world.status();
    /// assert!(result.is_err());
    /// assert!(matches!(fail_world.progress(), WorldStatus::Failed(_)));
    /// println!("Error from failed world: {}", result.unwrap_err());
    /// ```
    pub fn status(&self) -> Result<R, String> {
        let mut handle_guard = self.thread_handle.lock().unwrap();
        if let Some(handle) = handle_guard.take() {
            // スレッドが完了するまで待機
            // ここでスレッドがパニックした場合、Errが返る
            handle
                .join()
                .map_err(|e| format!("Thread panicked: {:?}", e))?;
        }

        // スレッドが終了した後、チャネルから結果を受け取る
        let mut receiver_opt = self.result_receiver.lock().unwrap();
        if let Some(receiver) = receiver_opt.take() {
            match receiver.recv() {
                Ok(task_result) => task_result, // タスク自体が返したResult<R, String>
                Err(_) => {
                    // 送信側がドロップされたか、メッセージが送信されなかった場合
                    let current_status = self.status.lock().unwrap().clone();
                    match current_status {
                        WorldStatus::Finished => {
                            Err("World finished but result not sent (internal error).".to_string())
                        }
                        WorldStatus::Failed(e) => Err(e),
                        WorldStatus::Stopped => {
                            Err("World was stopped before completion.".to_string())
                        }
                        WorldStatus::Killed => {
                            Err("World was killed before completion.".to_string())
                        }
                        _ => Err(format!(
                            "World ended with unexpected status and no result: {}",
                            current_status
                        )),
                    }
                }
            }
        } else {
            // result_receiverが既にtakeされていた場合（status()が複数回呼ばれたなど）
            let current_status = self.status.lock().unwrap().clone();
            match current_status {
                WorldStatus::Finished => Err("World result already retrieved.".to_string()),
                WorldStatus::Failed(e) => Err(e),
                _ => Err(format!(
                    "World result not available for status: {}",
                    current_status
                )),
            }
        }
    }
}

// DefaultトレイトはR型によって異なるため、一般的な実装は提供できない。
// 特定のR型に対してのみDefaultを実装できる。
// 例: impl Default for World<()> { ... }

// ------ AnyWorld (Multiverseで異なるR型のWorldを管理するためのトレイト) ------

/// 異なる戻り値の型を持つWorldをまとめて管理するためのトレイトです。
/// `Multiverse` が複数の `World<R>` を型消去して保持するために使用します。
pub trait AnyWorld: Send + Sync {
    /// Worldの現在の状態を取得します。
    fn any_progress(&self) -> WorldStatus;
    /// Worldを実行開始します。
    fn any_start(&self) -> Result<(), String>;
    /// Worldを停止します。
    fn any_stop(&self) -> Result<(), String>;
    /// Worldが完了するまで待機し、結果を`Box<dyn Any + Send>`として返します。
    fn any_status(&self) -> Result<Box<dyn Any + Send>, String>;
}

// World<R> が AnyWorld トレイトを実装するようにする
impl<R: Send + 'static + Any> AnyWorld for World<R> {
    fn any_progress(&self) -> WorldStatus {
        self.progress()
    }

    fn any_start(&self) -> Result<(), String> {
        self.start()
    }

    fn any_stop(&self) -> Result<(), String> {
        self.stop()
    }

    fn any_status(&self) -> Result<Box<dyn Any + Send>, String> {
        self.status().map(|r| Box::new(r) as Box<dyn Any + Send>)
    }
}