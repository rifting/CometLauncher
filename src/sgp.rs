use std::ffi::CString;
use std::path::PathBuf;
use std::ptr::null_mut;
use crate::utils::{find_files, create_paused_process, start_process};
use std::io::{BufRead, BufReader};
use std::thread;
use crate::log_msg;

#[cfg(windows)]
use winapi::um::processthreadsapi::{OpenProcess, CreateRemoteThread};
#[cfg(windows)]
use winapi::um::libloaderapi::{GetProcAddress, GetModuleHandleA};
#[cfg(windows)]
use winapi::um::memoryapi::{VirtualAllocEx, WriteProcessMemory};
#[cfg(windows)]
use winapi::um::handleapi::CloseHandle;
#[cfg(windows)]
use winapi::um::winnt::{PROCESS_ALL_ACCESS, MEM_COMMIT, PAGE_READWRITE};

#[derive(Debug)]
pub struct GameInstance {
    pub version: String,
    pub game_pid: u32,
}

pub fn extract_game_version(game_path: &std::path::PathBuf) -> String {
    game_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn is_chapter_one(version: &str) -> bool {
    version
        .split('.')
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .map(|major| major < 10)
        .unwrap_or(true) // Just in case
}

#[cfg(windows)]
fn inject_dll(pid: u32, dll_path: &str) -> Result<(), String> {
    unsafe {
        let process = OpenProcess(PROCESS_ALL_ACCESS, 0, pid);
        if process.is_null() {
            return Err(format!("OpenProcess failed for pid {}", pid));
        }

        let kernel32 = GetModuleHandleA(CString::new("KERNEL32").map_err(|e| e.to_string())?.as_ptr());
        if kernel32.is_null() {
            CloseHandle(process);
            return Err("GetModuleHandleA failed".into());
        }

        let load_library = GetProcAddress(kernel32, CString::new("LoadLibraryA").map_err(|e| e.to_string())?.as_ptr());
        if load_library.is_null() {
            CloseHandle(process);
            return Err("GetProcAddress(LoadLibraryA) failed".into());
        }

        let dll_c = CString::new(dll_path).map_err(|e| e.to_string())?;
        let alloc = VirtualAllocEx(
            process,
            null_mut(),
            dll_c.as_bytes_with_nul().len(),
            MEM_COMMIT,
            PAGE_READWRITE,
        );
        if alloc.is_null() {
            CloseHandle(process);
            return Err("VirtualAllocEx failed".into());
        }

        let wrote = WriteProcessMemory(
            process,
            alloc,
            dll_c.as_ptr() as *const _,
            dll_c.as_bytes_with_nul().len(),
            null_mut(),
        );
        if wrote == 0 {
            CloseHandle(process);
            return Err("WriteProcessMemory failed".into());
        }

        let thread = CreateRemoteThread(
            process,
            null_mut(),
            0,
            Some(std::mem::transmute(load_library)),
            alloc,
            0,
            null_mut(),
        );
        if thread.is_null() {
            CloseHandle(process);
            return Err("CreateRemoteThread failed".into());
        }

        CloseHandle(thread);
        CloseHandle(process);
        Ok(())
    }
}

pub fn start_game_processes(
    game_path: PathBuf,
    shipping_exe: &str,
    launcher_exe: &str,
    eac_exe: &str,
    dll_paths: DllPaths,
    args: &[String],
) -> Option<GameInstance> {
    let version = extract_game_version(&game_path);
    let _launcher_pid = create_paused_process(game_path.clone(), launcher_exe);
    let _eac_pid = create_paused_process(game_path.clone(), eac_exe);

    // wyd if you got to this point
    let shipping = find_files(game_path.clone(), shipping_exe.to_string());
    if shipping.len() != 1 {
        log_msg!("Expected exactly one shipping exe, found: {:?}", shipping);
        return None;
    }

    // Delete this random dll for some reason?
    let aftermath = find_files(game_path.clone(), crate::consts::GFSDK_AFTERMATH_LIB_DLL.to_string());
    for dll in aftermath {
        let _ = std::fs::remove_file(&dll);
    }

    // NO idea what the purpose of this is.
    let mut child = start_process(&shipping[0], Some(args), &[("OPENSSL_ia32cap", "~0x20000000")]).ok()?;
    let pid = child.id();

    if let Some(auth) = dll_paths.auth.as_ref() {
        if let Err(e) = inject_dll(pid, &auth.display().to_string()) {
            log_msg!("Auth DLL injection failed! {}: {}", auth.display(), e);
        }
    }

    let console_clone = dll_paths.console.clone();
    let memory_clone = dll_paths.memory_leak.clone();

    // Check if it's chapter 1
    let version_clone = version.clone();
    
    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout);
        let console_c = console_clone.clone();
        let memory_c = memory_clone.clone();
        thread::spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let l = line.trim_end().to_string();
                        // Game logs
                        log_msg!("[GAME] {}", l);
                        if l.contains("[UOnlineAccountCommon::ContinueLoggingIn]") && l.contains("(Completed)") {
                            // Chapter 1 memory leak
                            if is_chapter_one(&version_clone) {
                                if let Some(mem) = &memory_c {
                                    if let Err(e) = inject_dll(pid, &mem.display().to_string()) {
                                        log_msg!("[COMET] Memory DLL injection failed {}: {}", mem.display(), e);
                                    } else {
                                        log_msg!("[COMET] Memory DLL injected successfully");
                                    }
                                }
                            }
                            if let Some(cons) = &console_c {
                                if let Err(e) = inject_dll(pid, &cons.display().to_string()) {
                                    log_msg!("[COMET] Console DLL injection failed {}: {}", cons.display(), e);
                                } else {
                                    log_msg!("[COMET] Console DLL injected successfully");
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr);
        thread::spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                    }
                    Err(_) => break,
                }
            }
        });
    }

    Some(GameInstance {
        version,
        game_pid: pid
    })
}

pub struct DllPaths {
    pub auth: Option<PathBuf>,
    pub console: Option<PathBuf>,
    pub memory_leak: Option<PathBuf>
}