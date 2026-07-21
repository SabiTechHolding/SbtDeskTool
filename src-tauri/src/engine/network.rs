use std::time::Duration;

fn normalize_proxy(proxy_list: &str, url: &str) -> Option<String> {
    let preferred_scheme = if url.to_ascii_lowercase().starts_with("https://") {
        "https"
    } else {
        "http"
    };
    let entries: Vec<&str> = proxy_list
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();

    let selected = entries
        .iter()
        .find_map(|entry| {
            let (scheme, value) = entry.split_once('=')?;
            scheme
                .trim()
                .eq_ignore_ascii_case(preferred_scheme)
                .then_some(value.trim())
        })
        .or_else(|| {
            entries.iter().find_map(|entry| {
                let (scheme, value) = entry.split_once('=')?;
                scheme
                    .trim()
                    .eq_ignore_ascii_case("http")
                    .then_some(value.trim())
            })
        })
        .or_else(|| entries.first().copied())?
        .trim();

    let selected = selected
        .strip_prefix("PROXY ")
        .or_else(|| selected.strip_prefix("proxy "))
        .unwrap_or(selected)
        .trim();
    if selected.is_empty() || selected.eq_ignore_ascii_case("DIRECT") {
        return None;
    }
    if selected.contains("://") {
        Some(selected.to_owned())
    } else {
        Some(format!("http://{selected}"))
    }
}

#[cfg(target_os = "windows")]
fn resolve_system_proxy_blocking(url: &str) -> Result<Option<String>, String> {
    use std::{ffi::c_void, iter, os::windows::ffi::OsStrExt, ptr, slice};
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        Networking::WinHttp::{
            WinHttpCloseHandle, WinHttpGetIEProxyConfigForCurrentUser, WinHttpGetProxyForUrl,
            WinHttpOpen, WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTOPROXY_AUTO_DETECT,
            WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS, WINHTTP_AUTO_DETECT_TYPE_DHCP,
            WINHTTP_AUTO_DETECT_TYPE_DNS_A, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
            WINHTTP_PROXY_INFO,
        },
    };

    struct InternetHandle(*mut c_void);
    impl Drop for InternetHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the handle was returned by WinHTTP and is closed once here.
                unsafe { WinHttpCloseHandle(self.0) };
            }
        }
    }

    unsafe fn wide_string(value: *const u16) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let mut length = 0;
        // SAFETY: WinHTTP owns a valid null-terminated UTF-16 allocation here.
        while unsafe { *value.add(length) } != 0 {
            length += 1;
        }
        Some(String::from_utf16_lossy(unsafe {
            slice::from_raw_parts(value, length)
        }))
    }

    unsafe fn free_global(value: *mut u16) {
        if !value.is_null() {
            // SAFETY: WinHTTP documents these strings as GlobalFree allocations.
            unsafe { GlobalFree(value.cast()) };
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(iter::once(0))
            .collect()
    }

    let mut config: WINHTTP_CURRENT_USER_IE_PROXY_CONFIG = unsafe { std::mem::zeroed() };
    // SAFETY: config is a writable structure of the required type.
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) } == 0 {
        return Err(format!(
            "Unable to read current-user proxy settings: {}",
            std::io::Error::last_os_error()
        ));
    }

    let auto_config_url = unsafe { wide_string(config.lpszAutoConfigUrl) };
    let static_proxy = unsafe { wide_string(config.lpszProxy) };
    let has_auto_detect = config.fAutoDetect != 0;

    // The configuration strings remain valid only until they are freed below.
    unsafe {
        free_global(config.lpszAutoConfigUrl);
        free_global(config.lpszProxy);
        free_global(config.lpszProxyBypass);
    }

    if auto_config_url.is_none() && !has_auto_detect {
        return Ok(static_proxy.and_then(|proxy| normalize_proxy(&proxy, url)));
    }

    let agent = wide("SbtDeskTool Updater");
    // SAFETY: the agent is a valid null-terminated UTF-16 string.
    let session = InternetHandle(unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            ptr::null(),
            ptr::null(),
            0,
        )
    });
    if session.0.is_null() {
        return Ok(static_proxy.and_then(|proxy| normalize_proxy(&proxy, url)));
    }

    let auto_config_wide = auto_config_url.as_deref().map(wide);
    let mut options: WINHTTP_AUTOPROXY_OPTIONS = unsafe { std::mem::zeroed() };
    if let Some(value) = auto_config_wide.as_ref() {
        options.dwFlags = WINHTTP_AUTOPROXY_CONFIG_URL;
        options.lpszAutoConfigUrl = value.as_ptr();
    } else {
        options.dwFlags = WINHTTP_AUTOPROXY_AUTO_DETECT;
        options.dwAutoDetectFlags = WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A;
    }
    options.fAutoLogonIfChallenged = 1;

    let url_wide = wide(url);
    let mut proxy_info: WINHTTP_PROXY_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: session and all input/output buffers remain live for the call.
    let resolved = unsafe {
        WinHttpGetProxyForUrl(session.0, url_wide.as_ptr(), &mut options, &mut proxy_info)
    } != 0;
    let pac_proxy = resolved
        .then(|| unsafe { wide_string(proxy_info.lpszProxy) })
        .flatten();
    unsafe {
        free_global(proxy_info.lpszProxy);
        free_global(proxy_info.lpszProxyBypass);
    }

    if resolved {
        // A successful PAC result with no proxy means DIRECT for this URL. Do not
        // incorrectly fall back to the static proxy in that case.
        return Ok(pac_proxy.and_then(|proxy| normalize_proxy(&proxy, url)));
    }

    Ok(static_proxy.and_then(|proxy| normalize_proxy(&proxy, url)))
}

