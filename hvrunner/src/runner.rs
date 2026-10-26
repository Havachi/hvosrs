
use crate::config::Config;
use anyhow::Result;
use std::{io, process, time::Duration};
use thiserror::Error;
use wait_timeout::ChildExt;

pub fn run(config: Config, is_test: bool) -> Result<i32, RunError> {
	println!("Running kernel thought Qemu with profile: {}", if is_test {"test"} else {"live"});
	let mut run_command: Vec<_> = config
		.run_command
		.iter()
		.map(|arg| arg.replace("{}", &format!("{}", config.image_path.display().to_string())))
		.collect();
	if config.filesystem.is_some() {
		run_command.push("-drive".to_owned());
		run_command.push(format!("file={},format=raw,cache=writeback", config.filesystem.unwrap().clone()));
		
	}
	if is_test {
		if config.test_no_reboot {
			run_command.push("-no-reboot".to_owned());
		}
		if let Some(args) = config.test_args {
			run_command.extend(args);
		}
	} else if let Some(args) = config.run_args {
		run_command.extend(args);
	}

	println!("Running Qemu:");
	let runstr = run_command.join("\n\t");
	println!("{}", runstr);
	
	let mut command = process::Command::new(&run_command[0]);
	command.args(&run_command[1..]);

	let exit_code = if is_test {
		match handle_test_execution(&mut command, config.test_timeout.into())? {
			TestResult::Success => 0,
			TestResult::Failure => 1,
			TestResult::Timeout => {
				eprintln!("Test execution timed out");
				2
			},
		}
	} else {
		let status = command.status().map_err(|error| RunError::Io {
			context: IoErrorContext::QemuRunCommand { command: format!("{:?}", command) },
			error
		})?;
		status.code().unwrap_or(1)
	};
	Ok(exit_code)

}


enum TestResult {
    Success,
    Failure,
    Timeout,
}

fn handle_test_execution(command: &mut process::Command, timeout_secs: u64) 
        -> Result<TestResult, RunError> {
    let mut child = command.spawn().map_err(|error| RunError::Io {
        context: IoErrorContext::QemuTestCommand {
            command: format!("{:?}", command)
        },
        error,
    })?;

    let timeout = Duration::from_secs(timeout_secs);
    match child.wait_timeout(timeout)
        .map_err(context(IoErrorContext::WaitWithTimeout))? 
    {
        None => {
            child.kill().map_err(context(IoErrorContext::KillQemu))?;
            child.wait().map_err(context(IoErrorContext::WaitForQemu))?;
            Ok(TestResult::Timeout)
        }
        Some(exit_status) => {
            match exit_status.code() {
                Some(33) => Ok(TestResult::Success),  // Success exit code
                Some(0) => Ok(TestResult::Failure),   // Normal exit = test failure
                Some(_) => Ok(TestResult::Failure),   // Any other exit code = failure
                None => Err(RunError::NoQemuExitCode)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum RunError {
	#[error("Test timed out")]
	TestTimedOut,
	#[error("Failed to read QEMU exit code")]
	NoQemuExitCode,
	#[error("{context}: An I/O error occured: {error}")]
	Io { context: IoErrorContext, error: io::Error }
}


#[derive(Debug, Error)]
pub enum IoErrorContext {
	#[error("Failed to execute QEMU run command `{command}`")]
	QemuRunCommand { command: String },
	#[error("Failed to execute QEMU test command `{command}`")]
	QemuTestCommand { command: String },
	#[error("Failed to wait with timout")]
	WaitWithTimeout,
	#[error("Failed to kill QEMU")]
	KillQemu,
	#[error("Failed to wait for QEMU process")]
	WaitForQemu
}

fn context(context: IoErrorContext) -> impl FnOnce(io::Error) -> RunError {
	|error | RunError::Io { context, error }
}