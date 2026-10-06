use crate::management_error::ManagementError;

/// 解析钱包账本共用的严格游标参数，业务层继续校验分页上限。
pub(crate) fn parse_wallet_list_query(
    raw_query: Option<&str>,
    default_limit: usize,
) -> Result<(Option<i64>, usize), ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok((None, default_limit));
    };
    if raw_query.is_empty() {
        return Ok((None, default_limit));
    }
    validate_percent_encoding(raw_query)?;
    let mut before = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_positive_usize(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok((before, limit.unwrap_or(default_limit)))
}

fn parse_positive_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    let value = value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)?;
    (value > 0)
        .then_some(value)
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_positive_usize(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}
