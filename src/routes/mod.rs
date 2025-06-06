mod admin;
mod collaborator;
mod health_check;
mod home;
mod login;
mod subscriptions;
mod subscriptions_confirm;

pub use admin::*;
pub use collaborator::*;
pub use health_check::*;
pub use home::*;
pub use login::*;
pub use subscriptions::*;
pub use subscriptions_confirm::*;

fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}", e)?;

    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }

    Ok(())
}
