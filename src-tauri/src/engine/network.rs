use std::time::Duration;

const JSON_ACCEPT: &str = "application/json, text/plain, */*";
const BINARY_ACCEPT: &str = "application/octet-stream";
const HTTP_STATUS_ERROR_PREFIX: &str = "HTTP status error: ";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 SbtDeskTool";

fn is_http_status_error(error: &str) -> bool {
    error.starts_with(HTTP_STATUS_ERROR_PREFIX)
}

fn accept_header(url: &str) -> &'static str {
    let is_github_release_asset = url::Url::parse(url).is_ok_and(|parsed| {
        parsed.host_str() == Some("api.github.com") && parsed.path().contains("/releases/assets/")
    });
    if is_github_release_asset {
        BINARY_ACCEPT
    } else {
        JSON_ACCEPT
    }
}

async fn request_reqwest_bytes(
    url: &str,
    direct: bool,
    allow_invalid_certs: bool,
) -> Result<Vec<u8>, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .danger_accept_invalid_certs(allow_invalid_certs);
    if direct {
        builder = builder.no_proxy();
    }
    let client = builder
        .build()
        .map_err(|e| format!("Client build error: {e}"))?;
    client
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", accept_header(url))
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Request error: {e}"))?
        .error_for_status()
        .map_err(|e| format!("{HTTP_STATUS_ERROR_PREFIX}{e}"))?
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|e| format!("Response read error: {e}"))
}

#[cfg(target_os = "windows")]
fn request_wininet_blocking(url: &str) -> Result<Vec<u8>, String> {
    use std::{ffi::c_void, iter, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Networking::WinInet::{
        HttpQueryInfoW, InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile,
        InternetSetOptionW, HTTP_QUERY_FLAG_NUMBER, HTTP_QUERY_STATUS_CODE,
        INTERNET_FLAG_NO_CACHE_WRITE, INTERNET_FLAG_NO_UI, INTERNET_FLAG_RELOAD,
        INTERNET_OPEN_TYPE_PRECONFIG, INTERNET_OPTION_CONNECT_TIMEOUT,
        INTERNET_OPTION_RECEIVE_TIMEOUT, INTERNET_OPTION_SEND_TIMEOUT,
    };

    struct InternetHandle(*mut c_void);

    impl Drop for InternetHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the handle was returned by WinINet and is closed exactly once here.
                unsafe { InternetCloseHandle(self.0) };
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(iter::once(0))
            .collect()
    }

    fn last_error(action: &str) -> String {
        format!(
            "Windows proxy/PAC {action} error: {}",
            std::io::Error::last_os_error()
        )
    }

    let agent = wide("SbtDeskTool");
    let request_headers = wide(&format!(
        "Accept: {}\r\nX-GitHub-Api-Version: 2022-11-28\r\n",
        accept_header(url)
    ));
    let url = wide(url);
    // SAFETY: all strings are valid, null-terminated UTF-16 buffers that live for the call.
    let session = InternetHandle(unsafe {
        InternetOpenW(
            agent.as_ptr(),
            INTERNET_OPEN_TYPE_PRECONFIG,
            ptr::null(),
            ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Err(last_error("session"));
    }

    let timeout_ms: u32 = 20_000;
    for option in [
        INTERNET_OPTION_CONNECT_TIMEOUT,
        INTERNET_OPTION_SEND_TIMEOUT,
        INTERNET_OPTION_RECEIVE_TIMEOUT,
    ] {
        // SAFETY: timeout_ms is a live u32 buffer of the size passed to WinINet.
        let success = unsafe {
            InternetSetOptionW(
                session.0,
                option,
                (&timeout_ms as *const u32).cast(),
                size_of::<u32>() as u32,
            )
        };
        if success == 0 {
            return Err(last_error("timeout configuration"));
        }
    }

    // PRECONFIG evaluates the current user's Windows proxy and PAC settings. It does not
    // require elevation and preserves the behaviour of the original desktop application.
    let request = InternetHandle(unsafe {
        InternetOpenUrlW(
            session.0,
            url.as_ptr(),
            request_headers.as_ptr(),
            (request_headers.len() - 1) as u32,
            INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_UI,
            0,
        )
    });
    if request.0.is_null() {
        return Err(last_error("request"));
    }

    let mut status_code = 0_u32;
    let mut status_size = size_of::<u32>() as u32;
    let mut status_index = 0_u32;
    // SAFETY: all output pointers reference writable u32 values for the declared size.
    let status_ok = unsafe {
        HttpQueryInfoW(
            request.0,
            HTTP_QUERY_STATUS_CODE | HTTP_QUERY_FLAG_NUMBER,
            (&mut status_code as *mut u32).cast(),
            &mut status_size,
            &mut status_index,
        )
    };
    if status_ok == 0 {
        return Err(last_error("HTTP status query"));
    }
    if !(200..300).contains(&status_code) {
        return Err(format!(
            "{HTTP_STATUS_ERROR_PREFIX} Windows proxy/PAC returned status {status_code}"
        ));
    }

    let mut response = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let mut read = 0_u32;
        // SAFETY: buffer and read are writable for their declared sizes; request is live.
        let success = unsafe {
            InternetReadFile(
                request.0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut read,
            )
        };
        if success == 0 {
            return Err(last_error("response read"));
        }
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..read as usize]);
    }

    Ok(response)
}

