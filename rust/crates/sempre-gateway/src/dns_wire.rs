use std::{
    fmt::Write as _,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use crate::GatewayError;

const DNS_HEADER_SIZE: usize = 12;
const TYPE_A: u16 = 1;
const TYPE_AAAA: u16 = 28;
const TYPE_CNAME: u16 = 5;
const TYPE_TXT: u16 = 16;
pub(crate) const TYPE_HTTPS: u16 = 65;

#[derive(Debug)]
pub(crate) struct Question {
    pub(crate) name: String,
    pub(crate) record_type: u16,
    end: usize,
}

struct AnswerRecord {
    name: String,
    kind: u16,
    ttl: u32,
    start: usize,
    length: usize,
}

pub(crate) fn parse_question(packet: &[u8]) -> Result<Option<Question>, GatewayError> {
    if packet.len() < DNS_HEADER_SIZE {
        return Err(GatewayError::invalid(
            "DNS packet is shorter than its header",
        ));
    }
    if u16::from_be_bytes([packet[4], packet[5]]) == 0 {
        return Ok(None);
    }
    let (name, offset) = read_name(packet, DNS_HEADER_SIZE, 0)?;
    if offset + 4 > packet.len() {
        return Err(GatewayError::invalid("DNS question is truncated"));
    }
    Ok(Some(Question {
        name,
        record_type: u16::from_be_bytes([packet[offset], packet[offset + 1]]),
        end: offset + 4,
    }))
}

pub(crate) fn response_with_code(packet: &[u8], code: u8) -> Result<Vec<u8>, GatewayError> {
    if packet.len() < DNS_HEADER_SIZE {
        return Err(GatewayError::invalid(
            "DNS packet is shorter than its header",
        ));
    }
    let end = parse_question(packet)?.map_or(DNS_HEADER_SIZE, |question| question.end);
    let mut response = packet[..end].to_vec();
    response[2] |= 0x80;
    response[3] = (response[3] & 0xf0) | 0x80 | (code & 0x0f);
    response[6..12].fill(0);
    Ok(response)
}

pub(crate) fn build_query(name: &str, record_type: u16) -> Result<Vec<u8>, GatewayError> {
    let mut packet = vec![0_u8; DNS_HEADER_SIZE];
    packet[0..2].copy_from_slice(&random_id().to_be_bytes());
    packet[2] = 1;
    packet[5] = 1;
    let normalized = name.trim().trim_end_matches('.');
    if normalized.is_empty() {
        return Err(GatewayError::invalid("DNS query name is required"));
    }
    for label in normalized.split('.') {
        if label.is_empty() || label.len() > 63 || !label.is_ascii() {
            return Err(GatewayError::invalid("invalid DNS query name"));
        }
        packet.push(u8::try_from(label.len()).expect("label length checked"));
        packet.extend_from_slice(label.as_bytes());
    }
    packet.push(0);
    packet.extend_from_slice(&record_type.to_be_bytes());
    packet.extend_from_slice(&1_u16.to_be_bytes());
    if packet.len() > 255 {
        return Err(GatewayError::invalid("DNS query name is too long"));
    }
    Ok(packet)
}

fn random_id() -> u16 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    u16::try_from(hasher.finish() & u64::from(u16::MAX)).expect("masked DNS ID")
}

fn read_name(packet: &[u8], mut offset: usize, depth: u8) -> Result<(String, usize), GatewayError> {
    if depth > 16 {
        return Err(GatewayError::invalid("DNS name compression is recursive"));
    }
    let mut labels = Vec::new();
    let mut next = None;
    loop {
        let length = *packet
            .get(offset)
            .ok_or_else(|| GatewayError::invalid("DNS name is truncated"))?;
        if length == 0 {
            offset += 1;
            break;
        }
        if length & 0xc0 == 0xc0 {
            let low = *packet
                .get(offset + 1)
                .ok_or_else(|| GatewayError::invalid("DNS compression pointer is truncated"))?;
            let pointer = usize::from((u16::from(length & 0x3f) << 8) | u16::from(low));
            next.get_or_insert(offset + 2);
            let (suffix, _) = read_name(packet, pointer, depth + 1)?;
            labels.extend(suffix.trim_end_matches('.').split('.').map(str::to_owned));
            break;
        }
        let length = usize::from(length);
        let label = packet
            .get(offset + 1..offset + 1 + length)
            .ok_or_else(|| GatewayError::invalid("DNS label is truncated"))?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        offset += 1 + length;
    }
    Ok((format!("{}.", labels.join(".")), next.unwrap_or(offset)))
}

fn answer_records(packet: &[u8]) -> Result<Vec<AnswerRecord>, GatewayError> {
    let question = parse_question(packet)?;
    let mut offset = question.map_or(DNS_HEADER_SIZE, |question| question.end);
    let count = u16::from_be_bytes([packet[6], packet[7]]);
    let mut records = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let (name, next) = read_name(packet, offset, 0)?;
        if next + 10 > packet.len() {
            return Err(GatewayError::invalid("DNS answer is truncated"));
        }
        let kind = u16::from_be_bytes([packet[next], packet[next + 1]]);
        let ttl = u32::from_be_bytes([
            packet[next + 4],
            packet[next + 5],
            packet[next + 6],
            packet[next + 7],
        ]);
        let length = usize::from(u16::from_be_bytes([packet[next + 8], packet[next + 9]]));
        let start = next + 10;
        if start + length > packet.len() {
            return Err(GatewayError::invalid("DNS answer data is truncated"));
        }
        records.push(AnswerRecord {
            name,
            kind,
            ttl,
            start,
            length,
        });
        offset = start + length;
    }
    Ok(records)
}

