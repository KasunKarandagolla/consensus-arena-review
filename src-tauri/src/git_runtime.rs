use std::ffi::OsString;
use std::future::Future;
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

/// Run a bounded Git subprocess whose stdin is supplied by Arena and whose
/// stdout is consumed incrementally. The consumer must drain the stream; its
/// allocation policy remains with the caller. Child cleanup is explicit on
/// errors and timeout, including kill followed by wait.
pub async fn stream_with_input<T, F, Fut>(
    repo: &Path,
    args: &[&str],
    input: Vec<u8>,
    consume_stdout: F,
) -> Result<(ExitStatus, T), String>
where
    F: FnOnce(tokio::process::ChildStdout) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let owned = args.iter().map(OsString::from).collect::<Vec<_>>();
    let mut child = Command::new("git")
        .args(&owned)
        .current_dir(repo)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("run git {}: {error}", display_args(&owned)))?;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (Some(mut stdin), Some(stdout), Some(stderr)) = (stdin, stdout, stderr) else {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err("git streaming process pipes were unavailable".to_string());
    };

    let mut writer_task = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(&input)
            .await
            .map_err(|error| format!("write git stdin: {error}"))?;
        stdin
            .shutdown()
            .await
            .map_err(|error| format!("close git stdin: {error}"))
    });
    let mut stderr_task = tokio::spawn(read_bounded(stderr));
    let consume = consume_stdout(stdout);
    let operation = async {
        let ((), consumed) = tokio::try_join!(
            async {
                (&mut writer_task)
                    .await
                    .map_err(|error| format!("join git stdin writer: {error}"))??;
                Ok::<(), String>(())
            },
            consume,
        )?;
        let status = child
            .wait()
            .await
            .map_err(|error| format!("wait for git {}: {error}", display_args(&owned)))?;
        (&mut stderr_task)
            .await
            .map_err(|error| format!("join git stderr reader: {error}"))??;
        Ok::<(ExitStatus, T), String>((status, consumed))
    };

    match tokio::time::timeout(Duration::from_secs(GIT_TIMEOUT_SECONDS), operation).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => {
            writer_task.abort();
            stderr_task.abort();
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(error)
        }
        Err(_) => {
            writer_task.abort();
            stderr_task.abort();
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(format!(
                "git command timed out after {GIT_TIMEOUT_SECONDS}s: {}",
                display_args(&owned)
            ))
        }
    }
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
