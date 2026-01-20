use clementine_cli::config::ConfigErrors;
use clementine_cli::errors::BridgeCliError;
use colored::Colorize;
use std::{any::Any, error::Error};

/// Universal CLI command handler - handles all cases: sync/async, with/without result processing
#[macro_export]
macro_rules! handle_cli_command {
    // Async with result processing (most flexible)
    (async $expr:expr, $pattern:pat => { $($body:tt)* }) => {
        match $expr.await {
            Ok($pattern) => { $($body)* }
            Err(e) => {
                $crate::cli::macros::handle_err(e);
            }
        }
    };

    // Async without success message (convenience)
    (async $expr:expr) => {
        handle_cli_command!(async $expr, _result => {});
    };

    // Async with simple success message (convenience)
    (async $expr:expr, $success_msg:expr) => {
        handle_cli_command!(async $expr, _result => {
            println!("{}", $success_msg);
        });
    };

    // Sync with result processing (most flexible)
    ($expr:expr, $pattern:pat => { $($body:tt)* }) => {
        match $expr {
            Ok($pattern) => { $($body)* }
            Err(e) => {
                $crate::cli::macros::handle_err(e);
            }
        }
    };

    // Sync with simple success message (convenience)
    ($expr:expr, $success_msg:expr) => {
        handle_cli_command!($expr, _result => {
            println!("{}", $success_msg);
        });
    };

    // Sync without success message (convenience)
    ($expr:expr) => {
        handle_cli_command!($expr, _result => {});
    };
}

fn report_config_error(e: &ConfigErrors) {
    tracing::error!(error = ?e);
    eprintln!("{} {}", "Error:".bold().red(), e);
}

fn report_bridge(e: &BridgeCliError) {
    tracing::error!(error = ?e);
    eprintln!("{} {}", "Error:".bold().red(), e);
}

fn report_any(err: &(dyn Error + 'static)) {
    tracing::error!(error = ?err);
    eprintln!("{} {}", "Error:".bold().red(), err);
}

fn report_error_by_type<E>(e: E)
where
    E: Error + 'static,
{
    let err_obj: &dyn Error = &e;
    let any_view: &dyn Any = &e;

    if let Some(b) = any_view.downcast_ref::<BridgeCliError>() {
        report_bridge(b);
    } else if let Some(c) = any_view.downcast_ref::<ConfigErrors>() {
        report_config_error(c);
    } else {
        report_any(err_obj);
    }
}
pub(crate) fn handle_err<E>(e: E) -> !
where
    E: Error + 'static,
{
    report_error_by_type(e);
    std::process::exit(1);
}

#[macro_export]
macro_rules! handle_simple_call {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => {
                $crate::cli::macros::handle_err(e);
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use std::fmt;

    #[derive(Debug)]
    struct TestError(&'static str);

    impl fmt::Display for TestError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for TestError {}

    fn successful_operation() -> Result<String, TestError> {
        Ok("test result".to_string())
    }

    #[tokio::test]
    async fn test_async_successful_operation() {
        async fn async_success() -> Result<String, TestError> {
            Ok("async result".to_string())
        }

        let result = async_success().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "async result");
    }

    #[test]
    fn test_sync_successful_operation() {
        let result = successful_operation();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test result");
    }
}
