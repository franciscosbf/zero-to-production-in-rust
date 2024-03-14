use lazy_static::lazy_static;
use tera::{self, Context, Tera};

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("templates/**/*") {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Tera failed to parse templates: {}", e);
                ::std::process::exit(1);
            }
        };

        tera.autoescape_on(vec![".html"]);

        tera
    };
}

#[derive(Debug)]
pub struct SubcriptionConfirmation {
    pub html: String,
    pub text: String,
}

pub fn render_subscription_confirmation(
    confirmation_link: &str,
) -> Result<SubcriptionConfirmation, tera::Error> {
    let mut context = Context::new();
    context.insert("confirmation_link", confirmation_link);
    let html = TEMPLATES.render("subscription_confirmation.html", &context)?;

    let text = format!(
        "Welcome to our newsletter!\n\
                Visit {} to confirm your subscription.",
        confirmation_link
    );

    let template = SubcriptionConfirmation { html, text };

    Ok(template)
}
