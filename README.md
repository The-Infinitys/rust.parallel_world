# parallel_world

`parallel_world` は、Rust で並行タスクを管理するためのライブラリです。このライブラリを使うと、複数のタスクを並行して実行し、それぞれのタスクの状態を監視したり、停止したりすることができます。各タスクは独自のスレッドで実行され、異なる型の戻り値を持つことができます。

## インストール

このクレートをプロジェクトに追加するには、`Cargo.toml` に以下の行を追加してください:

```toml
[dependencies]
parallel_world = "0.1.0"
```

※注: このクレートは現在公開されていないため、ローカルで使用する場合は、プロジェクトディレクトリに `parallel_world` ディレクトリを配置し、`Cargo.toml` に以下を追加してください:

```toml
[dependencies]
parallel_world = { path = "../parallel_world" }
```

## 使用例

以下は、`parallel_world` を使用した簡単な例です:

```rust
use parallel_world::{Multiverse, World};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    let pw = Multiverse::new();

    // World A: 成功するタスク
    let world_a = World::from(|| {
        println!("[World A] 開始");
        sleep(Duration::from_millis(50));
        println!("[World A] 完了");
        100
    });
    pw.add("World_A".to_string(), world_a).unwrap();

    // World B: 文字列を返すタスク
    let world_b = World::from(|| {
        println!("[World B] 開始");
        sleep(Duration::from_millis(100));
        println!("[World B] 完了");
        "Hello from World B!".to_string()
    });
    pw.add("World_B".to_string(), world_b).unwrap();

    // 全てのWorldを実行開始
    pw.start_all();

    // World A の結果を取得
    match pw.status::<i32>("World_A") {
        Ok(result) => println!("World A: {}", result),
        Err(e) => println!("World A エラー: {}", e),
    }

    // World B の結果を取得
    match pw.status::<String>("World_B") {
        Ok(result) => println!("World B: {}", result),
        Err(e) => println!("World B エラー: {}", e),
    }
}
```

この例では、2 つの`World`を作成し、`Multiverse`に追加してから、全てを並行して実行します。そして、それぞれの`World`の結果を取得します。

## API

### Multiverse

| メソッド                                                                           | 説明                                                    |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------- |
| `new() -> Self`                                                                    | 新しい `Multiverse` インスタンスを作成します。      |
| `add<R: Send + 'static>(&self, id: String, world: World<R>) -> Result<(), String>` | 新しい `World` を追加します。                           |
| `del(&self, id: &str) -> Result<(), String>`                                       | 指定された ID の `World` を削除します（実行中は不可）。 |
| `list(&self) -> Vec<String>`                                                       | 登録されている `World` の ID リストを取得します。       |
| `start_all(&self)`                                                                 | 全ての `Ready` 状態の `World` を実行開始します。        |
| `exec(&self, id: &str) -> Result<(), String>`                                      | 指定された ID の `World` を実行開始します。             |
| `stop_all(&self)`                                                                  | 全ての実行中の `World` を停止します。                   |
| `kill(&self, id: &str) -> Result<(), String>`                                      | 指定された ID の `World` を停止します。                 |
| `progress(&self, id: &str) -> Result<WorldStatus, String>`                         | 指定された ID の `World` の状態を取得します。           |
| `status<T: Send + 'static>(&self, id: &str) -> Result<T, String>`                  | 指定された ID の `World` の実行結果を取得します。       |

### World<R>

| メソッド                                                          | 説明                             |
| ----------------------------------------------------------------- | -------------------------------- |
| `from<F>(f: F) -> Self` where `F: FnOnce() -> R + Send + 'static` | 新しい `World` を作成します。    |
| `start(&self) -> Result<(), String>`                              | `World` を実行開始します。       |
| `stop(&self) -> Result<(), String>`                               | `World` を停止します。           |
| `progress(&self) -> WorldStatus`                                  | `World` の状態を取得します。     |
| `status(&self) -> Result<R, String>`                              | `World` の実行結果を取得します。 |

### WorldStatus

| 状態             | 説明                         |
| ---------------- | ---------------------------- |
| `Ready`          | 実行準備完了                 |
| `Running`        | 実行中                       |
| `Finished`       | 正常終了                     |
| `Failed(String)` | 失敗（エラーメッセージ付き） |
| `Stopped`        | 停止                         |
| `Killed`         | 強制終了                     |

## 協調的な停止

`World` は、内部で停止フラグをチェックすることで、協調的に停止することができます。以下は、停止可能な `World` の例です:

```rust
let stop_flag = Arc::new(Mutex::new(false));
let stop_flag_clone = Arc::clone(&stop_flag);
let world_d = World::from(move || {
    let mut i = 0;
    loop {
        if *stop_flag_clone.lock().unwrap() {
            println!("[World D] 停止シグナルを受信しました。");
            break i;
        }
        println!("[World D] 処理中... {}", i);
        sleep(Duration::from_millis(20));
        i += 1;
        if i > 50 {
            break i;
        }
    }
});
pw.add("World_D".to_string(), world_d).unwrap();
pw.exec("World_D").unwrap();
// 後で停止
*stop_flag.lock().unwrap() = true;
// status を呼んで完了を待つ
let result = pw.status::<i32>("World_D").unwrap();
```

このように、`World` 内部で停止フラグをチェックすることで、外部からの停止リクエストに応答することができます。

## 注意点

- **スレッドベースの並行処理**: このクレートはスレッドを使用するため、CPU バウンドタスクに適しています。I/O バウンドタスクには、Rust の非同期ランタイム（例：[tokio](https://tokio.rs/)）が適している場合があります。
- **タスクの要件**: タスクは`Send + 'static`である必要があり、非静的参照を含むクロージャは使用できません。
- **エラーハンドリング**: パニックは捕捉され、`WorldStatus::Failed`として記録されます。

## ライセンス

このクレートは MIT ライセンスの下で配布されています。
