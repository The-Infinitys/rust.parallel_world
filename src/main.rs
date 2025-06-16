use parallel_world::{ParallelWorlds, World};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    println!("--- Parallel World Application Start ---");

    // 1. ParallelWorlds インスタンスを作成
    let pw = ParallelWorlds::new();

    // 2. 実行するWorld（タスク）を定義し、追加する

    // World A: 成功するタスク
    let world_a = World::from(|| {
        println!("[World A] 開始: 50ms 処理します。");
        sleep(Duration::from_millis(50));
        println!("[World A] 完了。");
    });
    pw.add("World_A".to_string(), world_a)
        .expect("Failed to add World_A");

    // World B: 少し長い処理をするタスク
    let world_b = World::from(|| {
        println!("[World B] 開始: 100ms 処理します。");
        sleep(Duration::from_millis(100));
        println!("[World B] 完了。");
    });
    pw.add("World_B".to_string(), world_b)
        .expect("Failed to add World_B");

    // World C: 失敗するタスク（パニックを発生させる）
    let world_c = World::from(|| {
        println!("[World C] 開始: パニックを発生させます。");
        sleep(Duration::from_millis(20));
        panic!("[World C] エラー発生！");
    });
    pw.add("World_C".to_string(), world_c)
        .expect("Failed to add World_C");

    println!("\n--- 全てのWorldを実行開始します ---");
    // 3. 全てのWorldを並行して実行開始
    pw.start_all();

    // 4. 各Worldの実行状況を監視し、完了を待機する

    // World A の状態を確認
    println!("\n--- World A の状態確認 ---");
    println!("World A の現在の状態: {}", pw.progress("World_A").unwrap());
    println!("World A の完了を待機中...");
    match pw.status("World_A") {
        Ok(_) => println!("World A が正常に完了しました。"),
        Err(e) => println!("World A の完了中にエラーが発生しました: {}", e),
    }
    println!("World A の最終状態: {}", pw.progress("World_A").unwrap());

    // World B の状態を確認
    println!("\n--- World B の状態確認 ---");
    println!("World B の現在の状態: {}", pw.progress("World_B").unwrap());
    println!("World B の完了を待機中...");
    match pw.status("World_B") {
        Ok(_) => println!("World B が正常に完了しました。"),
        Err(e) => println!("World B の完了中にエラーが発生しました: {}", e),
    }
    println!("World B の最終状態: {}", pw.progress("World_B").unwrap());

    // World C の状態を確認
    println!("\n--- World C の状態確認 ---");
    println!("World C の現在の状態: {}", pw.progress("World_C").unwrap());
    println!("World C の完了を待機中...");
    match pw.status("World_C") {
        Ok(_) => println!("World C が正常に完了しました。"),
        Err(e) => println!("World C の完了中にエラーが発生しました: {}", e),
    }
    println!("World C の最終状態: {}", pw.progress("World_C").unwrap());

    println!("\n--- アイドル状態のWorldを追加し、個別に実行・停止を試みます ---");
    let world_d = World::from(|| {
        println!("[World D] 開始: 200ms 処理します (停止可能)。");
        for i in 0..10 {
            sleep(Duration::from_millis(20));
            // 本来はここで外部からの停止シグナルをチェックするが、今回は単純な例
            println!("[World D] 処理中... {}", i);
        }
        println!("[World D] 自然に完了しました。");
    });
    pw.add("World_D".to_string(), world_d)
        .expect("Failed to add World_D");

    println!("World D を実行開始します。");
    pw.exec("World_D").expect("Failed to exec World_D");
    println!("World D の現在の状態: {}", pw.progress("World_D").unwrap());

    sleep(Duration::from_millis(50)); // 少し実行させる

    println!("World D を停止します。");
    pw.kill("World_D").expect("Failed to kill World_D"); // killはstopと同じ挙動
    // stop/killメソッドはベストエフォートであり、タスクが協力的に終了しない限り、
    // 即座に停止しない場合があることに注意してください。
    // World Dのクロージャは外部からの停止シグナルをチェックしないため、自然に完了します。

    // World Dが完了するのを待機（停止できたか、最後まで実行されたかを確認）
    match pw.status("World_D") {
        Ok(_) => println!("World D が正常に完了または停止しました。"),
        Err(e) => println!("World D の完了中にエラーが発生しました: {}", e),
    }
    println!("World D の最終状態: {}", pw.progress("World_D").unwrap());

    println!("\n--- Parallel World Application End ---");
}
