//! Bounded decoding for the read-only dashboard query-string envelope.

use agent_observability_contracts::dashboard::{
    DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES, DashboardQueryRequestV1,
};

pub(crate) fn decode_query(raw: Option<&str>) -> Result<DashboardQueryRequestV1, ()> {
    let raw = raw.ok_or(())?;
    if raw.len() > DASHBOARD_REQUEST_MAX_SERIALIZED_UTF8_BYTES {
        return Err(());
    }
    let encoded = raw.strip_prefix("request=").ok_or(())?;
    if encoded.contains('&') {
        return Err(());
    }
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut input = encoded.bytes();
    while let Some(byte) = input.next() {
        decoded.push(match byte {
            b'%' => {
                let high = hex(input.next().ok_or(())?)?;
                let low = hex(input.next().ok_or(())?)?;
                high * 16 + low
            }
            b'+' => b' ',
            value => value,
        });
    }
    let request: DashboardQueryRequestV1 = serde_json::from_slice(&decoded).map_err(|_| ())?;
    request.validate().map_err(|_| ())?;
    Ok(request)
}

fn hex(byte: u8) -> Result<u8, ()> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_a_single_closed_bounded_request() {
        assert!(decode_query(Some("request=%7B%22schemaVersion%22%3A%22agent_observability.dashboard_query.v1%22%2C%22kind%22%3A%22bootstrap%22%7D")).is_ok());
        for invalid in [
            None,
            Some(""),
            Some("request=%"),
            Some("request=%ff"),
            Some("request=%xy"),
            Some("request={}&request={}"),
            Some("request={}&sql=SELECT"),
            Some("request={\"kind\":\"bootstrap\",\"sql\":\"SELECT\"}"),
        ] {
            assert!(decode_query(invalid).is_err());
        }
        assert!(decode_query(Some(&format!("request={}", "x".repeat(8192)))).is_err());
    }
}