#[cfg(target_os = "windows")]
async fn request_windows(url: &str) -> Result<Vec<u8>, String> {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || request_wininet_blocking(&url))
        .await
        .map_err(|e| format!("Windows proxy/PAC task error: {e}"))?
}

#[cfg(not(target_os = "windows"))]
async fn request_windows(_url: &str) -> Result<Vec<u8>, String> {
    Err("Windows proxy/PAC fallback is unavailable".into())
}

fn strategy_order(preferred: u8) -> Vec<u8> {
    let preferred = preferred.min(4);
    let supported: &[u8] = if cfg!(target_os = "windows") {
        &[0, 1, 2, 3, 4]
    } else {
        &[0, 2, 3, 4]
    };
    let first = if supported.contains(&preferred) {
        preferred
    } else {
        0
    };
    let mut order = vec![first];
    for &strategy in supported {
        if strategy != first {
            order.push(strategy);
        }
    }
    order
}

pub async fn request_bytes_with_strategies(
    url: &str,
    preferred: u8,
) -> Result<(Vec<u8>, u8), String> {
    let mut errors = Vec::new();
    for strategy in strategy_order(preferred) {
        let result = match strategy {
            0 => request_reqwest_bytes(url, false, false).await,
            1 => request_windows(url).await,
            2 => request_reqwest_bytes(url, false, true).await,
            3 => request_reqwest_bytes(url, true, false).await,
            4 => request_reqwest_bytes(url, true, true).await,
            _ => unreachable!(),
        };
        match result {
            Ok(body) if !body.is_empty() => return Ok((body, strategy)),
            Ok(_) => errors.push(format!("strategy {strategy}: empty response")),
            Err(error) if is_http_status_error(&error) => {
                return Err(format!(
                    "Request received an HTTP error with strategy {strategy}: {error}"
                ));
            }
            Err(error) => errors.push(format!("strategy {strategy}: {error}")),
        }
    }
    Err(format!(
        "All network strategies failed: {}",
        errors.join(" | ")
    ))
}

pub async fn request_with_strategies(url: &str, preferred: u8) -> Result<(String, u8), String> {
    let (bytes, strategy) = request_bytes_with_strategies(url, preferred).await?;
    let body =
        String::from_utf8(bytes).map_err(|error| format!("Response encoding error: {error}"))?;
    Ok((body, strategy))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn http_status_errors_are_terminal() {
        assert!(is_http_status_error("HTTP status error: 404 Not Found"));
        assert!(is_http_status_error(
            "HTTP status error: Windows proxy/PAC returned status 503"
        ));
        assert!(!is_http_status_error("Request error: connection reset"));
        assert!(!is_http_status_error("Response read error: unexpected EOF"));
    }

    #[test]
    fn preferred_strategy_runs_first_without_duplicates() {
        if cfg!(target_os = "windows") {
            assert_eq!(strategy_order(2), vec![2, 0, 1, 3, 4]);
            assert_eq!(strategy_order(4), vec![4, 0, 1, 2, 3]);
        } else {
            assert_eq!(strategy_order(2), vec![2, 0, 3, 4]);
            assert_eq!(strategy_order(4), vec![4, 0, 2, 3]);
            assert_eq!(strategy_order(1), vec![0, 2, 3, 4]);
        }
    }

    #[test]
    fn github_release_assets_request_binary_content() {
        assert_eq!(
            accept_header("https://api.github.com/repos/example/app/releases/assets/123"),
            BINARY_ACCEPT
        );
        assert_eq!(
            accept_header("https://github.com/example/app/releases/latest/download/latest.json"),
            JSON_ACCEPT
        );
        assert_eq!(
            accept_header("https://api.github.com.example.test/releases/assets/123"),
            JSON_ACCEPT
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn wininet_reads_a_response_with_current_user_network_settings() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("read test server address");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept WinINet request");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).expect("read HTTP request");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 6\r\nConnection: close\r\n\r\nproxy")
                .expect("write HTTP response");
        });

        let body = request_wininet_blocking(&format!("http://{address}/translate"))
            .expect("request through WinINet");
        server.join().expect("join test server");
        assert_eq!(body, b"proxy");
    }
}
