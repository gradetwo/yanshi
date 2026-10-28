//! **P-20：父进程死后，子进程须在 5 秒内自行退出**（paint-fix 期 7）。
//!
//! **做法**：测试拉起一个 `bash` 父进程，父进程再拉起 `yanshi-mcp`（stdin 是测试
//! 握着的管道，不写、不关 ⇒ 没有 EOF，子进程不会因 stdin 退出 ✓）；杀掉父进程
//! 后轮询子进程 pid，须在 5 秒内消失；父进程活着时子进程不得误退。
//! 只杀自己拉起的进程 ✓。
//!
//! **为什么是 `bash` + `< /dev/stdin`** ✗：`sh`/`bash` 在无作业控制时都会把后台任务的
//! stdin 指到 `/dev/null`（盒上实测：`ls -l /proc/self/fd/0` 后台任务得 `/dev/null` ✓）⇒
//! 不加显式重定向，子进程一启动就因 stdin EOF 干净退出，测的根本不是看门线程 ✗。
//! `< /dev/stdin` 只在 bash 里生效（dash 解析它时 fd 0 已是 `/dev/null` ✓，盒上实测）。
//!
//! **变异**：把 `serve` 里的看门线程关掉 ⇒ 子进程在 5 秒内不消失 ⇒ 红 ✓
//! （stdin 一直握着，EOF 那条退出路走不到 ✓）。
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn child_alive(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // stat 格式：`pid (comm) state ...`；Z = 僵尸（已死、待收尸 ⇒ 不算活着）。
    let state = stat
        .rsplit(')')
        .next()
        .and_then(|rest| rest.trim_start().chars().next());
    !matches!(state, None | Some('Z'))
}

fn kill_pid(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
}

/// 测试结束时若子进程还活着就杀掉（只杀自己拉起的 ✓）。
struct KillOnDrop(u32);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if child_alive(self.0) {
            kill_pid(self.0);
        }
    }
}

/// **测试结束即删掉临时目录** ✓（第 813 轮；`/tmp` 是 1.9 GB 的 tmpfs ✓，
/// 断言失败提前返回时也要删 ⇒ `check-tests-clean-temp` 护栏要的就是绑定式清理器）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn child_exits_within_five_seconds_after_its_parent_dies() {
    let bin = env!("CARGO_BIN_EXE_yanshi-mcp");
    let root = std::env::temp_dir().join(format!("yanshi_p20_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    let pid_file = root.join("child.pid");
    // 父进程：拉起 yanshi-mcp，记下它的 pid，然后 wait（一直活着）。
    // `bash` + `< /dev/stdin`：shell 会把后台任务的 stdin 指到 /dev/null ⇒
    // 不加这个显式重定向，子进程一启动就因 stdin EOF 退出，测不到看门线程。
    let mut parent: Child = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "{bin} --root {} --doc doc_p20 < /dev/stdin & echo $! > {}; wait",
            root.display(),
            pid_file.display()
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("拉起父进程");
    let parent_pid = parent.id();
    // stdin 写端一直握着：子进程收不到 EOF ⇒ 只能靠看门线程退出 ✓。
    let parent_stdin = parent.stdin.take().expect("父进程 stdin");
    // 等子进程 pid 落盘（最多 10 秒）。
    let deadline = Instant::now() + Duration::from_secs(10);
    let child_pid: u32 = loop {
        if let Ok(text) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = text.trim().parse::<u32>() {
                break pid;
            }
        }
        if Instant::now() > deadline {
            kill_pid(parent_pid);
            let _ = parent.wait();
            panic!("子进程 pid 10 秒没落盘");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _guard = KillOnDrop(child_pid);
    // 父进程活着时子进程不得误退（等 1 秒）。
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        child_alive(child_pid),
        "父进程活着时子进程不得误退（子进程 {child_pid} 已不在）"
    );
    // 杀父进程（只杀自己拉起的 ✓），子进程须在 5 秒内自行退出。
    let killed_at = Instant::now();
    kill_pid(parent_pid);
    let _ = parent.wait();
    let exited_within = loop {
        if !child_alive(child_pid) {
            break Some(killed_at.elapsed());
        }
        if killed_at.elapsed() > Duration::from_secs(10) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    // 收尾：关掉 stdin 写端（子进程若还活着会因 EOF 退出；看门线程正常时它早已不在 ✓）。
    drop(parent_stdin);
    let elapsed = exited_within.expect("杀父进程 10 秒后子进程还在（看门线程没起作用）");
    assert!(
        elapsed < Duration::from_secs(5),
        "子进程应在 5 秒内随父进程退出，实测 {} ms",
        elapsed.as_millis()
    );
}
