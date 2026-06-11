use std::path::{PathBuf};
use std::process::{Child, Command, Stdio};
use walkdir::WalkDir;

#[cfg(windows)]
use winapi::um::tlhelp32::{CreateToolhelp32Snapshot, Thread32First, Thread32Next, THREADENTRY32, TH32CS_SNAPTHREAD};
#[cfg(windows)]
use winapi::um::processthreadsapi::{OpenThread, SuspendThread};
#[cfg(windows)]
use winapi::um::handleapi::CloseHandle;
#[cfg(windows)]
use winapi::shared::minwindef::FALSE;
#[cfg(windows)]
use winapi::um::winnt::THREAD_SUSPEND_RESUME;

pub fn find_files(search_dir: PathBuf, target_exec: String) -> Vec<PathBuf> {
    WalkDir::new(search_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.file_name().to_string_lossy() == target_exec
        })
        .map(|entry| entry.into_path())
        .collect()
}

pub fn create_paused_process(game_path: PathBuf, target_exec: &str) -> Option<u32> {
    println!("Starting {target_exec}...");

    let executables = find_files(game_path, target_exec.to_string());

    match executables.len() {
        0 => return None,
        1 => {}
        _ => {
            eprintln!(
                "Too many {target_exec} found: {:?}",
                executables
            );
            return None;
        }
    }

    let executable = &executables[0];

    // Again i have no idea what the point of this is!
    let process = start_process(
        executable,
        None,
        &[("OPENSSL_ia32cap", "~0x20000000")],
    ).ok()?;

    println!("Started paused {target_exec}: {:?}", process);

    let pid = process.id();

    suspend(pid);

    Some(pid)
}

pub fn start_process(executable: &PathBuf, args: Option<&[String]>, envs: &[(&str, &str)]) -> std::io::Result<Child> {
    let mut cmd = Command::new(executable);
    if let Some(dir) = executable.parent() {
        if dir.exists() {
            cmd.current_dir(dir);
        }
    }

    if let Some(a) = args {
        cmd.args(a);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    for (k, v) in envs {
        cmd.env(k, v);
    }

    cmd.spawn()
}

// suspends all threads
pub fn suspend(pid: u32) -> bool {
    #[cfg(windows)]
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == winapi::um::handleapi::INVALID_HANDLE_VALUE {
            return false;
        }

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;

        let mut success = false;
        if Thread32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32OwnerProcessID == pid {
                    let thread_handle = OpenThread(THREAD_SUSPEND_RESUME, FALSE, entry.th32ThreadID);
                    if !thread_handle.is_null() {
                        if SuspendThread(thread_handle) != u32::MAX {
                            success = true;
                        }
                        CloseHandle(thread_handle);
                    }
                }

                if Thread32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
        success
    }
    #[cfg(not(windows))]
    {
        false
    }
}
