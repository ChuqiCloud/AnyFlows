use af_db::{
    EncryptedCredentialEnvelope, SchedulerRuntimeCredentialRecord, SchedulerRuntimeProxyRecord,
};
use af_domain::{AsyncTaskBindingFingerprint, ChannelId, ChannelTimeout, CredentialId, GroupId};
use sha2::{Digest as _, Sha256};

use super::BoundVideoTargetSnapshot;

/// 对完整目标、凭据、代理和等待策略计算不泄露原值的稳定绑定指纹。
pub(super) fn binding_fingerprint(
    target_group_id: GroupId,
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_revision: u64,
    snapshot: &BoundVideoTargetSnapshot,
) -> AsyncTaskBindingFingerprint {
    let mut digest = Sha256::new();
    digest.update(b"anyflows/video-task-binding/v1\0");
    hash_i64(&mut digest, target_group_id.get());
    hash_i64(&mut digest, channel_id.get());
    hash_i64(&mut digest, credential_id.get());
    hash_u64(&mut digest, credential_revision);
    hash_bytes(&mut digest, snapshot.channel_type.as_str().as_bytes());
    hash_bytes(&mut digest, snapshot.protocol.as_str().as_bytes());
    hash_optional_bytes(&mut digest, snapshot.base_url.as_deref().map(str::as_bytes));
    hash_optional_u64(&mut digest, snapshot.timeout.map(ChannelTimeout::seconds));
    hash_u64(
        &mut digest,
        u64::try_from(snapshot.headers.len()).expect("Header 数量必须适配 u64"),
    );
    for header in &snapshot.headers {
        hash_bytes(&mut digest, header.name().as_bytes());
        hash_bytes(&mut digest, header.value().as_bytes());
    }
    hash_u64(
        &mut digest,
        u64::try_from(snapshot.auto_ban_rules.server_statuses().len())
            .expect("自动禁用状态码数量必须适配 u64"),
    );
    for status in snapshot.auto_ban_rules.server_statuses() {
        digest.update(status.get().to_be_bytes());
    }
    hash_u64(
        &mut digest,
        u64::try_from(snapshot.auto_ban_rules.keywords().len())
            .expect("自动禁用关键词数量必须适配 u64"),
    );
    for keyword in snapshot.auto_ban_rules.keywords() {
        hash_bytes(&mut digest, keyword.as_bytes());
    }
    hash_bool(&mut digest, snapshot.pool_mode);
    hash_credential(&mut digest, &snapshot.credential);
    hash_bytes(&mut digest, snapshot.requested_model.as_str().as_bytes());
    hash_bytes(&mut digest, snapshot.expected_model.as_str().as_bytes());
    hash_u64(
        &mut digest,
        u64::try_from(snapshot.attempt_timeout.as_millis()).expect("提交等待时间必须适配 u64"),
    );
    AsyncTaskBindingFingerprint::new(digest.finalize().into())
}

fn hash_credential(digest: &mut Sha256, credential: &SchedulerRuntimeCredentialRecord) {
    hash_i64(digest, credential.credential_id());
    hash_bytes(digest, credential.credential_kind().as_str().as_bytes());
    hash_envelope(digest, credential.envelope());
    hash_u64(digest, credential.credential_revision());
    hash_bool(digest, credential.proxy_required());
    match credential.proxy() {
        Some(proxy) => {
            digest.update([1]);
            hash_proxy(digest, proxy);
        }
        None => digest.update([0]),
    }
    digest.update(credential.priority().to_be_bytes());
    digest.update(credential.weight().to_be_bytes());
    hash_optional_u64(
        digest,
        credential
            .concurrency()
            .map(|concurrency| u64::from(concurrency.get())),
    );
}

fn hash_proxy(digest: &mut Sha256, proxy: &SchedulerRuntimeProxyRecord) {
    hash_i64(digest, proxy.proxy_id().get());
    hash_bytes(digest, proxy.scheme().as_str().as_bytes());
    hash_bytes(digest, proxy.host().as_bytes());
    digest.update(proxy.port().to_be_bytes());
    hash_optional_bytes(digest, proxy.username().map(str::as_bytes));
    match proxy.password_secret() {
        Some(envelope) => {
            digest.update([1]);
            hash_envelope(digest, envelope);
        }
        None => digest.update([0]),
    }
    hash_bool(digest, proxy.trust_proxy_dns());
    hash_i64(digest, proxy.version());
}

fn hash_envelope(digest: &mut Sha256, envelope: &EncryptedCredentialEnvelope) {
    hash_bytes(digest, envelope.key_id().as_bytes());
    hash_bytes(digest, envelope.nonce());
    hash_bytes(digest, envelope.ciphertext());
}

fn hash_optional_bytes(digest: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            hash_bytes(digest, value);
        }
        None => digest.update([0]),
    }
}

fn hash_optional_u64(digest: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            digest.update([1]);
            hash_u64(digest, value);
        }
        None => digest.update([0]),
    }
}

fn hash_bytes(digest: &mut Sha256, value: &[u8]) {
    hash_u64(
        digest,
        u64::try_from(value.len()).expect("哈希字段长度必须适配 u64"),
    );
    digest.update(value);
}

fn hash_bool(digest: &mut Sha256, value: bool) {
    digest.update([u8::from(value)]);
}

fn hash_i64(digest: &mut Sha256, value: i64) {
    digest.update(value.to_be_bytes());
}

fn hash_u64(digest: &mut Sha256, value: u64) {
    digest.update(value.to_be_bytes());
}
