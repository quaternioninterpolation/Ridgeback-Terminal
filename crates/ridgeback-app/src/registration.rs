/// OS-level registration: context menus, file associations, Quick Actions.
///
/// Run `ridgeback --register` to install, `ridgeback --unregister` to remove.

use anyhow::Result;
use std::path::Path;

pub fn register() -> Result<()> {
    let exe = std::env::current_exe()?;
    platform::register(&exe)
}

pub fn unregister() -> Result<()> {
    platform::unregister()
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn register(exe: &Path) -> Result<()> {
        install_quick_action()?;
        force_lsregister(exe);
        eprintln!("Ridgeback: registered Quick Action and refreshed Launch Services.");
        eprintln!("  Quick Action installed to: {}", workflow_path().display());
        eprintln!("  It will appear in Finder's right-click menu for folders after");
        eprintln!("  logging out and back in, or restarting the Finder.");
        Ok(())
    }

    pub fn unregister() -> Result<()> {
        remove_quick_action()?;
        eprintln!("Ridgeback: removed Quick Action.");
        Ok(())
    }

    fn home_dir() -> std::path::PathBuf {
        std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
    }

    fn workflow_path() -> std::path::PathBuf {
        home_dir().join("Library/Services/New Ridgeback Terminal Here.workflow")
    }

    fn install_quick_action() -> Result<()> {
        let contents = workflow_path().join("Contents");
        std::fs::create_dir_all(&contents)?;
        std::fs::write(contents.join("Info.plist"), WORKFLOW_INFO_PLIST)?;
        std::fs::write(contents.join("document.wflow"), DOCUMENT_WFLOW)?;
        Ok(())
    }

    fn remove_quick_action() -> Result<()> {
        let wf = workflow_path();
        if wf.exists() {
            std::fs::remove_dir_all(&wf)?;
        }
        Ok(())
    }

    /// Ask Launch Services to re-index the .app bundle so document type and
    /// URL-scheme registrations in Info.plist take effect immediately.
    fn force_lsregister(exe: &Path) {
        // Walk up from the binary to find the enclosing .app bundle.
        let app = exe.ancestors()
            .find(|p| p.extension().map(|e| e == "app").unwrap_or(false));

        let lsregister = concat!(
            "/System/Library/Frameworks/CoreServices.framework/",
            "Frameworks/LaunchServices.framework/Support/lsregister"
        );

        if let Some(app_path) = app {
            let _ = std::process::Command::new(lsregister)
                .args(["-f", &app_path.to_string_lossy()])
                .status();
        }
    }

    // ── Automator Quick Action — Info.plist ───────────────────────────────────
    static WORKFLOW_INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>New Ridgeback Terminal Here</string>
            </dict>
            <key>NSMessage</key>
            <string>runWorkflowAsService</string>
        </dict>
    </array>
</dict>
</plist>
"#;

    // ── Automator Quick Action — document.wflow ───────────────────────────────
    // Runs a shell script that opens Ridgeback at each selected folder path.
    // inputMethod=1 → pass input as arguments (one path per call).
    static DOCUMENT_WFLOW: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>AMApplicationBuild</key>
    <string>521.1</string>
    <key>AMApplicationVersion</key>
    <string>2.10</string>
    <key>AMDocumentVersion</key>
    <string>2</string>
    <key>actions</key>
    <array>
        <dict>
            <key>action</key>
            <dict>
                <key>AMAccepts</key>
                <dict>
                    <key>Container</key>
                    <string>List</string>
                    <key>Optional</key>
                    <true/>
                    <key>Types</key>
                    <array>
                        <string>com.apple.cocoa.path</string>
                    </array>
                </dict>
                <key>AMActionVersion</key>
                <string>2.0.3</string>
                <key>AMParameterProperties</key>
                <dict>
                    <key>COMMAND_STRING</key><dict/>
                    <key>CheckedForUserDefaultShell</key><dict/>
                    <key>inputMethod</key><dict/>
                    <key>shell</key><dict/>
                    <key>source</key><dict/>
                </dict>
                <key>AMProvides</key>
                <dict>
                    <key>Container</key>
                    <string>List</string>
                    <key>Types</key>
                    <array>
                        <string>com.apple.cocoa.string</string>
                    </array>
                </dict>
                <key>ActionBundlePath</key>
                <string>/System/Library/Automator/Run Shell Script.action</string>
                <key>ActionName</key>
                <string>Run Shell Script</string>
                <key>ActionParameters</key>
                <dict>
                    <key>COMMAND_STRING</key>
                    <string>for f in "$@"; do
    open -a "Ridgeback" --args --working-directory "$f"
done</string>
                    <key>CheckedForUserDefaultShell</key>
                    <true/>
                    <key>inputMethod</key>
                    <integer>1</integer>
                    <key>shell</key>
                    <string>/bin/bash</string>
                    <key>source</key>
                    <string></string>
                </dict>
                <key>BundleIdentifier</key>
                <string>com.apple.RunShellScript</string>
                <key>CFBundleVersion</key>
                <string>2.0.3</string>
                <key>CanShowSelectedItemsWhenRun</key>
                <false/>
                <key>CanShowWhenRun</key>
                <true/>
                <key>Category</key>
                <array>
                    <string>AMCategoryUtilities</string>
                </array>
                <key>Class Name</key>
                <string>RunShellScriptAction</string>
                <key>InputUUID</key>
                <string>B6D14C13-CB89-4B31-BBF9-88EA05C7E4DA</string>
                <key>Keywords</key>
                <array>
                    <string>Shell</string>
                    <string>Script</string>
                    <string>Command</string>
                    <string>Run</string>
                    <string>Unix</string>
                </array>
                <key>OutputUUID</key>
                <string>A5C09A61-9CD3-4B8E-9862-A0D47B2BBA99</string>
                <key>UUID</key>
                <string>F02AE9F5-2B3F-4D5E-8A6C-9B0C1D2E3F4A</string>
                <key>UnlockLabel</key>
                <string>Run Shell Script</string>
                <key>arguments</key>
                <dict>
                    <key>0</key>
                    <dict>
                        <key>default value</key>
                        <integer>0</integer>
                        <key>name</key>
                        <string>inputMethod</string>
                        <key>required</key>
                        <string>0</string>
                        <key>type</key>
                        <string>0</string>
                        <key>uuid</key>
                        <string>0</string>
                    </dict>
                </dict>
                <key>isViewVisible</key>
                <true/>
                <key>location</key>
                <string>309.000000:253.000000</string>
                <key>nibPath</key>
                <string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/English.lproj/main.nib</string>
            </dict>
            <key>isViewVisible</key>
            <true/>
        </dict>
    </array>
    <key>connectors</key>
    <dict/>
    <key>workflowMetaData</key>
    <dict>
        <key>serviceInputTypeIdentifier</key>
        <string>com.apple.Automator.fileSystemObject.folder</string>
        <key>serviceOutputTypeIdentifier</key>
        <string>com.apple.Automator.nothing</string>
        <key>serviceProcessesInput</key>
        <integer>0</integer>
        <key>workflowTypeIdentifier</key>
        <string>com.apple.Automator.servicesMenu</string>
    </dict>
</dict>
</plist>
"#;
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use winreg::enums::*;
    use winreg::RegKey;

    pub fn register(exe: &Path) -> Result<()> {
        let exe_str = exe.to_string_lossy();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)?;

        // ── Right-click on folder → "Open in Ridgeback Terminal" ─────────────
        let (key, _) = classes.create_subkey(r"Directory\shell\Ridgeback")?;
        key.set_value("", &"Open in Ridgeback Terminal")?;
        key.set_value("Icon", &format!("{},0", exe_str))?;
        let (cmd, _) = key.create_subkey("command")?;
        cmd.set_value("", &format!("\"{}\" --working-directory \"%1\"", exe_str))?;

        // ── Right-click on folder background → "Open in Ridgeback Terminal" ──
        let (key, _) = classes.create_subkey(r"Directory\Background\shell\Ridgeback")?;
        key.set_value("", &"Open in Ridgeback Terminal")?;
        key.set_value("Icon", &format!("{},0", exe_str))?;
        let (cmd, _) = key.create_subkey("command")?;
        cmd.set_value("", &format!("\"{}\" --working-directory \"%V\"", exe_str))?;

        // ── Drive root context menu ───────────────────────────────────────────
        let (key, _) = classes.create_subkey(r"Drive\shell\Ridgeback")?;
        key.set_value("", &"Open in Ridgeback Terminal")?;
        key.set_value("Icon", &format!("{},0", exe_str))?;
        let (cmd, _) = key.create_subkey("command")?;
        cmd.set_value("", &format!("\"{}\" --working-directory \"%1\"", exe_str))?;

        // ── Shell-script file associations ────────────────────────────────────
        for ext in &[".sh", ".bash", ".zsh", ".fish", ".command", ".ps1"] {
            register_file_ext(&classes, ext, &exe_str)?;
        }

        eprintln!("Ridgeback: registered context menus and file associations.");
        Ok(())
    }

    fn register_file_ext(classes: &RegKey, ext: &str, exe_str: &str) -> Result<()> {
        let prog_id = "Ridgeback.ShellScript";

        let (ext_key, _) = classes.create_subkey(ext)?;
        ext_key.set_value("", &prog_id)?;

        let (prog_key, _) = classes.create_subkey(prog_id)?;
        prog_key.set_value("", &"Shell Script")?;

        let (open_cmd, _) = prog_key.create_subkey(r"shell\open\command")?;
        open_cmd.set_value("", &format!("\"{}\" \"%1\"", exe_str))?;

        Ok(())
    }

    pub fn unregister() -> Result<()> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)?;

        for key in &[
            r"Directory\shell\Ridgeback",
            r"Directory\Background\shell\Ridgeback",
            r"Drive\shell\Ridgeback",
            "Ridgeback.ShellScript",
        ] {
            let _ = classes.delete_subkey_all(key);
        }
        // Remove file-ext keys only if they still point to our prog_id
        for ext in &[".sh", ".bash", ".zsh", ".fish", ".command", ".ps1"] {
            if let Ok(k) = classes.open_subkey(ext) {
                let val: String = k.get_value("").unwrap_or_default();
                if val == "Ridgeback.ShellScript" {
                    let _ = classes.delete_subkey_all(ext);
                }
            }
        }

        eprintln!("Ridgeback: removed context menus and file associations.");
        Ok(())
    }
}

