use std::time::Duration;

use af_domain::{CredentialKind, Status};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, Set,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
};

use crate::entity::{
    ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, SensitiveString, abilities,
    channel_groups, channel_models, channels, credentials, groups, proxies,
};

pub(super) fn group(name: &str) -> groups::ActiveModel {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(name.to_owned()),
        flags: Set(empty_json_object()),
        ..Default::default()
    }
}

pub(super) fn runtime_channel<const N: usize>(
    name: &str,
    status: Status,
    channel_type: &str,
    protocol: &str,
    base_url: &str,
    headers: [(&str, &str); N],
) -> channels::ActiveModel {
    let headers = Json::Object(
        headers
            .into_iter()
            .map(|(name, value)| (name.to_owned(), Json::String(value.to_owned())))
            .collect(),
    );
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set(channel_type.to_owned()),
        protocol: Set(protocol.to_owned()),
        base_url: Set(Some(ChannelBaseUrl::parse(base_url).unwrap())),
        status: Set(status.code()),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(headers).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn runtime_credential(
    channel_id: i64,
    kind: CredentialKind,
    priority: i32,
    schedulable: bool,
    proxy_id: Option<i64>,
    key_id: &str,
    marker: u8,
    rate_limited: bool,
) -> credentials::ActiveModel {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set(kind.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(key_id, marker)).unwrap()),
        status: Set(Status::Enabled.code()),
        priority: Set(priority),
        schedulable: Set(schedulable),
        proxy_id: Set(proxy_id),
        rate_limit_reset_at: Set(
            rate_limited.then(|| TimeDateTimeWithTimeZone::now_utc() + Duration::from_secs(3_600))
        ),
        ..Default::default()
    }
}

pub(super) fn runtime_proxy(id: i64) -> proxies::ActiveModel {
    let name = format!("runtime-proxy-{id}");
    proxies::ActiveModel {
        id: Set(id),
        active_name: Set(Some(name.clone())),
        name: Set(name),
        scheme: Set("http".to_owned()),
        host: Set(SensitiveString::from("proxy.example".to_owned())),
        port: Set(8080),
        ..Default::default()
    }
}

fn encrypted_envelope(key_id: &str, marker: u8) -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String(key_id.to_owned())),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([marker; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([marker; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

pub(super) fn attach(channel_id: i64, group_id: i64, model: &str) -> AttachmentModels {
    AttachmentModels {
        channel_model: channel_model(channel_id, model),
        channel_group: channel_group(channel_id, group_id),
    }
}

fn channel_model(channel_id: i64, model: &str) -> channel_models::ActiveModel {
    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set(model.to_owned()),
        ..Default::default()
    }
}

fn channel_group(channel_id: i64, group_id: i64) -> channel_groups::ActiveModel {
    channel_groups::ActiveModel {
        channel_id: Set(channel_id),
        group_id: Set(group_id),
        ..Default::default()
    }
}

pub(super) fn ability(
    group_id: i64,
    model: &str,
    channel_id: i64,
    enabled: bool,
    priority: i32,
    weight: i32,
) -> abilities::ActiveModel {
    abilities::ActiveModel {
        group_id: Set(group_id),
        model: Set(model.to_owned()),
        channel_id: Set(channel_id),
        enabled: Set(enabled),
        priority: Set(priority),
        weight: Set(weight),
        ..Default::default()
    }
}

pub(super) struct AttachmentModels {
    channel_model: channel_models::ActiveModel,
    channel_group: channel_groups::ActiveModel,
}

impl AttachmentModels {
    pub(super) async fn insert(
        self,
        connection: &sea_orm::DatabaseConnection,
    ) -> Result<(), sea_orm::DbErr> {
        self.channel_model.insert(connection).await?;
        self.channel_group.insert(connection).await?;
        Ok(())
    }
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}
