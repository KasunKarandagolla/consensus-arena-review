use std::ffi::OsString;
use std::path::Path;
use std::process::{ExitStatus, Output};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;

const GIT_TIMEOUT_SECONDS: u64 = 120;
const MAX_OUTPUT_BYTES: usize = 128 * 1024;

async fn read_bounded<R>(reader: R) -> Result<Vec<u8>, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut reader = reader;
    let mut captured = Vec::with_capacity(MAX_OUTPUT_BYTES.min(8192));
    let mut chunk = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|error| format!("read git output: {error}"))?;
        if count == 0 {
            break;
        }
        let remaining = MAX_OUTPUT_BYTES.saturating_sub(captured.len());
        let to_copy = remaining.min(count);
        captured.extend_from_slice(&chunk[..to_copy]);
        truncated |= to_copy < count;
    }
    if truncated {
        captured.extend_from_slice(b"\n[git output truncated]\n");
    }
    Ok(captured)
}

async fn collect_output(task: &mut JoinHandle<Result<Vec<u8>, String>>) -> Result<Vec<u8>, String> {
    match tokio::time::timeout(Duration::from_secs(1), &mut *task).await {
        Ok(Ok(Ok(output))) => Ok(output),
        Ok(Ok(Err(error))) => {
            if !task.is_finished() {
                task.abort();
            }
            Err(error)
        }
        Ok(Err(error)) => Err(format!("join git output reader: {error}")),
        Err(_) => {
            if !task.is_finished() {
                task.abort();
            }
            Err("timed out collecting git output".to_string())
        }
    }
}

fn display_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn output(repo: &Path, args: &[&str]) -> Result<Output, String> {
    let owned = args.iter().map(OsString::from).collect::<Vec<_>>();
    output_owned(repo, &owned).await
}

pub async fn output_owned(repo: &Path, args: &[OsString]) -> Result<Output, String> {
    let mut child: Child = Command::new("git")
        .args(args)
        .current_dir(repo)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("run git {}: {error}", display_args(args)))?;

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("git stdout pipe was unavailable".to_string());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err("git stderr pipe was unavailable".to_string());
        }
    };
    let mut stdout_task = tokio::spawn(read_bounded(stdout));
    let mut stderr_task = tokio::spawn(read_bounded(stderr));

    let status: ExitStatus =
        match tokio::time::timeout(Duration::from_secs(GIT_TIMEOUT_SECONDS), child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(error)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(format!("wait for git {}: {error}", display_args(args)));
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(format!(
                    "git command timed out after {GIT_TIMEOUT_SECONDS}s: {}",
                    display_args(args)
                ));
            }
        };

    let stdout = match collect_output(&mut stdout_task).await {
        Ok(stdout) => stdout,
        Err(error) => {
            stderr_task.abort();
            return Err(format!(
                "collect git stdout {}: {error}",
                display_args(args)
            ));
        }
    };
    let stderr = collect_output(&mut stderr_task)
        .await
        .map_err(|error| format!("collect git stderr {}: {error}", display_args(args)))?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn captures_git_output_with_the_bounded_runner() {
        let result = output(Path::new("."), &["--version"])
            .await
            .expect("git version should run");
        assert!(result.status.success());
        assert!(String::from_utf8_lossy(&result.stdout).contains("git version"));
    }

    #[tokio::test]
    async fn captures_bounded_output_while_draining_the_pipe() {
        let (mut writer, reader) = tokio::io::duplex(MAX_OUTPUT_BYTES + 8192);
        let writer_task =
            tokio::spawn(async move { writer.write_all(&vec![b'x'; MAX_OUTPUT_BYTES + 1]).await });
        let captured = read_bounded(reader).await.expect("read output");
        writer_task
            .await
            .expect("writer task")
            .expect("write output");
        assert!(captured.starts_with(&vec![b'x'; MAX_OUTPUT_BYTES]));
        assert!(captured.ends_with(b"\n[git output truncated]\n"));
    }
}