// ── Linux ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    pub fn register(exe: &Path) -> Result<()> {
        let apps_dir = desktop_apps_dir()?;
        std::fs::create_dir_all(&apps_dir)?;

        let desktop_path = apps_dir.join("ridgeback.desktop");
        std::fs::write(&desktop_path, desktop_file_content(exe))?;

        // Refresh the application database
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps_dir)
            .status();

        // Register as default handler for common shell MIME types
        for mime in &[
            "application/x-sh",
            "application/x-shellscript",
            "text/x-sh",
            "application/x-zsh",
            "application/x-bash",
        ] {
            let _ = std::process::Command::new("xdg-mime")
                .args(["default", "ridgeback.desktop", mime])
                .status();
        }

        eprintln!("Ridgeback: installed desktop entry to {}", desktop_path.display());
        eprintln!("  Registered as default handler for shell script MIME types.");
        eprintln!("  For 'Open Terminal Here' in Nautilus, install the nautilus-open-terminal");
        eprintln!("  package or configure your file manager's custom action to run:");
        eprintln!("    ridgeback --working-directory %d");
        Ok(())
    }

    pub fn unregister() -> Result<()> {
        let apps_dir = desktop_apps_dir()?;
        let desktop_path = apps_dir.join("ridgeback.desktop");
        if desktop_path.exists() {
            std::fs::remove_file(&desktop_path)?;
        }
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps_dir)
            .status();
        eprintln!("Ridgeback: removed desktop entry.");
        Ok(())
    }

    fn desktop_apps_dir() -> Result<std::path::PathBuf> {
        // Respect XDG_DATA_HOME, fall back to ~/.local/share
        let base = std::env::var("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("HOME")
                    .map(|h| std::path::PathBuf::from(h).join(".local/share"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("/usr/local/share"))
            });
        Ok(base.join("applications"))
    }

    fn desktop_file_content(exe: &Path) -> String {
        let exe_str = exe.display();
        format!(
            r#"[Desktop Entry]
Version=1.0
Type=Application
Name=Ridgeback
GenericName=Terminal Emulator
Comment=A GPU-accelerated terminal emulator
Exec={exe} --working-directory %f
Icon=ridgeback
Terminal=false
Categories=System;TerminalEmulator;Utility;
MimeType=application/x-sh;application/x-shellscript;text/x-sh;application/x-zsh;application/x-bash;
StartupNotify=true
Keywords=terminal;emulator;shell;console;

[Desktop Action new-window]
Name=New Terminal Window
Exec={exe}
"#,
            exe = exe_str
        )
    }
}

// ── Unsupported platforms ─────────────────────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    use super::*;

    pub fn register(_exe: &Path) -> Result<()> {
        anyhow::bail!("OS registration is not supported on this platform")
    }

    pub fn unregister() -> Result<()> {
        anyhow::bail!("OS registration is not supported on this platform")
    }
}