pub(crate) fn answer_ipv4_addresses(packet: &[u8]) -> Result<Vec<Ipv4Addr>, GatewayError> {
    Ok(answer_records(packet)?
        .into_iter()
        .filter(|record| record.kind == TYPE_A && record.length == 4)
        .map(|record| {
            Ipv4Addr::new(
                packet[record.start],
                packet[record.start + 1],
                packet[record.start + 2],
                packet[record.start + 3],
            )
        })
        .collect())
}

pub(crate) fn answer_ip_addresses(packet: &[u8]) -> Result<Vec<IpAddr>, GatewayError> {
    Ok(answer_records(packet)?
        .into_iter()
        .filter_map(|record| match (record.kind, record.length) {
            (TYPE_A, 4) => Some(IpAddr::V4(Ipv4Addr::new(
                packet[record.start],
                packet[record.start + 1],
                packet[record.start + 2],
                packet[record.start + 3],
            ))),
            (TYPE_AAAA, 16) => Some(IpAddr::V6(Ipv6Addr::from(
                <[u8; 16]>::try_from(&packet[record.start..record.start + 16])
                    .expect("length checked"),
            ))),
            _ => None,
        })
        .collect())
}

pub(crate) fn response_code(packet: &[u8]) -> Result<u8, GatewayError> {
    packet
        .get(3)
        .map(|flags| flags & 0x0f)
        .ok_or_else(|| GatewayError::invalid("DNS response is shorter than its header"))
}

pub(crate) fn format_answers(packet: &[u8]) -> Result<Vec<String>, GatewayError> {
    answer_records(packet)?
        .into_iter()
        .map(|record| {
            let value = match (record.kind, record.length) {
                (TYPE_A, 4) => Ipv4Addr::new(
                    packet[record.start],
                    packet[record.start + 1],
                    packet[record.start + 2],
                    packet[record.start + 3],
                )
                .to_string(),
                (TYPE_AAAA, 16) => {
                    let bytes: [u8; 16] = packet[record.start..record.start + 16]
                        .try_into()
                        .expect("length checked");
                    std::net::Ipv6Addr::from(bytes).to_string()
                }
                (TYPE_CNAME, _) => read_name(packet, record.start, 0)?.0,
                (TYPE_TXT, _) => format_txt(&packet[record.start..record.start + record.length]),
                _ => hex(&packet[record.start..record.start + record.length]),
            };
            Ok(format!(
                "{} {} IN {} {value}",
                record.name,
                record.ttl,
                type_name(record.kind)
            ))
        })
        .collect()
}

fn format_txt(data: &[u8]) -> String {
    let mut offset = 0;
    let mut values = Vec::new();
    while let Some(length) = data.get(offset).copied() {
        offset += 1;
        let end = (offset + usize::from(length)).min(data.len());
        values.push(format!("{:?}", String::from_utf8_lossy(&data[offset..end])));
        offset = end;
    }
    values.join(" ")
}

fn hex(data: &[u8]) -> String {
    data.iter().fold(String::new(), |mut output, byte| {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
        output
    })
}

pub(crate) fn record_number(value: &str) -> Result<(String, u16), GatewayError> {
    let name = if value.trim().is_empty() {
        "A".into()
    } else {
        value.trim().to_ascii_uppercase()
    };
    let number = match name.as_str() {
        "A" => TYPE_A,
        "AAAA" => TYPE_AAAA,
        "CNAME" => TYPE_CNAME,
        "TXT" => TYPE_TXT,
        "HTTPS" => TYPE_HTTPS,
        "MX" => 15,
        "NS" => 2,
        _ => {
            return Err(GatewayError::invalid(format!(
                "unsupported DNS query type {value:?}"
            )));
        }
    };
    Ok((name, number))
}

fn type_name(value: u16) -> &'static str {
    match value {
        TYPE_A => "A",
        TYPE_AAAA => "AAAA",
        TYPE_CNAME => "CNAME",
        TYPE_TXT => "TXT",
        TYPE_HTTPS => "HTTPS",
        15 => "MX",
        2 => "NS",
        _ => "UNKNOWN",
    }
}

pub(crate) fn fqdn(value: &str) -> String {
    format!("{}.", value.trim().trim_end_matches('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_round_trip_and_reject_response_preserve_question() {
        let query = build_query("example.com", TYPE_HTTPS).expect("query");
        let question = parse_question(&query).expect("parse").expect("question");
        assert_eq!(question.name, "example.com.");
        let response = response_with_code(&query, 3).expect("response");
        assert_eq!(response[3] & 0x0f, 3);
        assert_eq!(
            parse_question(&response)
                .expect("parse")
                .expect("question")
                .name,
            "example.com."
        );
    }
}
