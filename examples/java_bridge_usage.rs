
use launcher_core::java_bridge::{JavaBridge, BridgeError};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bridge = JavaBridge::new_default()
        .ok_or("mclauncher-api JAR not found")?;

    println!("=== Listing Minecraft Versions ===");
    match bridge.list_versions() {
        Ok(versions) => {
            println!("Found {} versions", versions.len());
            for v in versions.iter().take(10) {
                println!("  {}", v);
            }
        }
        Err(BridgeError::JvmExit(code, stderr)) => {
            eprintln!("JVM exited with code {}: {}", code, stderr);
        }
        Err(e) => eprintln!("Error: {}", e),
    }

    println!("\n=== Latest Version ===");
    match bridge.latest_version() {
        Ok(latest) => println!("Latest release: {}", latest),
        Err(e) => eprintln!("Error: {}", e),
    }

    println!("\n=== Login (Offline) ===");
    match bridge.login("Steve", "", None) {
        Ok(session) => {
            println!("Authenticated as: {}", session.username);
            println!("User type: {}", session.user_type);
            println!("Session is legacy: {}", session.is_legacy());
            println!("Session is Microsoft: {}", session.is_microsoft());
        }
        Err(e) => eprintln!("Error: {}", e),
    }

    println!("\n=== Build Launch Command ===");
    match bridge.build_launch_command(
        "1.21.1",
        "test_token",
        "12345678-1234-1234-1234-123456789abc",
        "Steve",
    ) {
        Ok(cmd) => {
            println!("Launch command ({}) {:?}", cmd.len(), cmd);
        }
        Err(e) => eprintln!("Error: {}", e),
    }

    Ok(())
}
