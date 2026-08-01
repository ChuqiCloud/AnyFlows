use std::fmt;

use af_domain::{RedemptionBatchId, RedemptionCodeId, WalletEventId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::Zeroizing;

use super::MAX_REDEMPTION_BATCH_CODES;

const REDEMPTION_CODE_PREFIX: &str = "rc-af-";
const REDEMPTION_CODE_ENTROPY_BYTES: usize = 32;
const REDEMPTION_CODE_RANDOM_TEXT_BYTES: usize = 43;
const REDEMPTION_CODE_TEXT_BYTES: usize =
    REDEMPTION_CODE_PREFIX.len() + REDEMPTION_CODE_RANDOM_TEXT_BYTES;

/// 兑换码安全材料生成或解析错误；不保留明文或摘要。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionMaterialError {
    /// 操作系统 CSPRNG 当前不可用。
    #[error("兑换码安全随机源不可用")]
    EntropyUnavailable,
    /// 批次码数量超出生成边界。
    #[error("兑换码批次数量无效")]
    InvalidCount,
    /// 客户端提交值不符合唯一规范格式。
    #[error("兑换码格式无效")]
    InvalidFormat,
    /// 随机材料违反内部标识不变量。
    #[error("兑换码随机材料无效")]
    Invariant,
}

/// 客户端提交或首次签发的规范兑换码明文。
///
/// 该类型拥有并在释放时清零自己的副本，不实现 `Clone`、`Display` 或序列化；
/// 调用方只能在首次展示或摘要计算边界显式读取明文。
pub struct PresentedRedemptionCode(Zeroizing<String>);

impl PresentedRedemptionCode {
    /// 严格解析 `rc-af-` 加 32 字节 Base64URL 无填充随机段。
    pub fn parse(value: &str) -> Result<Self, RedemptionMaterialError> {
        if value.len() != REDEMPTION_CODE_TEXT_BYTES || !value.starts_with(REDEMPTION_CODE_PREFIX) {
            return Err(RedemptionMaterialError::InvalidFormat);
        }
        let random_text = &value[REDEMPTION_CODE_PREFIX.len()..];
        let mut decoded = Zeroizing::new([0_u8; REDEMPTION_CODE_ENTROPY_BYTES]);
        let decoded_length = URL_SAFE_NO_PAD
            .decode_slice(random_text, &mut *decoded)
            .map_err(|_| RedemptionMaterialError::InvalidFormat)?;
        if decoded_length != REDEMPTION_CODE_ENTROPY_BYTES
            || URL_SAFE_NO_PAD.encode(*decoded) != random_text
        {
            return Err(RedemptionMaterialError::InvalidFormat);
        }
        Ok(Self(Zeroizing::new(value.to_owned())))
    }

    /// 返回只应在首次签发响应或摘要边界读取的明文。
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }

    /// 计算数据库唯一查找使用的 SHA-256 摘要。
    #[must_use]
    pub fn digest(&self) -> RedemptionCodeDigest {
        RedemptionCodeDigest(Sha256::digest(self.0.as_bytes()).into())
    }
}

impl fmt::Debug for PresentedRedemptionCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PresentedRedemptionCode(<redacted>)")
    }
}

/// 兑换码明文的固定 SHA-256 摘要。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct RedemptionCodeDigest([u8; 32]);

impl RedemptionCodeDigest {
    /// 从数据库中的 64 位小写十六进制文本恢复摘要。
    pub fn from_persistence_key(value: &str) -> Result<Self, RedemptionMaterialError> {
        if value.len() != 64 {
            return Err(RedemptionMaterialError::Invariant);
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            let high = decode_hex(pair[0]).ok_or(RedemptionMaterialError::Invariant)?;
            let low = decode_hex(pair[1]).ok_or(RedemptionMaterialError::Invariant)?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }

    /// 返回数据库持久化使用的 64 位小写十六进制文本。
    #[must_use]
    pub fn persistence_key(self) -> String {
        encode_hex(&self.0)
    }
}

impl fmt::Debug for RedemptionCodeDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionCodeDigest(<redacted>)")
    }
}

/// 单个兑换码写入数据库的非明文定义。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct RedemptionCodeDefinition {
    code_id: RedemptionCodeId,
    digest: RedemptionCodeDigest,
}

impl RedemptionCodeDefinition {
    /// 组合稳定业务标识与明文摘要。
    pub fn new(
        code_id: RedemptionCodeId,
        digest: RedemptionCodeDigest,
    ) -> Result<Self, RedemptionMaterialError> {
        let wallet_event =
            WalletEventId::new(code_id.bytes()).map_err(|_| RedemptionMaterialError::Invariant)?;
        if wallet_event.is_system_opening() {
            return Err(RedemptionMaterialError::Invariant);
        }
        Ok(Self { code_id, digest })
    }

