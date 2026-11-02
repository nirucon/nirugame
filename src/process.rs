//! Process isolation for launchers and optional helpers.
//!
//! NIRUWM keeps a session's programs in one process group. A child that opens
//! the controlling terminal from a background group receives SIGTTIN, which
//! stops every process in that group, including the window manager. Every
//! external program is therefore placed in its own session and loses the
//! controlling terminal before exec.

#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::process::Command;

/// Start in a new session, so terminal job control can never stop NIRUWM or
/// another NIRUGAME child. Errors are reported to the parent before exec.
pub fn detach_terminal(command: &mut Command) {
    #[cfg(target_os = "linux")]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = command;
}

/// Ask the kernel to terminate the child if NIRUGAME disappears. This is used
/// for the SID player only; launched games must survive the launcher.
#[cfg(target_os = "linux")]
pub fn die_with_parent(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            if libc::getppid() == 1 {
                libc::_exit(1);
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
pub fn die_with_parent(command: &mut Command) {
    let _ = command;
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn detached_child_owns_a_new_session() {
        let mut command = Command::new("sleep");
        command.arg("2").stdin(Stdio::null());
        detach_terminal(&mut command);
        let mut child = command.spawn().unwrap();
        let pid = child.id() as i32;
        unsafe {
            assert_eq!(libc::getsid(pid), pid);
            assert_eq!(libc::getpgid(pid), pid);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}
