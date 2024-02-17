use reqwest::Client;
use secrecy::{ExposeSecret, Secret};

use crate::domain::SubscriberEmail;

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct SendEmailRequest<'a> {
    from: &'a str,
    to: &'a str,
    subject: &'a str,
    html_body: &'a str,
    text_body: &'a str,
}

pub struct EmailClient {
    http_client: Client,
    base_url: reqwest::Url,
    sender: SubscriberEmail,
    authorization_token: Secret<String>,
}

impl EmailClient {
    pub fn new(
        base_url: reqwest::Url,
        sender: SubscriberEmail,
        authorization_token: Secret<String>,
    ) -> Self {
        Self {
            http_client: Client::new(),
            base_url,
            sender,
            authorization_token,
        }
    }

    pub async fn send_email(
        &self,
        recipient: SubscriberEmail,
        subject: &str,
        html_content: &str,
        text_content: &str,
    ) -> Result<(), reqwest::Error> {
        let url = self.base_url.join("email").unwrap();
        let request_body = SendEmailRequest {
            from: self.sender.as_ref(),
            to: recipient.as_ref(),
            subject,
            html_body: html_content,
            text_body: text_content,
        };

        self.http_client
            .post(url)
            .header(
                "X-Postmark-Server-Token",
                self.authorization_token.expose_secret(),
            )
            .header("Accept", "application/json")
            // json method sets the header at this time.
            // However, I prefer to be sceptical about that.
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use fake::faker::lorem::en::Sentence;
    use fake::Faker;
    use fake::{faker::internet::en::SafeEmail, Fake};
    use secrecy::Secret;
    use wiremock::matchers::{header, header_exists, method, path};
    use wiremock::{Match, Mock, MockServer, ResponseTemplate};

    use crate::domain::SubscriberEmail;
    use crate::email_client::EmailClient;

    struct SendEmailBodyMatcher;

    impl Match for SendEmailBodyMatcher {
        fn matches(&self, request: &wiremock::Request) -> bool {
            serde_json::from_slice::<serde_json::Value>(&request.body).map_or(false, |body| {
                body.get("From").is_some()
                    && body.get("To").is_some()
                    && body.get("Subject").is_some()
                    && body.get("HtmlBody").is_some()
                    && body.get("TextBody").is_some()
            })
        }
    }

    #[tokio::test]
    async fn send_email_sends_the_expected_request() {
        let mock_server = MockServer::start().await;
        let sender = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let base_url = reqwest::Url::parse(&mock_server.uri()).unwrap();
        let email_client = EmailClient::new(base_url, sender, Secret::new(Faker.fake()));

        Mock::given(method("POST"))
            .and(path("/email"))
            .and(header_exists("X-Postmark-Server-Token"))
            .and(header("Content-Type", "application/json"))
            .and(header("Accept", "application/json"))
            .and(SendEmailBodyMatcher)
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let subscriber_email = SubscriberEmail::parse(SafeEmail().fake()).unwrap();
        let subject = Sentence(1..2).fake::<String>();
        let content = Sentence(1..10).fake::<String>();

        let _ = email_client
            .send_email(subscriber_email, &subject, &content, &content)
            .await;
    }
}