    /// 返回兑换码稳定业务标识。
    #[must_use]
    pub const fn code_id(self) -> RedemptionCodeId {
        self.code_id
    }

    /// 返回仅用于仓储唯一查找的摘要。
    #[must_use]
    pub const fn digest(self) -> RedemptionCodeDigest {
        self.digest
    }
}

impl fmt::Debug for RedemptionCodeDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionCodeDefinition(<redacted>)")
    }
}

/// 单个只应展示一次的已签发兑换码。
pub struct IssuedRedemptionCode {
    code_id: RedemptionCodeId,
    code: PresentedRedemptionCode,
}

impl IssuedRedemptionCode {
    /// 使用操作系统 CSPRNG 生成业务标识和 256 位兑换码。
    pub fn generate() -> Result<Self, RedemptionMaterialError> {
        let code_id = random_code_id()?;
        let mut entropy = Zeroizing::new([0_u8; REDEMPTION_CODE_ENTROPY_BYTES]);
        getrandom::fill(&mut *entropy).map_err(|_| RedemptionMaterialError::EntropyUnavailable)?;
        Ok(Self::from_entropy(code_id, &entropy))
    }

    /// 返回只应在批次签发响应中展示一次的明文。
    #[must_use]
    pub const fn code(&self) -> &PresentedRedemptionCode {
        &self.code
    }

    /// 返回可以安全持久化的业务标识与摘要定义。
    #[must_use]
    pub fn definition(&self) -> RedemptionCodeDefinition {
        RedemptionCodeDefinition {
            code_id: self.code_id,
            digest: self.code.digest(),
        }
    }

    fn from_entropy(
        code_id: RedemptionCodeId,
        entropy: &[u8; REDEMPTION_CODE_ENTROPY_BYTES],
    ) -> Self {
        let mut text = Zeroizing::new(String::with_capacity(REDEMPTION_CODE_TEXT_BYTES));
        text.push_str(REDEMPTION_CODE_PREFIX);
        URL_SAFE_NO_PAD.encode_string(entropy, &mut text);
        Self {
            code_id,
            code: PresentedRedemptionCode(text),
        }
    }
}

impl fmt::Debug for IssuedRedemptionCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedRedemptionCode(<redacted>)")
    }
}

/// 一批只应展示一次的兑换码与稳定批次标识。
pub struct IssuedRedemptionBatch {
    batch_id: RedemptionBatchId,
    codes: Vec<IssuedRedemptionCode>,
}

impl IssuedRedemptionBatch {
    /// 使用操作系统 CSPRNG 生成指定数量的独立兑换码。
    pub fn generate(count: usize) -> Result<Self, RedemptionMaterialError> {
        if !(1..=MAX_REDEMPTION_BATCH_CODES).contains(&count) {
            return Err(RedemptionMaterialError::InvalidCount);
        }
        let batch_id = random_batch_id()?;
        let mut codes = Vec::with_capacity(count);
        for _ in 0..count {
            codes.push(IssuedRedemptionCode::generate()?);
        }
        Ok(Self { batch_id, codes })
    }

    /// 返回本批次稳定标识。
    #[must_use]
    pub const fn batch_id(&self) -> RedemptionBatchId {
        self.batch_id
    }

    /// 返回不含明文、可重复提交的持久化定义。
    #[must_use]
    pub fn definitions(&self) -> Vec<RedemptionCodeDefinition> {
        self.codes
            .iter()
            .map(IssuedRedemptionCode::definition)
            .collect()
    }

    /// 消费批次并返回只应展示一次的兑换码。
    #[must_use]
    pub fn into_codes(self) -> Vec<IssuedRedemptionCode> {
        self.codes
    }
}

impl fmt::Debug for IssuedRedemptionBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedRedemptionBatch(<redacted>)")
    }
}

fn random_batch_id() -> Result<RedemptionBatchId, RedemptionMaterialError> {
    loop {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| RedemptionMaterialError::EntropyUnavailable)?;
        if let Ok(id) = RedemptionBatchId::new(bytes) {
            return Ok(id);
        }
    }
}

fn random_code_id() -> Result<RedemptionCodeId, RedemptionMaterialError> {
    loop {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| RedemptionMaterialError::EntropyUnavailable)?;
        let Ok(id) = RedemptionCodeId::new(bytes) else {
            continue;
        };
        let wallet_event =
            WalletEventId::new(bytes).map_err(|_| RedemptionMaterialError::Invariant)?;
        if !wallet_event.is_system_opening() {
            return Ok(id);
        }
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn issued_code_from_test_entropy(
    code_id: RedemptionCodeId,
    entropy: [u8; REDEMPTION_CODE_ENTROPY_BYTES],
) -> IssuedRedemptionCode {
    IssuedRedemptionCode::from_entropy(code_id, &entropy)
}
