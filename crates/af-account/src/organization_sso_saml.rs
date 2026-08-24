//! 企业 SSO SAML XMLDSig 的受限纯验证边界。
//!
//! 本模块只消费已由受控回调层取得的 SAML Response 与 Provider 锁定的证书，
//! 不发起网络请求、不写入数据库，也不解析身份、时间窗或登录事务关联。

use std::fmt;

use quick_xml::{
    XmlVersion,
    events::{BytesStart, Event},
    name::ResolveResult,
    reader::NsReader,
};
use saml_rs::{
    constants::{digest_algorithm, namespace, signature_algorithm, transform_algorithm},
    crypto::verify_signature_with_limits,
    xml::XmlLimits,
};
use thiserror::Error;

const MAX_SAML_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_SAML_CERTIFICATE_BYTES: usize = 16 * 1024;
const MAX_SAML_CERTIFICATES: usize = 8;
const MAX_SAML_XML_ATTRIBUTES: usize = 32;
const MAX_SAML_XML_DEPTH: usize = 32;
const MAX_SAML_XML_NODES: usize = 2_048;
const MAX_SAML_XML_TEXT_BYTES: usize = 16 * 1024;
const MAX_SAML_SIGNATURES: usize = 2;

const SAML_XML_LIMITS: XmlLimits = XmlLimits {
    max_bytes: MAX_SAML_RESPONSE_BYTES,
    max_depth: MAX_SAML_XML_DEPTH,
    max_nodes: MAX_SAML_XML_NODES,
    max_attributes_per_element: MAX_SAML_XML_ATTRIBUTES,
    max_attribute_value_bytes: 4 * 1024,
    max_text_bytes: MAX_SAML_XML_TEXT_BYTES,
};

/// 企业 SAML 验签所需的 Provider 配置快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoSamlSignatureVerificationConfig {
    trusted_certificates: Vec<String>,
}

impl OrganizationSsoSamlSignatureVerificationConfig {
    /// 固定当前 Provider 配置版本允许的受信签名证书，支持有限数量的证书轮换。
    pub fn new(certificates: Vec<String>) -> Result<Self, OrganizationSsoSamlSignatureError> {
        if certificates.is_empty() || certificates.len() > MAX_SAML_CERTIFICATES {
            return Err(OrganizationSsoSamlSignatureError::InvalidConfiguration);
        }

        let trusted_certificates = certificates
            .into_iter()
            .map(|certificate| {
                let certificate = certificate.trim().to_owned();
                if !valid_certificate(&certificate) {
                    return Err(OrganizationSsoSamlSignatureError::InvalidConfiguration);
                }
                Ok(certificate)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            trusted_certificates,
        })
    }
}

impl fmt::Debug for OrganizationSsoSamlSignatureVerificationConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoSamlSignatureVerificationConfig(<redacted>)")
    }
}

/// 已通过受信 XMLDSig 验证且没有签名包装歧义的 SAML Response 标记。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct VerifiedOrganizationSsoSamlResponse;

impl fmt::Debug for VerifiedOrganizationSsoSamlResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedOrganizationSsoSamlResponse")
    }
}

/// 企业 SAML XML 输入、证书或 XMLDSig 验证失败时使用的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoSamlSignatureError {
    /// Provider 快照没有受控的受信证书集合。
    #[error("企业 SAML Provider 配置无效")]
    InvalidConfiguration,
    /// Response 不满足受限 XML 与 SHA-256 XMLDSig 验证规则。
    #[error("企业 SAML Response 验证失败")]
    Rejected,
}

/// 企业 SAML XMLDSig 纯验证器；回调、事务和身份绑定必须由后续受控服务组合。
#[derive(Clone)]
pub struct OrganizationSsoSamlSignatureVerifier {
    config: OrganizationSsoSamlSignatureVerificationConfig,
}

impl OrganizationSsoSamlSignatureVerifier {
    /// 绑定创建登录事务时锁定的 Provider 配置版本中的受信证书。
    #[must_use]
    pub const fn new(config: OrganizationSsoSamlSignatureVerificationConfig) -> Self {
        Self { config }
    }

