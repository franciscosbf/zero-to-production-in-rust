use std::ops::Deref;

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
pub struct Template {
    pub html: String,
    pub text: String,
}

#[derive(Debug)]
pub struct SubcriptionConfirmation(Template);

impl Deref for SubcriptionConfirmation {
    type Target = Template;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
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

    let template = Template { html, text };

    Ok(SubcriptionConfirmation(template))
}

#[derive(Debug)]
pub struct CollaboratorInvitation(Template);

impl Deref for CollaboratorInvitation {
    type Target = Template;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn render_collaborator_invitation(
    registration_link: &str,
) -> Result<CollaboratorInvitation, tera::Error> {
    let mut context = Context::new();
    context.insert("registration_link", registration_link);
    let html = TEMPLATES.render("collaborator_invitation.html", &context)?;

    let text = format!(
        "Welcome to our newsletter!\n\
                Visit {} to register as collaborator.",
        registration_link
    );

    let template = Template { html, text };

    Ok(CollaboratorInvitation(template))
}
