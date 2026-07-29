use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::IpAddr;

pub fn is_blocked(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked(&IpAddr::V4(v4));
            }
            let octets = v6.octets();
            v6.is_loopback()
                || v6.is_unspecified()
                || (octets[0] & 0xfe == 0xfc)
                || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
        }
    }
}

#[derive(Clone)]
pub struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let n = name.as_str().to_string();
        Box::pin(async move {
            let addrs = tokio::task::spawn_blocking(move || {
                std::net::ToSocketAddrs::to_socket_addrs(&(n.as_str(), 0))
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            let mut valid = Vec::new();
            for a in addrs {
                let ip = a.ip();
                if is_blocked(&ip) {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "SSRF attempt blocked",
                    )) as _);
                }
                valid.push(a);
            }
            Ok(Box::new(valid.into_iter()) as Addrs)
        })
    }
}
