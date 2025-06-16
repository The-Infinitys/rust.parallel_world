use parallel_world::{ParallelWorlds, World, WorldStatus};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    println!("--- Parallel World Application Start ---");

    // 1. ParallelWorlds インスタンスを作成
    let pw = ParallelWorlds::new();

    // 2. 実行するWorld（タスク）を定義し、追加する

    // World A: 成功するタスク (引数なし、整数を返す)
    let world_a = World::from(|| {
        println!("[World A] 開始: 50ms 処理します。");
        sleep(Duration::from_millis(50));
        println!("[World A] 完了。");
        // 戻り値
        100
    });
    pw.add("World_A".to_string(), world_a)
        .expect("Failed to add World_A");

    // World B: 少し長い処理をするタスク (引数なし、文字列を返す)
    let world_b = World::from(|| {
        println!("[World B] 開始: 100ms 処理します。");
        sleep(Duration::from_millis(100));
        println!("[World B] 完了。");
        // 戻り値
        "Hello from World B!".to_string()
    });
    pw.add("World_B".to_string(), world_b)
        .expect("Failed to add World_B");

    // World C: 失敗するタスク（パニックを発生させる、戻り値は仮の型）
    let world_c = World::from(|| {
        println!("[World C] 開始: パニックを発生させます。");
        sleep(Duration::from_millis(20));
        panic!("[World C] エラー発生！");
        // unreachable! なので、どんな型でもOKだが型推論のためにダミー値を置く
        false
    });
    pw.add("World_C".to_string(), world_c)
        .expect("Failed to add World_C");

    // World E: 引数を取り、それを使って計算するタスク (引数あり、タプルを返す)
    // クロージャで引数をキャプチャする形式
    let x = 10;
    let y = 20;
    let world_e = World::from(move || {
        // move キーワードでxとyをキャプチャ
        println!("[World E] 開始: 引数を使って計算します。 x={}, y={}", x, y);
        sleep(Duration::from_millis(70));
        let sum = x + y;
        let product = x * y;
        println!("[World E] 完了。");
        (sum, product) // タプルを返す
    });
    pw.add("World_E".to_string(), world_e)
        .expect("Failed to add World_E");

    println!("\n--- 全てのWorldを実行開始します ---");
    pw.start_all();

    // World A の完了を待機し、結果を取得 (i32型を期待)
    println!("\n--- World A の状態確認 ---");
    println!("World A の現在の状態: {}", pw.progress("World_A").unwrap());
    match pw.status::<i32>("World_A") {
        // status::<i32>でi32型を期待
        Ok(result) => println!("World A が正常に完了しました。戻り値: {}", result),
        Err(e) => println!("World A の完了中にエラーが発生しました: {}", e),
    }
    println!("World A の最終状態: {}", pw.progress("World_A").unwrap());

    // World B の完了を待機し、結果を取得 (String型を期待)
    println!("\n--- World B の状態確認 ---");
    println!("World B の現在の状態: {}", pw.progress("World_B").unwrap());
    match pw.status::<String>("World_B") {
        // status::<String>でString型を期待
        Ok(result) => println!("World B が正常に完了しました。戻り値: \"{}\"", result),
        Err(e) => println!("World B の完了中にエラーが発生しました: {}", e),
    }
    println!("World B の最終状態: {}", pw.progress("World_B").unwrap());

    // World C の完了を待機し、エラーを取得 (bool型を期待するが、失敗するのでErrが返る)
    println!("\n--- World C の状態確認 ---");
    println!("World C の現在の状態: {}", pw.progress("World_C").unwrap());
    match pw.status::<bool>("World_C") {
        // status::<bool>でbool型を期待
        Ok(result) => println!("World C が正常に完了しました。戻り値: {}", result),
        Err(e) => println!("World C の完了中にエラーが発生しました: {}", e),
    }
    println!("World C の最終状態: {}", pw.progress("World_C").unwrap());

    // World E の完了を待機し、結果を取得 (タプル型を期待)
    println!("\n--- World E の状態確認 ---");
    println!("World E の現在の状態: {}", pw.progress("World_E").unwrap());
    type WorldEResult = (i32, i32); // タプル型にエイリアスを付けて可読性向上
    match pw.status::<WorldEResult>("World_E") {
        // status::<WorldEResult>でタプル型を期待
        Ok((sum, product)) => println!(
            "World E が正常に完了しました。戻り値: 和={}, 積={}",
            sum, product
        ),
        Err(e) => println!("World E の完了中にエラーが発生しました: {}", e),
    }
    println!("World E の最終状態: {}", pw.progress("World_E").unwrap());

    // World D: 協調的に停止できるタスク（再利用）
    let stop_flag_d = Arc::new(Mutex::new(false));
    let stop_flag_d_clone = Arc::clone(&stop_flag_d);

    let world_d = World::from(move || {
        println!("[World D] 開始: 20ms間隔で処理します (停止可能)。");
        let mut i = 0;
        loop {
            if *stop_flag_d_clone.lock().unwrap() {
                println!("[World D] 停止シグナルを受信しました。終了します。");
                break i; // 停止した時点のiを返す
            }
            println!("[World D] 処理中... {}", i);
            sleep(Duration::from_millis(20));
            i += 1;
            if i > 50 {
                println!("[World D] 処理回数の上限に達しました。自然に完了します。");
                break i;
            }
        }
    });
    pw.add("World_D".to_string(), world_d)
        .expect("Failed to add World_D");

    println!("\nWorld D を実行開始します。");
    pw.exec("World_D").expect("Failed to exec World_D");
    println!("World D の現在の状態: {}", pw.progress("World_D").unwrap());

    sleep(Duration::from_millis(50)); // 少し実行させる

    println!("World D を停止シグナルで停止します。");
    *stop_flag_d.lock().unwrap() = true; // 停止フラグを設定

    println!("World D の完了を待機します。");
    match pw.status::<i32>("World_D") {
        // World Dはi32を返す
        Ok(count) => println!(
            "World D が協調的に停止し、完了しました。カウント: {}",
            count
        ),
        Err(e) => println!("World D の完了中にエラーが発生しました: {}", e),
    }
    println!("World D の最終状態: {}", pw.progress("World_D").unwrap());

    println!("\n--- Parallel World Application End ---");
}
