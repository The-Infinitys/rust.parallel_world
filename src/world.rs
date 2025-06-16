// src/world.rs

use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// Worldの実行状態を表す列挙型
#[derive(Debug, Clone, PartialEq)]
pub enum WorldStatus {
    Ready,          // 実行準備完了
    Running,        // 実行中
    Finished,       // 実行完了（成功）
    Failed(String), // 実行失敗（エラーメッセージを含む）
    Stopped,        // 停止済み
    Killed,         // 強制終了済み
}

impl fmt::Display for WorldStatus {
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

/// WorldはParalellWorldsで使用するタスクの単位です。
pub struct World {
    /// 実行する関数（Job）
    // Option<Box<dyn FnOnce() + Send + 'static>> を使用して、
    // ジョブが実行された後にNoneに設定できるようにします。
    // Mutexでラップして、スレッド間で安全にアクセスできるようにします。
    process: Mutex<Option<Box<dyn FnOnce() + Send + 'static>>>,
    /// Worldの現在の状態
    status: Arc<Mutex<WorldStatus>>,
    /// 実行中のスレッドハンドル（Noneは未実行または実行完了/停止）
    // Option<JoinHandle<()>> を使用して、スレッドが実行中か否かを管理します。
    // また、join()を呼び出す際に所有権を移せるようにします。
    thread_handle: Mutex<Option<JoinHandle<()>>>,
}

impl World {
    /// 何もしないWorldを作成します。
    pub fn new() -> Self {
        World {
            process: Mutex::new(None), // 初期状態ではプロセスなし
            status: Arc::new(Mutex::new(WorldStatus::Ready)),
            thread_handle: Mutex::new(None),
        }
    }

    /// 関数からWorldを作成します。
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

    /// Worldのプロセスを実行し、終了まで待機します。
    /// このメソッドは現在のスレッドをブロックします。
    pub fn run(&self) -> Result<(), String> {
        self.start()?; // バックグラウンドで実行開始
        self.status() // 実行終了を待機し、結果を返す
    }

    /// Worldのプロセスを実行します（終了を待機しない）。
    /// このメソッドは新しいスレッドを生成し、すぐに戻ります。
    ///
    /// # Errors
    /// Worldが既に実行中、またはプロセスが設定されていない場合にエラーを返します。
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

    /// Worldのプロセスを停止します（ベストエフォート）。
    /// Rustの`std::thread`には直接スレッドを停止する機能がないため、
    /// この機能の実装はより複雑になります。ここではplace holderとしています。
    /// （例: 実行中のタスクに停止シグナルを送るようなメカニズムが必要）
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

    /// Worldの実行状態を取得します。
    pub fn progress(&self) -> WorldStatus {
        self.status.lock().unwrap().clone()
    }

    /// 実行終了まで待機し、成功したか失敗したかなどの値を返します。
    ///
    /// # Returns
    /// `Ok(())` 成功時、`Err(String)` 失敗時
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
            _ => Err(format!(
                "World finished with unexpected status: {}",
                current_status
            )),
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
