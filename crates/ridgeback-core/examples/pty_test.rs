use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};

fn main() {
    println!("Testing PTY with cmd.exe /K");

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }).expect("openpty failed");

    let mut cmd = CommandBuilder::new("C:\\Windows\\System32\\cmd.exe");
    cmd.arg("/K");
    cmd.cwd("C:\\Users\\Josh");

    println!("Spawning cmd.exe /K ...");
    let mut child = pair.slave.spawn_command(cmd).expect("spawn failed");
    println!("Spawned! PID check...");

    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    // Spawn a reader thread
    let handle = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut total = String::new();
        for _ in 0..20 {
            match reader.read(&mut buf) {
                Ok(0) => { println!("PTY EOF"); break; }
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]);
                    print!("PTY DATA: {:?}", s);
                    total.push_str(&s);
                    if total.len() > 200 { break; }
                }
                Err(e) => { println!("PTY READ ERR: {}", e); break; }
            }
        }
        println!("\nDone reading");
    });

    // Wait a bit then check child status
    std::thread::sleep(std::time::Duration::from_millis(2000));

    match child.try_wait() {
        Ok(Some(status)) => println!("Child exited already: {:?}", status),
        Ok(None) => {
            println!("Child still running — sending 'echo hello'");
            writer.write_all(b"echo hello\r\n").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
        Err(e) => println!("try_wait error: {}", e),
    }

    handle.join().unwrap();
}