#[cfg(target_os = "windows")]
pub async fn resolve_system_proxy(url: &str) -> Result<Option<String>, String> {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || resolve_system_proxy_blocking(&url))
        .await
        .map_err(|error| format!("Windows proxy/PAC task error: {error}"))?
}

#[cfg(not(target_os = "windows"))]
pub async fn resolve_system_proxy(_url: &str) -> Result<Option<String>, String> {
    Ok(None)
}

async fn request_reqwest(
    url: &str,
    direct: bool,
    allow_invalid_certs: bool,
) -> Result<String, String> {
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
        .header(
            "User-Agent",
            "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36 SbtDeskTool",
        )
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| format!("Request error: {e}"))?
        .error_for_status()
        .map_err(|e| format!("HTTP error: {e}"))?
        .text()
        .await
        .map_err(|e| format!("Response read error: {e}"))
}

#[cfg(target_os = "windows")]
fn request_wininet_blocking(url: &str) -> Result<String, String> {
    use std::{ffi::c_void, iter, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Networking::WinInet::{
        InternetCloseHandle, InternetOpenUrlW, InternetOpenW, InternetReadFile, InternetSetOptionW,
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
            ptr::null(),
            0,
            INTERNET_FLAG_RELOAD | INTERNET_FLAG_NO_CACHE_WRITE | INTERNET_FLAG_NO_UI,
            0,
        )
    });
    if request.0.is_null() {
        return Err(last_error("request"));
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

    String::from_utf8(response).map_err(|e| format!("Windows response encoding error: {e}"))
}

#[cfg(target_os = "windows")]
async fn request_windows(url: &str) -> Result<String, String> {
    let url = url.to_owned();
    tokio::task::spawn_blocking(move || request_wininet_blocking(&url))
        .await
        .map_err(|e| format!("Windows proxy/PAC task error: {e}"))?
}

#[cfg(not(target_os = "windows"))]
async fn request_windows(_url: &str) -> Result<String, String> {
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

pub async fn request_with_strategies(url: &str, preferred: u8) -> Result<(String, u8), String> {
    let mut errors = Vec::new();
    for strategy in strategy_order(preferred) {
        let result = match strategy {
            0 => request_reqwest(url, false, false).await,
            1 => request_windows(url).await,
            2 => request_reqwest(url, false, true).await,
            3 => request_reqwest(url, true, false).await,
            4 => request_reqwest(url, true, true).await,
            _ => unreachable!(),
        };
        match result {
            Ok(body) if !body.trim().is_empty() => return Ok((body, strategy)),
            Ok(_) => errors.push(format!("strategy {strategy}: empty response")),
            Err(error) => errors.push(format!("strategy {strategy}: {error}")),
        }
    }
    Err(format!(
        "All network strategies failed: {}",
        errors.join(" | ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn selects_proxy_for_update_url() {
        assert_eq!(
            normalize_proxy(
                "http=proxy.local:8080;https=secure.local:8443",
                "https://example.com"
            ),
            Some("http://secure.local:8443".into())
        );
        assert_eq!(
            normalize_proxy("PROXY proxy.local:8080", "https://example.com"),
            Some("http://proxy.local:8080".into())
        );
        assert_eq!(normalize_proxy("DIRECT", "https://example.com"), None);
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
        assert_eq!(body, "proxy");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolves_current_user_proxy_for_update_endpoint() {
        let resolved = resolve_system_proxy_blocking(
            "https://github.com/SabiTechHolding/SbtDeskTool/releases/latest/download/latest.json",
        );
        println!("resolved updater proxy: {resolved:?}");
        assert!(
            resolved.is_ok(),
            "proxy/PAC resolution failed: {resolved:?}"
        );
    }
}
