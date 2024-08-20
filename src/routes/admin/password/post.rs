use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;

use crate::{
    authentication::{self, validate_credentials, AuthError, Credentials, UserId},
    routes::admin::dashboard::get_username,
    util::{e500, see_other},
};

#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: Secret<String>,
    new_password: Secret<String>,
    new_password_check: Secret<String>,
}

pub async fn change_password(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    if form.new_password.expose_secret() != form.new_password_check.expose_secret() {
        FlashMessage::error(
            "You entered two different new passwords - the field values must match.",
        )
        .send();

        return Ok(see_other("/admin/password"));
    }

    if !(8..=64).contains(&form.0.new_password.expose_secret().len()) {
        FlashMessage::error("New password must contain at least 8 and up to 64 characters.").send();

        return Ok(see_other("/admin/password"));
    }

    let user_id = user_id.into_inner();

    let username = get_username(*user_id, &pool).await.map_err(e500)?;
    let credentials = Credentials {
        username,
        password: form.0.current_password,
    };
    if let Err(e) = validate_credentials(credentials, &pool).await {
        return match e {
            AuthError::InvalidCredentials(_) => {
                FlashMessage::error("The current password is incorrect.").send();

                Ok(see_other("/admin/password"))
            }
            AuthError::UnexpectedError(_) => Err(e500(e)),
        };
    }

    authentication::change_password(*user_id, form.0.new_password, &pool)
        .await
        .map_err(e500)?;

    FlashMessage::error("Your password has been changed.").send();

    Ok(see_other("/admin/password"))
}
