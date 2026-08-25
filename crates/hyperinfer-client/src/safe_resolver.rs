use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::{IpAddr, ToSocketAddrs};

#[derive(Clone, Default)]
pub struct SafeResolver;

impl Resolve for SafeResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let name_str = name.as_str().to_string();

        Box::pin(async move {
            let addrs = tokio::task::spawn_blocking(move || {
                let host_port = format!("{}:0", name_str);
                host_port.to_socket_addrs()
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

            let mut valid_addrs = Vec::new();
            for addr in addrs {
                if !is_blocked(addr.ip()) {
                    valid_addrs.push(addr);
                }
            }

            if valid_addrs.is_empty() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Blocked IP",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }

            let result: Box<dyn Iterator<Item = std::net::SocketAddr> + Send> =
                Box::new(valid_addrs.into_iter());
            let addrs_trait: Addrs = Box::new(result);
            Ok(addrs_trait)
        })
    }
}

pub fn is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 10
                || (o[0] == 172 && o[1] >= 16 && o[1] <= 31)
                || (o[0] == 192 && o[1] == 168)
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || o[0] == 0
                || (o[0] == 169 && o[1] == 254)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                is_blocked(IpAddr::V4(v4))
            } else {
                if v6.is_loopback() || v6.is_unspecified() {
                    return true;
                }
                let octets = v6.octets();
                if octets[0] & 0xfe == 0xfc {
                    return true;
                }
                if octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80 {
                    return true;
                }
                false
            }
        }
    }
}