    /// 验证严格受限的单 Assertion Response 与受信 SHA-256 XMLDSig。
    pub fn verify(
        &self,
        saml_response: &str,
    ) -> Result<VerifiedOrganizationSsoSamlResponse, OrganizationSsoSamlSignatureError> {
        inspect_saml_response(saml_response)?;

        // 上游库只以配置证书作为信任锚，并复验引用覆盖、重复 ID 与签名包装。
        let (verified, signed_content) = verify_signature_with_limits(
            saml_response,
            &self.config.trusted_certificates,
            SAML_XML_LIMITS,
        )
        .map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?;
        if !verified || signed_content.is_none() {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        Ok(VerifiedOrganizationSsoSamlResponse)
    }
}

impl fmt::Debug for OrganizationSsoSamlSignatureVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoSamlSignatureVerifier(<configuration>)")
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ElementKind {
    Response,
    Assertion,
    Signature,
    Other,
}

struct SAMLInputInspection {
    stack: Vec<ElementKind>,
    root_seen: bool,
    assertion_count: usize,
    certificate_count: usize,
    digest_method_count: usize,
    node_count: usize,
    signature_count: usize,
    signature_method_count: usize,
}

impl SAMLInputInspection {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            root_seen: false,
            assertion_count: 0,
            certificate_count: 0,
            digest_method_count: 0,
            node_count: 0,
            signature_count: 0,
            signature_method_count: 0,
        }
    }

    fn open(
        &mut self,
        element: &BytesStart<'_>,
        namespace: &ResolveResult<'_>,
        empty: bool,
    ) -> Result<ElementKind, OrganizationSsoSamlSignatureError> {
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
        if self.node_count > MAX_SAML_XML_NODES || self.stack.len() >= MAX_SAML_XML_DEPTH {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        let attributes = inspected_attributes(element)?;
        let local_name = element.local_name();
        let local_name = local_name.as_ref();
        let is_response = local_name == b"Response";
        let is_assertion = local_name == b"Assertion";
        let is_signature = local_name == b"Signature";
        let is_dsig = resolved_namespace_is(namespace, namespace::DSIG);

        if self.stack.is_empty() {
            if self.root_seen
                || !is_response
                || !resolved_namespace_is(namespace, namespace::PROTOCOL)
            {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            self.root_seen = true;
        } else if is_response {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        let parent = self.stack.last().copied();
        if is_assertion {
            if !resolved_namespace_is(namespace, namespace::ASSERTION)
                || parent != Some(ElementKind::Response)
                || empty
            {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            self.assertion_count = self
                .assertion_count
                .checked_add(1)
                .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            if self.assertion_count > 1 {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
        }

        let inside_signature = self
            .stack
            .iter()
            .any(|kind| *kind == ElementKind::Signature);
        if is_signature {
            if !is_dsig || !matches!(parent, Some(ElementKind::Response | ElementKind::Assertion)) {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            self.signature_count = self
                .signature_count
                .checked_add(1)
                .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            if self.signature_count > MAX_SAML_SIGNATURES {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
        } else if inside_signature && xml_signature_local_name(local_name) && !is_dsig {
            // 上游签名库按本地名定位 XMLDSig 结构，必须在进入前拒绝命名空间替换。
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }

        if inside_signature && is_dsig {
            self.validate_signature_element(local_name, attributes.algorithm.as_deref())?;
        }

        let kind = if is_response {
            ElementKind::Response
        } else if is_assertion {
            ElementKind::Assertion
        } else if is_signature {
            ElementKind::Signature
        } else {
            ElementKind::Other
        };
        Ok(kind)
    }

    fn validate_signature_element(
        &mut self,
        local_name: &[u8],
        algorithm: Option<&str>,
    ) -> Result<(), OrganizationSsoSamlSignatureError> {
        match local_name {
            b"SignatureMethod" => {
                let algorithm = required_algorithm(algorithm)?;
                if !matches!(
                    algorithm,
                    signature_algorithm::RSA_SHA256 | signature_algorithm::RSA_SHA256_MGF1
                ) {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
                self.signature_method_count = self
                    .signature_method_count
                    .checked_add(1)
                    .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            }
            b"DigestMethod" => {
                if required_algorithm(algorithm)? != digest_algorithm::SHA256 {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
                self.digest_method_count = self
                    .digest_method_count
                    .checked_add(1)
                    .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
            }
            b"CanonicalizationMethod" => {
                if required_algorithm(algorithm)? != transform_algorithm::EXC_C14N {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
            }
            b"Transform" => {
                let algorithm = required_algorithm(algorithm)?;
                if !matches!(
                    algorithm,
                    transform_algorithm::ENVELOPED_SIGNATURE | transform_algorithm::EXC_C14N
                ) {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
            }
            b"X509Certificate" => {
                self.certificate_count = self
                    .certificate_count
                    .checked_add(1)
                    .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
                if self.certificate_count > MAX_SAML_CERTIFICATES {
                    return Err(OrganizationSsoSamlSignatureError::Rejected);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), OrganizationSsoSamlSignatureError> {
        self.stack
            .pop()
            .map(|_| ())
            .ok_or(OrganizationSsoSamlSignatureError::Rejected)
    }

    fn finish(self) -> Result<(), OrganizationSsoSamlSignatureError> {
        if !self.stack.is_empty()
            || !self.root_seen
            || self.assertion_count != 1
            || self.signature_count == 0
            || self.signature_method_count == 0
            || self.digest_method_count == 0
        {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }
        Ok(())
    }
}

struct InspectedAttributes {
    algorithm: Option<String>,
}

fn inspected_attributes(
    element: &BytesStart<'_>,
) -> Result<InspectedAttributes, OrganizationSsoSamlSignatureError> {
    let mut algorithm = None;
    let mut count = 0usize;
    for attribute in element.attributes().with_checks(true) {
        let attribute = attribute.map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?;
        count = count
            .checked_add(1)
            .ok_or(OrganizationSsoSamlSignatureError::Rejected)?;
        if count > MAX_SAML_XML_ATTRIBUTES {
            return Err(OrganizationSsoSamlSignatureError::Rejected);
        }
        if attribute.key.as_ref() == b"Algorithm" {
            if algorithm.is_some() {
                return Err(OrganizationSsoSamlSignatureError::Rejected);
            }
            let value = attribute
                .decoded_and_normalized_value(XmlVersion::Implicit1_0, element.decoder())
                .map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?
                .into_owned();
            algorithm = Some(value);
        }
    }
    Ok(InspectedAttributes { algorithm })
}

fn inspect_saml_response(saml_response: &str) -> Result<(), OrganizationSsoSamlSignatureError> {
    if saml_response.is_empty() || saml_response.len() > MAX_SAML_RESPONSE_BYTES {
        return Err(OrganizationSsoSamlSignatureError::Rejected);
    }

    let mut reader = NsReader::from_str(saml_response);
    let mut inspection = SAMLInputInspection::new();
    loop {
        let (namespace, event) = reader
            .read_resolved_event()
            .map_err(|_| OrganizationSsoSamlSignatureError::Rejected)?;
        match event {
            Event::Start(element) => {
                let kind = inspection.open(&element, &namespace, false)?;
                inspection.stack.push(kind);
            }
            Event::Empty(element) => {
                let _ = inspection.open(&element, &namespace, true)?;
            }
            Event::End(_) => inspection.close()?,
            // 禁止 DTD 可同时阻断实体展开与外部实体读取路径。
            Event::DocType(_) => return Err(OrganizationSsoSamlSignatureError::Rejected),
            Event::Eof => break,
            _ => {}
        }
    }
    inspection.finish()
}

fn resolved_namespace_is(namespace: &ResolveResult<'_>, expected: &str) -> bool {
    matches!(namespace, ResolveResult::Bound(value) if value.as_ref() == expected.as_bytes())
}

fn xml_signature_local_name(local_name: &[u8]) -> bool {
    matches!(
        local_name,
        b"Signature"
            | b"SignedInfo"
            | b"CanonicalizationMethod"
            | b"SignatureMethod"
            | b"Reference"
            | b"Transforms"
            | b"Transform"
            | b"DigestMethod"
            | b"DigestValue"
            | b"SignatureValue"
            | b"KeyInfo"
            | b"X509Data"
            | b"X509Certificate"
    )
}

fn required_algorithm(algorithm: Option<&str>) -> Result<&str, OrganizationSsoSamlSignatureError> {
    algorithm
        .filter(|value| !value.is_empty() && value.trim() == *value && value.is_ascii())
        .ok_or(OrganizationSsoSamlSignatureError::Rejected)
}

fn valid_certificate(certificate: &str) -> bool {
    certificate.len() <= MAX_SAML_CERTIFICATE_BYTES
        && certificate.is_ascii()
        && certificate.starts_with("-----BEGIN CERTIFICATE-----")
        && certificate.ends_with("-----END CERTIFICATE-----")
}

#[cfg(test)]
mod tests {
    use super::*;
    use saml_rs::crypto::{construct_saml_signature, keys::load_private_key};

    const TEST_CERTIFICATE: &str =
        include_str!("fixtures/organization_sso_saml_test_certificate.pem");
    const TEST_PRIVATE_KEY: &str = include_str!("fixtures/organization_sso_saml_test_private.pem");
    const UNSIGNED_RESPONSE: &str = r#"<samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion" ID="_response-1" Version="2.0" IssueInstant="2026-08-24T00:00:00Z"><saml:Issuer>https://idp.example.com/metadata</saml:Issuer><samlp:Status><samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/></samlp:Status><saml:Assertion ID="_assertion-1" Version="2.0" IssueInstant="2026-08-24T00:00:00Z"><saml:Issuer>https://idp.example.com/metadata</saml:Issuer><saml:Subject><saml:NameID>directory-subject-123</saml:NameID></saml:Subject></saml:Assertion></samlp:Response>"#;

    fn verifier() -> OrganizationSsoSamlSignatureVerifier {
        OrganizationSsoSamlSignatureVerifier::new(
            OrganizationSsoSamlSignatureVerificationConfig::new(vec![TEST_CERTIFICATE.to_owned()])
                .unwrap(),
        )
    }

    fn signed_response() -> String {
        let key = load_private_key(TEST_PRIVATE_KEY, None).unwrap();
        construct_saml_signature(
            UNSIGNED_RESPONSE,
            false,
            &key,
            TEST_CERTIFICATE,
            signature_algorithm::RSA_SHA256,
            &[],
            None,
        )
        .unwrap()
    }

    #[test]
    fn verifier_accepts_a_real_trusted_rsa_sha256_signature() {
        let verified = verifier().verify(&signed_response()).unwrap();

        assert_eq!(
            format!("{verified:?}"),
            "VerifiedOrganizationSsoSamlResponse"
        );
    }

    #[test]
    fn verifier_rejects_weak_algorithms_dtd_and_assertion_ambiguity() {
        let weak_algorithm = signed_response().replacen(
            signature_algorithm::RSA_SHA256,
            signature_algorithm::RSA_SHA1,
            1,
        );
        assert_eq!(
            verifier().verify(&weak_algorithm),
            Err(OrganizationSsoSamlSignatureError::Rejected)
        );

        let dtd = format!("<!DOCTYPE Response [<!ENTITY marker \"x\">]>{UNSIGNED_RESPONSE}");
        assert_eq!(
            verifier().verify(&dtd),
            Err(OrganizationSsoSamlSignatureError::Rejected)
        );

        let ambiguous = signed_response().replacen(
            "</samlp:Response>",
            "<saml:Assertion xmlns:saml=\"urn:oasis:names:tc:SAML:2.0:assertion\" ID=\"_assertion-2\"><saml:Subject/></saml:Assertion></samlp:Response>",
            1,
        );
        assert_eq!(
            verifier().verify(&ambiguous),
            Err(OrganizationSsoSamlSignatureError::Rejected)
        );
    }

    #[test]
    fn configuration_requires_a_bounded_pinned_certificate_set() {
        assert_eq!(
            OrganizationSsoSamlSignatureVerificationConfig::new(Vec::new()),
            Err(OrganizationSsoSamlSignatureError::InvalidConfiguration)
        );
        assert_eq!(
            OrganizationSsoSamlSignatureVerificationConfig::new(vec![
                "not a certificate".to_owned()
            ]),
            Err(OrganizationSsoSamlSignatureError::InvalidConfiguration)
        );
    }
}
