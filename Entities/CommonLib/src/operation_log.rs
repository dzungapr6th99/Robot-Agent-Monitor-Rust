use std::{
    fmt::Arguments,
    io::{self, Write},
};

pub fn info(target: &str, message: Arguments<'_>) {
    write_log("INFO", target, message);
}

pub fn error(target: &str, message: Arguments<'_>) {
    write_log("ERROR", target, message);
}

fn write_log(level: &str, target: &str, message: Arguments<'_>) {
    let _ = writeln!(io::stderr().lock(), "[{level}] {target}: {message}");
}

pub fn log_result<T>(
    target: &str,
    operation: &str,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    result.inspect_err(|failure| {
        error(target, format_args!("{operation} failed: {failure:#}"));
    })
}

/// Logs an operation's returned error while preserving its original type and context.
#[macro_export]
macro_rules! logged_result {
    (async $operation:literal, $body:block) => {
        $crate::operation_log::log_result(module_path!(), $operation, (async $body).await)
    };
    ($operation:literal, $body:block) => {
        $crate::operation_log::log_result(module_path!(), $operation, (|| $body)())
    };
}
