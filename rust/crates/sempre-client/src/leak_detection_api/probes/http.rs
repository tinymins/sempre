pub(super) fn body(response: &[u8]) -> Result<&[u8], String> {
    if response.len() > 24_576 {
        return Err("HTTPS 响应超过大小限制".into());
    }
    let boundary = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("HTTPS 响应头不完整")?;
    if boundary > 16_384 {
        return Err("HTTPS 响应头超过限制".into());
    }
    let headers = std::str::from_utf8(&response[..boundary]).map_err(|_| "HTTPS 响应头无效")?;
    let mut lines = headers.lines();
    let status = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>();
    if status.len() < 2 || !["HTTP/1.0", "HTTP/1.1"].contains(&status[0]) || status[1] != "200" {
        return Err("公网 IP 服务未返回 HTTP 200；没有跟随重定向".into());
    }
    let mut length = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("HTTPS 响应头字段无效")?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err("服务返回不支持的传输编码；没有尝试从不完整正文判断公网 IP".into());
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("重复的响应长度字段".into());
            }
            length = Some(value.trim().parse::<usize>().map_err(|_| "响应长度无效")?);
        }
        if name.eq_ignore_ascii_case("content-encoding")
            && !value.trim().eq_ignore_ascii_case("identity")
        {
            return Err("服务返回不支持的正文编码".into());
        }
    }
    let body = &response[boundary + 4..];
    if body.len() > 8192 || length.is_some_and(|length| length != body.len()) {
        return Err("HTTPS 正文超过限制或长度不完整".into());
    }
    Ok(body)
}

pub(super) async fn read_response<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;
    let mut response = Vec::new();
    loop {
        let mut chunk = [0_u8; 2048];
        let count = reader
            .read(&mut chunk)
            .await
            .map_err(|_| "HTTPS 响应读取失败")?;
        if count == 0 {
            return Ok(response);
        }
        response.extend_from_slice(&chunk[..count]);
        if response.len() > 24_576 {
            return Err("HTTPS 响应超过大小限制".into());
        }
        if let Some(boundary) = response.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers =
                std::str::from_utf8(&response[..boundary]).map_err(|_| "HTTPS 响应头无效")?;
            for line in headers.lines().skip(1) {
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("transfer-encoding") {
                        return Err("服务返回不支持的传输编码".into());
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        let length = value.trim().parse::<usize>().map_err(|_| "响应长度无效")?;
                        if length > 8192 {
                            return Err("HTTPS 正文超过限制".into());
                        }
                        if response.len() >= boundary + 4 + length {
                            return Ok(response);
                        }
                    }
                }
            }
        }
    }
}
