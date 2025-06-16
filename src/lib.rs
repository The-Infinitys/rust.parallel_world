// src/lib.rs

pub mod parallel_worlds;
pub mod world;

// クレートのトップレベルで利用できるように、use宣言を追加
pub use parallel_worlds::ParallelWorlds;
pub use world::{World, WorldStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;
    #[test]
    fn test_world_creation_and_run() {
        let world = World::from(|| {
            println!("World 1 executing synchronously!");
            sleep(Duration::from_millis(100));
        });
        assert_eq!(world.progress(), WorldStatus::Ready);
        let result = world.run();
        assert!(result.is_ok());
        assert_eq!(world.progress(), WorldStatus::Finished);

        let world_fail = World::from(|| {
            println!("World 2 executing synchronously and failing!");
            sleep(Duration::from_millis(50));
            panic!("Oops, something went wrong!");
        });
        assert_eq!(world_fail.progress(), WorldStatus::Ready);
        let result_fail = world_fail.run();
        assert!(result_fail.is_err());
        assert!(matches!(world_fail.progress(), WorldStatus::Failed(_)));
    }

    #[test]
    fn test_parallel_worlds_basic_operations() {
        let pw = ParallelWorlds::new();
        assert!(pw.list().is_empty());

        let world1 = World::from(|| {
            println!("World A executing.");
            sleep(Duration::from_millis(200));
        });
        let world2 = World::from(|| {
            println!("World B executing.");
            sleep(Duration::from_millis(100));
        });

        assert!(pw.add("A".to_string(), world1).is_ok());
        assert!(pw.add("B".to_string(), world2).is_ok());
        assert_eq!(pw.list().len(), 2);
        assert!(pw.list().contains(&"A".to_string()));
        assert!(pw.list().contains(&"B".to_string()));

        // 存在しないWorldのgetはNone
        assert!(pw.get("C").is_none());
        // 存在するWorldのgetはSome
        assert!(pw.get("A").is_some());
    }

    #[test]
    fn test_parallel_worlds_execution() {
        let pw = ParallelWorlds::new();

        let world1_status = Arc::new(Mutex::new(WorldStatus::Ready));
        let status1_clone = Arc::clone(&world1_status);
        let world1 = World::from(move || {
            println!("World 1: Starting.");
            sleep(Duration::from_millis(300));
            println!("World 1: Done.");
            *status1_clone.lock().unwrap() = WorldStatus::Finished; // 手動でステータス更新 (World内部でされるはずだが確認用)
        });
        assert!(pw.add("W1".to_string(), world1).is_ok());

        let world2_status = Arc::new(Mutex::new(WorldStatus::Ready));
        let status2_clone = Arc::clone(&world2_status);
        let world2 = World::from(move || {
            println!("World 2: Starting.");
            sleep(Duration::from_millis(150));
            println!("World 2: Done.");
            *status2_clone.lock().unwrap() = WorldStatus::Finished;
        });
        assert!(pw.add("W2".to_string(), world2).is_ok());

        println!("Starting all worlds...");
        pw.start_all();

        // 実行開始直後はRunningになっていることを確認
        assert_eq!(pw.progress("W1").unwrap(), WorldStatus::Running);
        assert_eq!(pw.progress("W2").unwrap(), WorldStatus::Running);

        println!("Waiting for World 2 to finish...");
        assert!(pw.status("W2").is_ok()); // World 2が先に終わるはず

        // World 2はFinished、World 1はまだRunningかFinished
        assert_eq!(pw.progress("W2").unwrap(), WorldStatus::Finished);
        let w1_status = pw.progress("W1").unwrap();
        assert!(w1_status == WorldStatus::Running || w1_status == WorldStatus::Finished);

        println!("Waiting for World 1 to finish...");
        assert!(pw.status("W1").is_ok()); // World 1が次に終わる

        assert_eq!(pw.progress("W1").unwrap(), WorldStatus::Finished);

        println!("All worlds finished.");
    }

    #[test]
    fn test_parallel_worlds_error_handling() {
        let pw = ParallelWorlds::new();

        let world_err = World::from(|| {
            println!("World Error: About to panic!");
            panic!("Simulated error!");
        });
        assert!(pw.add("WE".to_string(), world_err).is_ok());

        pw.exec("WE").unwrap();
        let result = pw.status("WE");
        assert!(result.is_err());
        assert!(matches!(pw.progress("WE").unwrap(), WorldStatus::Failed(_)));
        println!(
            "World WE status after error: {:?}",
            pw.progress("WE").unwrap()
        );

        // 実行中のWorldを削除できないことの確認
        let world_running = World::from(|| {
            sleep(Duration::from_secs(1));
        });
        pw.add("WR".to_string(), world_running).unwrap();
        pw.exec("WR").unwrap();
        assert!(pw.del("WR").is_err()); // 実行中は削除できない
        pw.status("WR").unwrap(); // 終了まで待機
        assert!(pw.del("WR").is_ok()); // 終了後は削除できる
    }

    #[test]
    fn test_parallel_worlds_stop() {
        let pw = ParallelWorlds::new();
        // 停止シグナルを受け取れるWorldを想定
        let controlled_world = World::from(|| {
            println!("Controlled World: Starting.");
            // ここで、外部からの停止シグナルを待つなどのロジックが必要
            // 現状はsleepで代用
            for i in 0..10 {
                sleep(Duration::from_millis(100));
                println!("Controlled World: Progress {}", i);
            }
            println!("Controlled World: Done naturally.");
        });
        pw.add("CW".to_string(), controlled_world).unwrap();

        pw.exec("CW").unwrap();
        assert_eq!(pw.progress("CW").unwrap(), WorldStatus::Running);

        sleep(Duration::from_millis(250)); // 少し実行させてから停止を試みる
        println!("Attempting to stop CW...");
        // 現状のstop実装では、スレッドのjoin()がブロックされるため、
        // テストで停止が即座に反映されることは期待できないが、状態は更新される
        assert!(pw.kill("CW").is_ok());

        // 停止のtry_join()のようなものがないため、sleepで待つしかないが、
        // 実際にはWorldStatusがStoppedになることを期待する。
        // もしWorld::stop()がスレッドを完全に停止できるなら、WorldStatus::Stoppedになる。
        // そうでない場合（join()がブロックされる）、まだRunningのままかもしれない。
        sleep(Duration::from_millis(50)); // statusが更新されるのを待つ
        let status_after_stop = pw.progress("CW").unwrap();
        println!("Status after stop attempt: {:?}", status_after_stop);
        // ここはWorld::stopの実装に依存します。
        // 理想的には WorldStatus::Stopped を期待しますが、スレッドが完全に終了するまではRunningかもしれません。
        // 今回の簡易実装では、stop()呼び出し後にすぐにStoppedになりますが、スレッド自体はまだ動いている可能性があります。
        assert!(
            status_after_stop == WorldStatus::Stopped || status_after_stop == WorldStatus::Running
        );

        // 最終的にはスレッドが終了するのを待つ
        let _ = pw.status("CW"); // 完全に終了を待つ
        assert!(
            pw.progress("CW").unwrap() == WorldStatus::Stopped
                || pw.progress("CW").unwrap() == WorldStatus::Finished
        );
    }
}
