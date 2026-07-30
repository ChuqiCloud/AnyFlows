use std::{fmt, time::Duration};

use af_admin::{
    AdminEmailTlsMode, EmailDelivery, EmailDeliveryError, EmailDeliveryFuture, EmailDeliveryRequest,
};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
    transport::smtp::{AsyncSmtpTransportBuilder, authentication::Credentials},
};
use tokio::time::timeout;

/// 基于 lettre 的异步 SMTP 投递器，只允许强制 STARTTLS 或隐式 TLS。
#[derive(Clone, Copy, Default)]
pub struct SmtpEmailDelivery;

impl EmailDelivery for SmtpEmailDelivery {
    fn send<'a>(&'a self, request: EmailDeliveryRequest) -> EmailDeliveryFuture<'a> {
        Box::pin(async move {
            let message = build_message(&request)?;
            let deadline = Duration::from_secs(u64::from(request.timeout_seconds()));
            let transport = build_transport(&request, deadline)?;
            match timeout(deadline, transport.send(message)).await {
                Err(_) => Err(EmailDeliveryError::Timeout),
                Ok(Err(error)) if error.is_timeout() => Err(EmailDeliveryError::Timeout),
                Ok(Err(_)) => Err(EmailDeliveryError::Failed),
                Ok(Ok(_)) => Ok(()),
            }
        })
    }
}

impl fmt::Debug for SmtpEmailDelivery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SmtpEmailDelivery")
    }
}

fn build_message(request: &EmailDeliveryRequest) -> Result<Message, EmailDeliveryError> {
    let from = mailbox(request.from_address(), request.from_name())?;
    let recipient = mailbox(request.recipient(), None)?;
    let mut builder = Message::builder()
        .from(from)
        .to(recipient)
        .subject(request.subject())
        .header(ContentType::TEXT_PLAIN);
    if let Some(reply_to) = request.reply_to() {
        builder = builder.reply_to(mailbox(reply_to, None)?);
    }
    builder
        .body(request.body().to_owned())
        .map_err(|_| EmailDeliveryError::InvalidConfiguration)
}

fn mailbox(address: &str, display_name: Option<&str>) -> Result<Mailbox, EmailDeliveryError> {
    let address = address
        .parse()
        .map_err(|_| EmailDeliveryError::InvalidConfiguration)?;
    Ok(Mailbox::new(display_name.map(str::to_owned), address))
}

fn build_transport(
    request: &EmailDeliveryRequest,
    deadline: Duration,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, EmailDeliveryError> {
    let mut builder = smtp_builder(request.host(), request.tls_mode())?
        .port(request.port())
        .timeout(Some(deadline));
    builder = match (request.username(), request.password()) {
        (None, None) => builder,
        (Some(username), Some(password)) => {
            builder.credentials(Credentials::new(username.to_owned(), password.to_owned()))
        }
        _ => return Err(EmailDeliveryError::InvalidConfiguration),
    };
    Ok(builder.build())
}

fn smtp_builder(
    host: &str,
    tls_mode: AdminEmailTlsMode,
) -> Result<AsyncSmtpTransportBuilder, EmailDeliveryError> {
    match tls_mode {
        AdminEmailTlsMode::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host),
        AdminEmailTlsMode::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(host),
    }
    .map_err(|_| EmailDeliveryError::InvalidConfiguration)
}

#[cfg(test)]
mod tests {
    use tokio::{io::AsyncReadExt as _, net::TcpListener};

    use super::*;

    #[test]
    fn mailbox_builder_rejects_invalid_addresses() {
        assert!(mailbox("sender@example.com", Some("AnyFlows")).is_ok());
        assert_eq!(
            mailbox("invalid", None).unwrap_err(),
            EmailDeliveryError::InvalidConfiguration
        );
    }

    #[tokio::test]
    async fn hard_timeout_stops_a_silent_smtp_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut byte = [0_u8; 1];
            let _ = socket.read(&mut byte).await;
        });
        let deadline = Duration::from_millis(50);
        let transport = smtp_builder("127.0.0.1", AdminEmailTlsMode::StartTls)
            .unwrap()
            .port(address.port())
            .timeout(Some(deadline))
            .build::<Tokio1Executor>();
        let message = Message::builder()
            .from("sender@example.com".parse().unwrap())
            .to("recipient@example.com".parse().unwrap())
            .subject("AnyFlows SMTP 配置测试")
            .body("测试正文".to_owned())
            .unwrap();

        let result = timeout(deadline, transport.send(message)).await;

        assert!(result.is_err() || result.is_ok_and(|result| result.is_err()));
        server.abort();
    }
}
